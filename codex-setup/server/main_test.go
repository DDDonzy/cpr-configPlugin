package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

const fixture = `{"model-a":{"display_name":"Model A","reasoning_levels":["low","high"],"context_window":128000,"max_output_tokens":8000,"input_modalities":["text","image"]},"model-b":{"display_name":"Model B","reasoning_levels":[],"context_window":64000,"input_modalities":["text"]}}`

func appForTest(t *testing.T, status int) (*App, *int) {
	t.Helper()
	calls := 0
	up := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		if r.URL.Path != "/v1/models" || r.Header.Get("Authorization") != "Bearer sk_test_key" {
			t.Error("invalid authentication request")
		}
		w.WriteHeader(status)
		if status == 200 {
			_, _ = w.Write([]byte(`{"data":[{"id":"model-a"}]}`))
		}
	}))
	t.Cleanup(up.Close)
	dir := t.TempDir()
	m := filepath.Join(dir, "models.json")
	p := filepath.Join(dir, "policy.json")
	b := filepath.Join(dir, "setup.bat")
	_ = os.WriteFile(m, []byte(fixture), 0600)
	_ = os.WriteFile(p, []byte(`{"default_model":"","default_reasoning_effort":"","supports_websockets":false,"remote_compaction":"v2","managed_actor_header":true,"image_generation":true,"goals":true}`), 0600)
	_ = os.WriteFile(b, []byte("@echo off\r\n"), 0600)
	return &App{m, p, b, up.URL, &http.Client{Timeout: time.Second, CheckRedirect: func(_ *http.Request, _ []*http.Request) error { return http.ErrUseLastResponse }}, make(chan struct{}, 8)}, &calls
}
func request(a *App, key, path string) *httptest.ResponseRecorder {
	r := httptest.NewRequest("GET", path, nil)
	if key != "" {
		r.Header.Set("Authorization", key)
	}
	w := httptest.NewRecorder()
	a.ServeHTTP(w, r)
	return w
}
func TestAuthRequired(t *testing.T) {
	a, c := appForTest(t, 200)
	for _, k := range []string{"", "Basic foo", "Bearer x", "Bearer bad key"} {
		if w := request(a, k, "/api/codex/setup"); w.Code != 401 {
			t.Fatal(w.Code)
		}
	}
	if *c != 0 {
		t.Fatal("unauthenticated upstream request")
	}
}
func TestAuthFailure(t *testing.T) {
	for _, code := range []int{401, 403, 500} {
		a, _ := appForTest(t, code)
		w := request(a, "Bearer sk_test_key", "/api/codex/setup")
		want := code
		if code == 500 {
			want = 503
		}
		if w.Code != want {
			t.Fatal(w.Code)
		}
	}
}
func TestBundleAndRefresh(t *testing.T) {
	a, c := appForTest(t, 200)
	w := request(a, "Bearer sk_test_key", "/api/codex/setup")
	if w.Code != 200 {
		t.Fatal(w.Code, w.Body.String())
	}
	var v map[string]interface{}
	_ = json.Unmarshal(w.Body.Bytes(), &v)
	if strings.Contains(w.Body.String(), "sk_test_key") || strings.Contains(w.Body.String(), a.ModelsPath) {
		t.Fatal("private information exposed")
	}
	if v["config"].(map[string]interface{})["model_reasoning_effort"] != "high" {
		t.Fatal("effort default")
	}
	models := v["models_json"].(map[string]interface{})["models"].([]interface{})
	if len(models) != 2 || models[0].(map[string]interface{})["slug"] != "model-a" {
		t.Fatal("model order")
	}
	old := v["version"]
	_ = os.WriteFile(a.ModelsPath, []byte(strings.Replace(fixture, "Model A", "Updated A", 1)), 0600)
	w = request(a, "Bearer sk_test_key", "/api/codex/setup")
	_ = json.Unmarshal(w.Body.Bytes(), &v)
	if v["version"] == old || *c != 2 {
		t.Fatal("no live refresh")
	}
	if w.Header().Get("Cache-Control") != "no-store" {
		t.Fatal("cache header")
	}
}
func TestCatalogValidation(t *testing.T) {
	for _, value := range []string{`{}`, `{"a":{}}`, fixture + `{}`, strings.Replace(fixture, `"context_window":128000`, `"context_window":-1`, 1), strings.Replace(fixture, `"text","image"`, `"text","video"`, 1), strings.Replace(fixture, `"low","high"`, `"high","high"`, 1), strings.Replace(fixture, `"model-b"`, `"model-a"`, 1), strings.Replace(fixture, `"max_output_tokens":8000`, `"oops":8000`, 1)} {
		if _, e := readModels([]byte(value)); e == nil {
			t.Fatal("accepted invalid catalog", value)
		}
	}
}
func TestCatalogOrder(t *testing.T) {
	es, e := readModels([]byte(fixture))
	if e != nil || len(es) != 2 || es[1].ID != "model-b" {
		t.Fatal(e)
	}
}
func TestBadPolicyAndCatalog(t *testing.T) {
	a, _ := appForTest(t, 200)
	_ = os.WriteFile(a.ModelsPath, []byte(`{}`), 0600)
	if request(a, "Bearer sk_test_key", "/api/codex/setup").Code != 503 {
		t.Fatal("invalid catalog accepted")
	}
	_ = os.WriteFile(a.ModelsPath, []byte(fixture), 0600)
	_ = os.WriteFile(a.PolicyPath, []byte(`{"remote_compaction":"v2","default_model":"model-b"}`), 0600)
	if request(a, "Bearer sk_test_key", "/api/codex/setup").Code != 409 {
		t.Fatal("unavailable default selected")
	}
	_ = os.WriteFile(a.PolicyPath, []byte(`{"remote_compaction":"v2","default_reasoning_effort":"max"}`), 0600)
	if request(a, "Bearer sk_test_key", "/api/codex/setup").Code != 503 {
		t.Fatal("unsupported effort selected")
	}
}
func TestRoutes(t *testing.T) {
	a, c := appForTest(t, 200)
	if request(a, "", "/api/codex/setup?key=bad").Code != 400 {
		t.Fatal("query accepted")
	}
	if request(a, "", "/other").Code != 404 {
		t.Fatal("unknown route")
	}
	if request(a, "", "/healthz").Code != 204 {
		t.Fatal("health")
	}
	w := request(a, "", "/codex-setup.bat")
	if w.Code != 200 || !strings.Contains(w.Header().Get("Content-Disposition"), "attachment") {
		t.Fatal("download")
	}
	r := httptest.NewRequest("POST", "/api/codex/setup", nil)
	w = httptest.NewRecorder()
	a.ServeHTTP(w, r)
	if w.Code != 405 || *c != 0 {
		t.Fatal("method validation")
	}
}
func TestBusy(t *testing.T) {
	a, _ := appForTest(t, 200)
	for i := 0; i < cap(a.Slots); i++ {
		a.Slots <- struct{}{}
	}
	if request(a, "Bearer sk_test_key", "/api/codex/setup").Code != 503 {
		t.Fatal("unbounded concurrency")
	}
}
func TestDuplicateAuthorization(t *testing.T) {
	a, c := appForTest(t, 200)
	r := httptest.NewRequest("GET", "/api/codex/setup", nil)
	r.Header.Add("Authorization", "Bearer sk_test_key")
	r.Header.Add("Authorization", "Bearer sk_test_key")
	w := httptest.NewRecorder()
	a.ServeHTTP(w, r)
	if w.Code != 401 || *c != 0 {
		t.Fatal("multiple authorization accepted")
	}
}
func TestUltraAndTimeoutDefaults(t *testing.T) {
	a, _ := appForTest(t, 200)
	w := request(a, "Bearer sk_test_key", "/api/codex/setup")
	if w.Code != 200 {
		t.Fatal(w.Code)
	}
	var v map[string]interface{}
	if json.Unmarshal(w.Body.Bytes(), &v) != nil {
		t.Fatal("invalid JSON")
	}
	if v["schema_version"] != float64(2) {
		t.Fatal("schema version")
	}
	if v["config"].(map[string]interface{})["stream_idle_timeout_ms"] != float64(900000) {
		t.Fatal("idle timeout")
	}
	for _, m := range v["models_json"].(map[string]interface{})["models"].([]interface{}) {
		levels := m.(map[string]interface{})["supported_reasoning_levels"].([]interface{})
		n := 0
		for _, level := range levels {
			if level.(map[string]interface{})["effort"] == "ultra" {
				n++
			}
		}
		if n != 1 {
			t.Fatal("model must contain ultra exactly once")
		}
	}
	if _, err := readModels([]byte(strings.Replace(fixture, `"low","high"`, `"high","ultra"`, 1))); err == nil {
		t.Fatal("Codex-only ultra polluted base schema")
	}
}
func TestInvalidTimeout(t *testing.T) {
	a, _ := appForTest(t, 200)
	if err := os.WriteFile(a.PolicyPath, []byte(`{"remote_compaction":"v2","stream_idle_timeout_ms":-1}`), 0600); err != nil {
		t.Fatal(err)
	}
	if request(a, "Bearer sk_test_key", "/api/codex/setup").Code != 503 {
		t.Fatal("invalid timeout accepted")
	}
}
