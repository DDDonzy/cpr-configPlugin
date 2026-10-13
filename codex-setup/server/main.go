package main

import (
	"bytes"
	"crypto/sha256"
	_ "embed"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"strings"
	"time"
)

//go:embed assets/codex-setup.bat
var embeddedBat []byte

const maxFile = 1 << 20
const version = "1.0.0"

type BaseModel struct {
	DisplayName string   `json:"display_name"`
	Reasoning   []string `json:"reasoning_levels"`
	Context     int64    `json:"context_window"`
	Output      *int64   `json:"max_output_tokens,omitempty"`
	Inputs      []string `json:"input_modalities"`
}
type Entry struct {
	ID    string
	Model BaseModel
}
type Policy struct {
	DefaultModel        string `json:"default_model"`
	DefaultEffort       string `json:"default_reasoning_effort"`
	Websockets          bool   `json:"supports_websockets"`
	RemoteCompaction    string `json:"remote_compaction"`
	ManagedActor        bool   `json:"managed_actor_header"`
	ImageGeneration     bool   `json:"image_generation"`
	Goals               bool   `json:"goals"`
	StreamIdleTimeoutMS int64  `json:"stream_idle_timeout_ms"`
}
type App struct {
	ModelsPath, PolicyPath, BatPath, Upstream string
	Client                                    *http.Client
	Slots                                     chan struct{}
}

