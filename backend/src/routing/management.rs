//! 提供账号名单编辑、分组保存和应用前检查，只有显式应用才写原生权限

use crate::routing::{
    domain::{self, Account, Binding, GroupState, ModelGroup, PolicyRequest},
    host, identity,
};
use gateway_plugin_sdk::{
    ErrorCode, PluginFault,
    call::management::{
        ManagementPage, ManagementRegistration, ManagementRequest, ManagementResource,
        ManagementResponse, ManagementRoute,
    },
    client::{TypedCall, TypedReply},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct App {
    writes: Mutex<()>,
}
#[derive(Debug)]
struct ApiError {
    status: u16,
    code: &'static str,
    message: String,
}
type Reply = Result<TypedReply<ManagementResponse>, ApiError>;
impl ApiError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            code: "invalid_request",
            message: message.into(),
        }
    }
    fn conflict(message: &str) -> Self {
        Self {
            status: 409,
            code: "conflict",
            message: message.into(),
        }
    }
    fn into_reply(self) -> Result<TypedReply<ManagementResponse>, PluginFault> {
        encode(
            self.status,
            &json!({"error":{"code":self.code,"message":self.message}}),
        )
    }
}
impl From<PluginFault> for ApiError {
    fn from(e: PluginFault) -> Self {
        if e.code == ErrorCode::Conflict {
            Self::conflict("分组已在另一页面更新，请重新加载")
        } else {
            Self {
                status: 502,
                code: "host_operation_failed",
                message: "宿主操作未完成，请重新加载确认原生状态".into(),
            }
        }
    }
}
fn encode(
    status: u16,
    value: &impl Serialize,
) -> Result<TypedReply<ManagementResponse>, PluginFault> {
    let payload = serde_json::to_vec(value)
        .map_err(|_| PluginFault::new(ErrorCode::Fault, "响应编码失败"))?;
    Ok(TypedReply::new(ManagementResponse {
        status,
        content_type: "application/json".into(),
        headers: vec![],
    })
    .with_payload(payload))
}
fn reply(value: &impl Serialize) -> Reply {
    encode(200, value).map_err(|_| ApiError::invalid("响应编码失败"))
}
fn decode<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(data).map_err(|_| ApiError::invalid("请求 JSON 格式或字段无效"))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveGroups {
    expected_version: Option<u64>,
    groups: Vec<ModelGroup>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Catalog {
    account_id: String,
    #[serde(default)]
    refresh: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogAll {
    #[serde(default)]
    refresh: bool,
}

pub fn registration() -> ManagementRegistration {
    let routes = [
        ("GET", "api/snapshot"),
        ("GET", "api/upstreams"),
        ("GET", "api/catalog-all"),
        ("POST", "api/catalog-all"),
        ("POST", "api/catalog"),
        ("POST", "api/groups"),
        ("POST", "api/preview"),
        ("POST", "api/apply"),
    ]
    .into_iter()
    .map(|(method, path)| ManagementRoute {
        method: method.into(),
        path: path.into(),
        request_content_types: if method == "GET" {
            vec![]
        } else {
            vec!["application/json".into()]
        },
        response_content_types: vec!["application/json".into()],
    })
    .collect();
    ManagementRegistration {
        routes,
        resources: ["web/index.html", "web/app.js", "web/app.css"]
            .into_iter()
            .map(|path| ManagementResource {
                path: path.into(),
                public: path == "web/app.css",
            })
            .collect(),
        pages: vec![ManagementPage {
            id: "model-routing-groups".into(),
            title: "模型路由分组".into(),
            description: None,
            entry: "web/index.html".into(),
            icon: None,
        }],
        callbacks: vec![],
    }
}
impl App {
    pub async fn handle(
        &self,
        call: TypedCall<ManagementRequest>,
    ) -> Result<TypedReply<ManagementResponse>, PluginFault> {
        self.route(call).await.or_else(ApiError::into_reply)
    }
    async fn route(&self, call: TypedCall<ManagementRequest>) -> Reply {
        if !call.request.query.is_empty() {
            return Err(ApiError::invalid("插件接口不支持查询参数"));
        }
        if call.payload.len() > domain::MAX_DOCUMENT_BYTES {
            return Err(ApiError::invalid("请求内容过大"));
        }
        match (call.request.method.as_str(), call.request.path.as_str()) {
            ("GET", "api/snapshot") => {
                if !call.payload.is_empty() {
                    return Err(ApiError::invalid("GET 请求不能携带正文"));
                }
                let accounts = host::accounts(&call.host).await?;
                let state = host::get_state(&call.host).await?;
                reply(
                    &json!({"accounts":accounts,"state":state,"limits":{"modelsPerAccount":256,"groups":32},"pluginVersion":env!("CARGO_PKG_VERSION")}),
                )
            }
            ("GET", "api/upstreams") => {
                if !call.payload.is_empty() {
                    return Err(ApiError::invalid("GET 请求不能携带正文"));
                }
                let accounts = host::accounts(&call.host).await?;
                reply(&upstreams(&call, &accounts).await)
            }
            (method @ ("GET" | "POST"), "api/catalog-all") => {
                let refresh = if method == "POST" {
                    decode::<CatalogAll>(&call.payload)?.refresh
                } else {
                    if !call.payload.is_empty() {
                        return Err(ApiError::invalid("GET 请求不能携带正文"));
                    }
                    false
                };
                let accounts = host::accounts(&call.host).await?;
                let mut ids = std::collections::BTreeSet::new();
                let mut warnings = Vec::new();
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(22);
                let mut read_count = 0;
                for account in &accounts {
                    if tokio::time::Instant::now() >= deadline {
                        break;
                    }
                    let result = tokio::time::timeout_at(
                        deadline,
                        host::native(
                            &call.host,
                            if refresh { "POST" } else { "GET" },
                            if refresh {
                                "/api/admin/accounts/models/refresh".into()
                            } else {
                                format!("/api/admin/accounts/models?accountId={}", account.id)
                            },
                            if refresh {
                                Some(json!({"accountId":account.id}))
                            } else {
                                None
                            },
                        ),
                    )
                    .await;
                    read_count += 1;
                    match result {
                        Ok(Ok(data)) => {
                            if let Some(models) = data.get("models").and_then(Value::as_array) {
                                for item in models {
                                    if let Some(id) = item
                                        .get("id")
                                        .and_then(Value::as_str)
                                        .filter(|id| domain::valid_model_id(id))
                                    {
                                        ids.insert(id.to_owned());
                                    }
                                }
                            } else {
                                warnings.push(format!(
                                    "账号 {} 的目录格式不兼容，当前结果不完整",
                                    account.display_label()
                                ));
                            }
                        }
                        _ => warnings.push(format!(
                            "账号 {} 的目录暂不可读，当前结果不完整",
                            account.display_label()
                        )),
                    }
                    if ids.len() > 10000 {
                        return Err(ApiError::invalid(
                            "聚合目录超过 10000 个模型，请使用当前账号目录",
                        ));
                    }
                }
                if read_count < accounts.len() {
                    warnings.push(format!(
                        "读取目录超时，尚有 {} 个账号未读取，请重试",
                        accounts.len() - read_count
                    ));
                }
                reply(
                    &json!({"models":ids,"warnings":warnings,"accountCount":accounts.len(),"readAccountCount":read_count}),
                )
            }
            ("POST", "api/catalog") => {
                let req: Catalog = decode(&call.payload)?;
                if !domain::valid_account_id(&req.account_id) {
                    return Err(ApiError::invalid("账号 ID 无效"));
                }
                let accounts = host::accounts(&call.host).await?;
                if !accounts.iter().any(|a| a.id == req.account_id) {
                    return Err(ApiError::invalid("账号不存在"));
                }
                let data = if req.refresh {
                    host::native(
                        &call.host,
                        "POST",
                        "/api/admin/accounts/models/refresh".into(),
                        Some(json!({"accountId":req.account_id})),
                    )
                    .await?
                } else {
                    host::native(
                        &call.host,
                        "GET",
                        format!("/api/admin/accounts/models?accountId={}", req.account_id),
                        None,
                    )
                    .await?
                };
                let models = data
                    .get("models")
                    .and_then(Value::as_array)
                    .ok_or_else(|| ApiError::invalid("上游模型目录格式不兼容"))?;
                let mut ids = std::collections::BTreeSet::new();
                for item in models {
                    if let Some(id) = item
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|id| domain::valid_model_id(id))
                    {
                        ids.insert(id.to_owned());
                    }
                }
                reply(&json!({"models":ids,"accountId":req.account_id}))
            }
            ("POST", "api/groups") => {
                let _guard = self.writes.lock().await;
                let req: SaveGroups = decode(&call.payload)?;
                domain::validate_groups(&req.groups).map_err(ApiError::invalid)?;
                let mut state = host::get_state(&call.host).await?;
                if state.version != req.expected_version {
                    return Err(ApiError::conflict("分组版本已变化，请重新加载"));
                }
                state.value.groups = req
                    .groups
                    .into_iter()
                    .map(|mut g| {
                        g.models =
                            domain::normalize_models(&g.models, true).map_err(ApiError::invalid)?;
                        Ok(g)
                    })
                    .collect::<Result<Vec<_>, ApiError>>()?;
                let saved = host::put_state(&call.host, state.value, state.version).await?;
                reply(&saved)
            }
            ("POST", "api/preview") => {
                let req: PolicyRequest = decode(&call.payload)?;
                let (account, state, after) = prepare(&call, &req, false).await?;
                let diff = domain::diff(&account, after).map_err(ApiError::invalid)?;
                reply(&json!({"diff":diff,"groupVersion":state.version}))
            }
            ("POST", "api/apply") => {
                let _guard = self.writes.lock().await;
                let req: PolicyRequest = decode(&call.payload)?;
                let (account, mut state, after) = prepare(&call, &req, true).await?;
                let diff = domain::diff(&account, after.clone()).map_err(ApiError::invalid)?;
                if !account.model_access.equivalent(&after) {
                    host::update_policy(&call.host, &account.id, &after).await?;
                }
                // 原生修改与插件状态不是同一事务，先确认真正生效的原生值，再记录 UI 关联
                let updated = host::accounts(&call.host)
                    .await?
                    .into_iter()
                    .find(|a| a.id == account.id)
                    .ok_or_else(|| ApiError::invalid("账号已删除，请在原生账号页核对"))?;
                if !updated.model_access.equivalent(&after) {
                    return Err(ApiError::conflict(
                        "写入后原生规则与目标不一致，请重新加载，勿自动重试",
                    ));
                }
                state.value.bindings.insert(
                    account.id.clone(),
                    Binding {
                        group_ids: req.group_ids,
                        manual_models: domain::normalize_models(&req.manual_models, true)
                            .map_err(ApiError::invalid)?,
                        applied_policy: after,
                    },
                );
                let mut warnings = Vec::new();
                let state =
                    match host::put_state(&call.host, state.value.clone(), state.version).await {
                        Ok(saved) => saved,
                        Err(_) => {
                            warnings.push(
                                "原生模型权限已生效，但分组关联保存失败；重新加载后以原生规则为准"
                                    .to_owned(),
                            );
                            host::get_state(&call.host).await.unwrap_or(state)
                        }
                    };
                reply(
                    &json!({"account":updated,"state":state,"diff":diff,"warnings":warnings,"nativeApplied":true}),
                )
            }
            _ => Err(ApiError {
                status: 404,
                code: "not_found",
                message: "未找到插件管理接口".into(),
            }),
        }
    }
}
/// 只为 API Key 账号读取原生详情中的非敏感连接配置；单个账号失败不影响其余结果，也不写任何状态
async fn upstreams(call: &TypedCall<ManagementRequest>, accounts: &[Account]) -> Value {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(12);
    let mut result = serde_json::Map::new();
    let (mut failed, mut pending) = (0usize, 0usize);
    for account in accounts {
        let value = match account.identity.authentication_kind.as_deref() {
            Some("api_key") if domain::valid_account_id(&account.id) => {
                if tokio::time::Instant::now() >= deadline {
                    pending += 1;
                    json!({"kind":"api_key","status":"timeout"})
                } else {
                    match tokio::time::timeout_at(
                        deadline,
                        host::native(
                            &call.host,
                            "GET",
                            format!("/api/admin/accounts/detail?accountId={}", account.id),
                            None,
                        ),
                    )
                    .await
                    {
                        Ok(Ok(data)) => {
                            let config = data.get("credentialConfiguration");
                            let address = config
                                .and_then(|c| c.get("base_url"))
                                .and_then(Value::as_str)
                                .and_then(identity::sanitize_base_url);
                            let transport = config
                                .and_then(|c| c.get("transport"))
                                .and_then(Value::as_str)
                                .filter(|t| matches!(*t, "http" | "prefer_websocket"));
                            match address {
                                Some(address) => {
                                    json!({"kind":"api_key","status":"ok","address":address,"transport":transport})
                                }
                                None => {
                                    failed += 1;
                                    json!({"kind":"api_key","status":"unavailable"})
                                }
                            }
                        }
                        Ok(Err(_)) => {
                            failed += 1;
                            json!({"kind":"api_key","status":"unavailable"})
                        }
                        Err(_) => {
                            pending += 1;
                            json!({"kind":"api_key","status":"timeout"})
                        }
                    }
                }
            }
            Some("oauth") => json!({"kind":"oauth","status":"ok"}),
            _ => json!({"kind":"unknown","status":"ok"}),
        };
        result.insert(account.id.clone(), value);
    }
    let mut warnings = Vec::new();
    if failed > 0 {
        warnings.push(format!("{failed} 个 API Key 账号的上游地址暂不可读"));
    }
    if pending > 0 {
        warnings.push(format!(
            "读取超时，尚有 {pending} 个 API Key 账号未显示上游地址，请重新加载"
        ));
    }
    json!({"upstreams":result,"warnings":warnings})
}
async fn prepare(
    call: &TypedCall<ManagementRequest>,
    req: &PolicyRequest,
    saving: bool,
) -> Result<(Account, GroupState, domain::ModelAccess), ApiError> {
    let state = host::get_state(&call.host).await?;
    if state.version != req.expected_group_version {
        return Err(ApiError::conflict("分组版本已变化，请重新加载后预览"));
    }
    let after = if saving {
        domain::resolve_policy(req, &state.value)
    } else {
        domain::resolve_preview_policy(req, &state.value)
    }
    .map_err(ApiError::invalid)?;
    let account = host::accounts(&call.host)
        .await?
        .into_iter()
        .find(|a| a.id == req.account_id)
        .ok_or_else(|| ApiError::invalid("账号不存在"))?;
    if !account.model_access.equivalent(&req.before) {
        return Err(ApiError::conflict(
            "原生模型权限已被其他页面修改，请重新加载，未覆盖旧规则",
        ));
    }
    Ok((account, state, after))
}
