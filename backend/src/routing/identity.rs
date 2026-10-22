//! 从原生账号列表按允许清单投影可识别的账号身份；不转发令牌、密钥或任意未知字段

use serde::Serialize;
use serde_json::Value;

const MAX_EMAIL: usize = 320;
const MAX_LABEL: usize = 256;
const MAX_NOTES: usize = 500;
const MAX_PLAN: usize = 64;
const MAX_GROUPS: usize = 32;
const MAX_GROUP_NAME: usize = 128;
const MAX_TIMESTAMP: usize = 64;
const MAX_BASE_URL: usize = 2048;
const MAX_HOST: usize = 255;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountGroupRef {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub enabled: bool,
}

/// 页面用于区分账号的非敏感字段；全部来自宿主管理接口的公开投影
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountIdentity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_type_display: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<AccountGroupRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
}

/// API Key 账号的上游来源，只保留 scheme://host[:port]；路径可能携带中转密钥，因此不展示
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamAddress {
    pub url: String,
    pub host: String,
    pub loopback: bool,
}

fn clean(value: &str, max_chars: usize) -> Option<String> {
    let text: String = value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        out.push('…');
    }
    Some(out)
}

fn text(item: &Value, key: &str, max_chars: usize) -> Option<String> {
    item.get(key)
        .and_then(Value::as_str)
        .and_then(|s| clean(s, max_chars))
}