func readBounded(path string, limit int64) ([]byte, error) {
	f, e := os.Open(path)
	if e != nil {
		return nil, e
	}
	defer f.Close()
	b, e := io.ReadAll(io.LimitReader(f, limit+1))
	if e != nil {
		return nil, e
	}
	if int64(len(b)) > limit {
		return nil, errors.New("file too large")
	}
	return b, nil
}
func validText(s string, n int) bool {
	return len(s) > 0 && len(s) <= n && strings.TrimSpace(s) == s && !strings.ContainsAny(s, "\r\n\x00")
}
func member(xs []string, s string) bool {
	for _, x := range xs {
		if x == s {
			return true
		}
	}
	return false
}
func readModels(data []byte) ([]Entry, error) {
	d := json.NewDecoder(bytes.NewReader(data))
	t, e := d.Token()
	if e != nil || t != json.Delim('{') {
		return nil, errors.New("invalid catalog")
	}
	out := []Entry{}
	seen := map[string]bool{}
	for d.More() {
		tok, e := d.Token()
		if e != nil {
			return nil, e
		}
		id, ok := tok.(string)
		if !ok || !validText(id, 256) || strings.ContainsAny(id, " \t*") || seen[id] {
			return nil, errors.New("invalid or duplicate model id")
		}
		seen[id] = true
		var raw json.RawMessage
		if e = d.Decode(&raw); e != nil {
			return nil, e
		}
		md := json.NewDecoder(bytes.NewReader(raw))
		md.DisallowUnknownFields()
		var m BaseModel
		if e = md.Decode(&m); e != nil {
			return nil, e
		}
		if !validText(m.DisplayName, 512) || m.Context <= 0 || m.Context > 9007199254740991 || (m.Output != nil && (*m.Output <= 0 || *m.Output > 9007199254740991)) {
			return nil, errors.New("invalid model limits")
		}
		if len(m.Inputs) == 0 || len(m.Reasoning) > 16 {
			return nil, errors.New("invalid model capabilities")
		}
		unique := map[string]bool{}
		for _, s := range m.Reasoning {
			if !member([]string{"none", "minimal", "low", "medium", "high", "xhigh", "max"}, s) || unique[s] {
				return nil, errors.New("invalid effort")
			}
			unique[s] = true
		}
		unique = map[string]bool{}
		for _, s := range m.Inputs {
			if !member([]string{"text", "image", "audio"}, s) || unique[s] {
				return nil, errors.New("unsupported input modality")
			}
			unique[s] = true
		}
		out = append(out, Entry{id, m})
		if len(out) > 2000 {
			return nil, errors.New("too many models")
		}
	}
	if _, e = d.Token(); e != nil {
		return nil, e
	}
	var extra interface{}
	if d.Decode(&extra) != io.EOF {
		return nil, errors.New("trailing data")
	}
	if len(out) == 0 {
		return nil, errors.New("empty catalog")
	}
	return out, nil
}
func codexModels(entries []Entry) map[string]interface{} {
	models := []interface{}{}
	desc := map[string]string{"none": "No reasoning effort", "minimal": "Minimal reasoning effort", "low": "Low reasoning effort", "medium": "Medium reasoning effort", "high": "High reasoning effort", "xhigh": "Extra-high reasoning effort", "max": "Maximum reasoning effort", "ultra": "Ultra reasoning effort"}
	for i, e := range entries {
		levels := []interface{}{}
		for _, r := range e.Model.Reasoning {
			levels = append(levels, map[string]string{"effort": r, "description": desc[r]})
		}
		levels = append(levels, map[string]string{"effort": "ultra", "description": desc["ultra"]})
		models = append(models, map[string]interface{}{"slug": e.ID, "display_name": e.Model.DisplayName, "supported_reasoning_levels": levels, "shell_type": "unified_exec", "visibility": "list", "supported_in_api": true, "priority": i, "base_instructions": "You are a coding assistant. Help the user complete their task using the available tools.", "support_verbosity": true, "truncation_policy": map[string]interface{}{"mode": "tokens", "limit": 128000}, "experimental_supported_tools": []string{}, "context_window": e.Model.Context, "max_context_window": e.Model.Context, "input_modalities": e.Model.Inputs, "supports_reasoning_summary_parameter": true})
	}
	return map[string]interface{}{"models": models}
}
func (a *App) authenticate(r *http.Request) (map[string]bool, int) {
	h := r.Header.Values("Authorization")
	if len(h) != 1 || !strings.HasPrefix(h[0], "Bearer ") {
		return nil, 401
	}
	key := strings.TrimPrefix(h[0], "Bearer ")
	if len(key) < 8 || len(key) > 512 {
		return nil, 401
	}
	for _, c := range key {
		if c < 33 || c > 126 {
			return nil, 401
		}
	}
	req, e := http.NewRequestWithContext(r.Context(), http.MethodGet, a.Upstream+"/v1/models", nil)
	if e != nil {
		return nil, 503
	}
	req.Header.Set("Authorization", h[0])
	req.Header.Set("User-Agent", "codex-setup/"+version)
	res, e := a.Client.Do(req)
	if e != nil {
		return nil, 503
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		if res.StatusCode == 401 || res.StatusCode == 403 {
			return nil, res.StatusCode
		}
		return nil, 503
	}
	b, e := io.ReadAll(io.LimitReader(res.Body, maxFile+1))
	if e != nil || len(b) > maxFile {
		return nil, 503
	}
	var list struct {
		Data []struct {
			ID string `json:"id"`
		} `json:"data"`
	}
	if json.Unmarshal(b, &list) != nil || list.Data == nil {
		return nil, 503
	}
	allowed := map[string]bool{}
	for _, m := range list.Data {
		allowed[m.ID] = true
	}
	return allowed, 200
}
func fail(w http.ResponseWriter, status int, code string) {
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(map[string]string{"error": code})
}
func (a *App) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Header().Set("Cache-Control", "no-store")
	if r.Method != http.MethodGet {
		w.Header().Set("Allow", "GET")
		fail(w, 405, "method_not_allowed")
		return
	}
	if r.URL.RawQuery != "" {
		fail(w, 400, "query_not_allowed")
		return
	}
	if r.URL.Path == "/healthz" {
		w.WriteHeader(204)
		return
	}
	if r.URL.Path == "/codex-setup.bat" {
		b, e := embeddedBat, error(nil)
		if a.BatPath != "" {
			b, e = readBounded(a.BatPath, 2<<20)
		}
		if e != nil {
			fail(w, 503, "download_unavailable")
			return
		}
		w.Header().Set("Content-Type", "application/octet-stream")
		w.Header().Set("Content-Disposition", `attachment; filename="codex-setup.bat"`)
		_, _ = w.Write(b)
		return
	}
	if r.URL.Path != "/api/codex/setup" {
		fail(w, 404, "not_found")
		return
	}
	select {
	case a.Slots <- struct{}{}:
		defer func() { <-a.Slots }()
	default:
		fail(w, 503, "busy")
		return
	}
	allowed, status := a.authenticate(r)
	if status != 200 {
		code := "authentication_failed"
		if status == 503 {
			code = "gateway_unavailable"
		}
		fail(w, status, code)
		return
	}
	data, e := readBounded(a.ModelsPath, maxFile)
	if e != nil {
		fail(w, 503, "catalog_unavailable")
		return
	}
	entries, e := readModels(data)
	if e != nil {
		fail(w, 503, "catalog_invalid")
		return
	}
	pb, e := readBounded(a.PolicyPath, 16384)
	if e != nil {
		fail(w, 503, "policy_unavailable")
		return
	}
	pd := json.NewDecoder(bytes.NewReader(pb))
	pd.DisallowUnknownFields()
	var policy Policy
	if pd.Decode(&policy) != nil || !member([]string{"unsupported", "v2"}, policy.RemoteCompaction) {
		fail(w, 503, "policy_invalid")
		return
	}
	// The maintained catalog is metadata, not an access grant. Choose a default that CPR advertises to this key.
	chosen := -1
	for i, e := range entries {
		if allowed[e.ID] && member(e.Model.Inputs, "text") && (policy.DefaultModel == "" || policy.DefaultModel == e.ID) {
			chosen = i
			break
		}
	}
	if chosen < 0 {
		fail(w, 409, "no_accessible_default_model")
		return
	}
	m := entries[chosen]
	effort := policy.DefaultEffort
	if effort != "" && !member(m.Model.Reasoning, effort) {
		fail(w, 503, "default_effort_not_supported")
		return
	}
	if effort == "" {
		if member(m.Model.Reasoning, "high") {
			effort = "high"
		} else if len(m.Model.Reasoning) > 0 {
			effort = m.Model.Reasoning[0]
		}
	}
	sum := sha256.Sum256(append(append([]byte{}, data...), pb...))
	if policy.StreamIdleTimeoutMS == 0 {
		policy.StreamIdleTimeoutMS = 900000
	}
	if policy.StreamIdleTimeoutMS < 1000 || policy.StreamIdleTimeoutMS > 3600000 {
		fail(w, 503, "policy_invalid")
		return
	}
	config := map[string]interface{}{"model": m.ID, "review_model": m.ID, "service_tier": "default", "stream_idle_timeout_ms": policy.StreamIdleTimeoutMS, "supports_websockets": policy.Websockets, "remote_compaction": policy.RemoteCompaction, "managed_actor_header": policy.ManagedActor, "image_generation": policy.ImageGeneration, "goals": policy.Goals}
	if effort != "" {
		config["model_reasoning_effort"] = effort
	}
	result := map[string]interface{}{"schema_version": 2, "generator_version": version, "version": hex.EncodeToString(sum[:]), "models_json": codexModels(entries), "config": config}
	b, e := json.MarshalIndent(result, "", "  ")
	if e != nil || len(b) > 4<<20 {
		fail(w, 503, "bundle_too_large")
		return
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	_, _ = w.Write(b)
}
func main() {
	listen := flag.String("listen", "127.0.0.1:18941", "listen address")
	models := flag.String("models", "/opt/codex-proxy-rs/.runtime/data/model-capabilities/models.common.json", "catalog")
	policy := flag.String("policy", "/opt/codex-setup/client-policy.json", "policy")
	bat := flag.String("bat", "", "optional BAT override (default: embedded artifact)")
	upstream := flag.String("upstream", "http://127.0.0.1:8080", "CPR authentication endpoint")
	flag.Parse()
	if !strings.HasPrefix(*listen, "127.0.0.1:") || !strings.HasPrefix(*upstream, "http://127.0.0.1:") {
		log.Fatal("loopback addresses required")
	}
	app := &App{*models, *policy, *bat, strings.TrimRight(*upstream, "/"), &http.Client{Timeout: 10 * time.Second, CheckRedirect: func(_ *http.Request, _ []*http.Request) error { return http.ErrUseLastResponse }, Transport: &http.Transport{Proxy: nil, MaxIdleConnsPerHost: 8}}, make(chan struct{}, 8)}
	srv := &http.Server{Addr: *listen, Handler: app, ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 10 * time.Second, WriteTimeout: 20 * time.Second, IdleTimeout: 30 * time.Second, MaxHeaderBytes: 8192}
	fmt.Println("codex-setup " + version + " listening on " + *listen)
	log.Fatal(srv.ListenAndServe())
}