fn valid_color(value: &str) -> bool {
    let hex = value.strip_prefix('#').unwrap_or("");
    (hex.len() == 6 || hex.len() == 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_group_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn valid_email(value: &str) -> bool {
    value.len() <= MAX_EMAIL
        && value.contains('@')
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn valid_kind(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

fn timestamp(value: Option<&Value>) -> Option<String> {
    let value = value?.as_str()?.trim();
    (!value.is_empty()
        && value.len() <= MAX_TIMESTAMP
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b':' | b'.' | b'+')))
    .then(|| value.to_owned())
}

/// 只读取显式列出的字段；令牌、刷新状态、配额明细等其他字段即使存在也不会进入结果
pub fn project(item: &Value) -> AccountIdentity {
    let groups = item
        .get("groups")
        .and_then(Value::as_array)
        .map(|groups| {
            groups
                .iter()
                .filter_map(|group| {
                    let id = group.get("id")?.as_str()?;
                    if !valid_group_id(id) {
                        return None;
                    }
                    Some(AccountGroupRef {
                        id: id.to_owned(),
                        name: text(group, "name", MAX_GROUP_NAME)?,
                        color: group
                            .get("color")
                            .and_then(Value::as_str)
                            .filter(|c| valid_color(c))
                            .map(str::to_owned),
                        enabled: group
                            .get("enabled")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                    })
                })
                .take(MAX_GROUPS)
                .collect()
        })
        .unwrap_or_default();
    AccountIdentity {
        email: item
            .get("email")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|e| valid_email(e))
            .map(str::to_owned),
        label: text(item, "label", MAX_LABEL),
        notes: text(item, "notes", MAX_NOTES),
        authentication_kind: item
            .get("authenticationKind")
            .and_then(Value::as_str)
            .filter(|k| valid_kind(k))
            .map(str::to_owned),
        plan_type: text(item, "planType", MAX_PLAN),
        plan_type_display: text(item, "planTypeDisplay", MAX_PLAN),
        groups,
        last_used_at: timestamp(item.get("usage").and_then(|u| u.get("lastUsedAt"))),
    }
}

/// 与 CPR 原生账号页一致：API Key 账号用名称，OAuth 账号用邮箱
pub fn display_label(name: &str, identity: &AccountIdentity) -> String {
    let name = name.trim();
    if identity.authentication_kind.as_deref() == Some("api_key") && !name.is_empty() {
        return name.to_owned();
    }
    identity
        .email
        .clone()
        .or_else(|| identity.label.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| name.to_owned())
}

fn valid_host_port(authority: &str) -> Option<&str> {
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        let host = &authority[..end + 2];
        let tail = &rest[end + 1..];
        if !rest[..end]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.')
        {
            return None;
        }
        (host, tail.strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if host.is_empty() || authority.len() > MAX_HOST {
        return None;
    }
    if !host.starts_with('[')
        && !host
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return None;
    }
    if let Some(port) = port {
        if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    } else if authority.starts_with('[') && !authority.ends_with(']') {
        return None;
    }
    Some(host)
}

/// 仅返回 scheme://host[:port]；含认证信息或不规范的地址返回 None，而不是猜测展示
pub fn sanitize_base_url(raw: &str) -> Option<UpstreamAddress> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.len() > MAX_BASE_URL
        || raw.contains('@')
        || raw.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return None;
    }
    let (scheme, rest) = raw.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let host = valid_host_port(&authority)?;
    let loopback = host == "localhost"
        || host.ends_with(".localhost")
        || host.starts_with("127.")
        || host == "[::1]";
    Some(UpstreamAddress {
        url: format!("{scheme}://{authority}"),
        host: authority.clone(),
        loopback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projection_is_an_allowlist() {
        let item = json!({
            "id":"acct_test","name":"OpenAI 账号","provider":"openai",
            "email":"Someone@Example.com","authenticationKind":"oauth",
            "planType":"pro","planTypeDisplay":"Pro","label":null,"notes":"  主力\n账号 ",
            "accessToken":"TOKEN_SENTINEL","refreshToken":"REFRESH_SENTINEL","apiKey":"KEY_SENTINEL",
            "accessTokenExpiresAt":"2026-10-22T00:00:00Z","hasRefreshToken":true,
            "userId":"user-sentinel","accountId":"uuid-sentinel",
            "groups":[{"id":"grp_test","name":"DEFAULT","color":"#05FF0080","enabled":true}],
            "usage":{"lastUsedAt":"2026-10-22T11:35:17.311784+08:00","requestCount":3},
            "quota":{"windows":[{"key":"weekly"}]}
        });
        let identity = project(&item);
        let text = serde_json::to_string(&identity).unwrap();
        for secret in [
            "TOKEN_SENTINEL",
            "REFRESH_SENTINEL",
            "KEY_SENTINEL",
            "user-sentinel",
            "uuid-sentinel",
            "expires",
            "quota",
            "windows",
        ] {
            assert!(
                !text.to_lowercase().contains(&secret.to_lowercase()),
                "{secret} leaked: {text}"
            );
        }
        assert_eq!(identity.email.as_deref(), Some("Someone@Example.com"));
        assert_eq!(identity.authentication_kind.as_deref(), Some("oauth"));
        assert_eq!(identity.plan_type_display.as_deref(), Some("Pro"));
        assert_eq!(identity.notes.as_deref(), Some("主力 账号"));
        assert_eq!(identity.groups.len(), 1);
        assert_eq!(identity.groups[0].color.as_deref(), Some("#05FF0080"));
        assert_eq!(
            identity.last_used_at.as_deref(),
            Some("2026-10-22T11:35:17.311784+08:00")
        );
    }

    #[test]
    fn projection_rejects_malformed_values() {
        let long = "x".repeat(900);
        let groups: Vec<Value> = (0..40)
            .map(|i| json!({"id":format!("grp_{i}"),"name":format!("G{i}"),"color":"red;background:url(x)"}))
            .chain([json!({"id":"bad id","name":"Bad"}), json!({"id":"grp_noname"})])
            .collect();
        let identity = project(&json!({
            "email":"not an email","authenticationKind":"OAuth<script>","notes":long,
            "planType":"","groups":groups,"usage":{"lastUsedAt":"<b>yesterday</b>"}
        }));
        assert_eq!(identity.email, None);
        assert_eq!(identity.authentication_kind, None);
        assert_eq!(identity.plan_type, None);
        assert_eq!(
            identity.notes.as_ref().unwrap().chars().count(),
            MAX_NOTES + 1
        );
        assert_eq!(identity.groups.len(), MAX_GROUPS);
        assert!(
            identity
                .groups
                .iter()
                .all(|g| g.color.is_none() && g.id.starts_with("grp_"))
        );
        assert_eq!(identity.last_used_at, None);
        assert_eq!(project(&json!(null)), AccountIdentity::default());
    }

    #[test]
    fn display_label_matches_native_accounts_page() {
        let oauth = AccountIdentity {
            email: Some("a@example.com".into()),
            authentication_kind: Some("oauth".into()),
            ..Default::default()
        };
        assert_eq!(display_label("OpenAI 账号", &oauth), "a@example.com");
        let key = AccountIdentity {
            email: Some("ignored@example.com".into()),
            authentication_kind: Some("api_key".into()),
            ..Default::default()
        };
        assert_eq!(display_label("CPA-API", &key), "CPA-API");
        assert_eq!(
            display_label("Legacy", &AccountIdentity::default()),
            "Legacy"
        );
        let labelled = AccountIdentity {
            label: Some("Team".into()),
            ..Default::default()
        };
        assert_eq!(display_label("x", &labelled), "Team");
    }

    #[test]
    fn base_url_never_exposes_credentials_path_or_query() {
        let a = sanitize_base_url(
            "HTTPS://Relay.Example.test:8443/sk-path-secret/v1/?token=SECRET#frag",
        )
        .unwrap();
        assert_eq!(a.url, "https://relay.example.test:8443");
        assert_eq!(a.host, "relay.example.test:8443");
        assert!(!a.loopback);
        let b = sanitize_base_url("http://127.0.0.1:8317/v1").unwrap();
        assert_eq!(
            (b.url.as_str(), b.host.as_str(), b.loopback),
            ("http://127.0.0.1:8317", "127.0.0.1:8317", true)
        );
        let c = sanitize_base_url("https://[::1]:9000/v1").unwrap();
        assert!(c.loopback && c.url == "https://[::1]:9000");
        assert!(sanitize_base_url("http://LOCALHOST/").unwrap().loopback);
        assert_eq!(
            sanitize_base_url("https://relay.test?x=1").unwrap().url,
            "https://relay.test"
        );
        for bad in [
            "",
            "ftp://x.test",
            "javascript:alert(1)",
            "https://",
            "https://bad host/",
            "https://a.test:99999999",
            "https://a.test:/v1",
            "https://user:pass@relay.test/v1",
            "https://relay.test/p@th",
            "https://a.test/\nx",
            "https://[zz::1]/",
            "https://[::1",
            "https://[::1]x/",
            "https://a.test:80:80",
        ] {
            assert_eq!(sanitize_base_url(bad), None, "{bad}");
        }
    }
}
