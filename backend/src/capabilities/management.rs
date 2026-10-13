use crate::capabilities::{
    domain::{Catalog, MAX_BYTES, diff},
    store::{Store, StoreError},
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
use serde_json::json;
use tokio::sync::Mutex;
#[derive(Default)]
pub struct App {
    store: Store,
    writes: Mutex<()>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Update {
    expected_version: String,
    models: Catalog,
    model_order: Vec<String>,
}
fn encode(
    status: u16,
    value: &impl Serialize,
) -> Result<TypedReply<ManagementResponse>, PluginFault> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| PluginFault::new(ErrorCode::Fault, "响应编码失败"))?;
    Ok(TypedReply::new(ManagementResponse {
        status,
        content_type: "application/json".into(),
        headers: vec![],
    })
    .with_payload(bytes))
}
pub fn registration() -> ManagementRegistration {
    ManagementRegistration {
        routes: [
            ("GET", "api/snapshot"),
            ("POST", "api/preview"),
            ("POST", "api/save"),
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
        .collect(),
        resources: ["web/index.html", "web/app.js", "web/app.css"]
            .into_iter()
            .map(|path| ManagementResource {
                path: path.into(),
                public: path == "web/app.css",
            })
            .collect(),
        pages: vec![ManagementPage {
            id: "model-capabilities".into(),
            title: "模型基础配置".into(),
            description: Some("维护独立 JSON 文件，不参与模型调度".into()),
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
        match self.route(call).await {
            Ok(value) => encode(200, &value),
            Err(e) => encode(
                e.status,
                &json!({"error":{"code":if e.status==409{"conflict"}else{"configuration_error"},"message":e.message}}),
            ),
        }
    }
    async fn route(
        &self,
        call: TypedCall<ManagementRequest>,
    ) -> Result<serde_json::Value, StoreError> {
        if !call.request.query.is_empty() {
            return Err(StoreError::invalid("不支持查询参数"));
        }
        if call.payload.len() > MAX_BYTES * 2 + 4096 {
            return Err(StoreError::invalid("请求过大"));
        }
        match (call.request.method.as_str(), call.request.path.as_str()) {
            ("GET", "api/snapshot") => {
                if !call.payload.is_empty() {
                    return Err(StoreError::invalid("GET 请求不能携带正文"));
                }
                let snapshot = self.store.snapshot()?;
                Ok(json!(snapshot))
            }
            ("POST", path @ ("api/preview" | "api/save")) => {
                let update: Update = serde_json::from_slice(&call.payload)
                    .map_err(|e| StoreError::invalid(format!("请求 JSON 或字段无效：{e}")))?;
                let models = update
                    .models
                    .ordered(&update.model_order)
                    .map_err(StoreError::invalid)?;
                if update.expected_version.len() != 64
                    || !update
                        .expected_version
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit())
                {
                    return Err(StoreError::invalid("文件版本无效，请重新加载"));
                }
                if path == "api/preview" {
                    let (snapshot, change) =
                        self.store.preview(&update.expected_version, &models)?;
                    Ok(json!({"diff":change,"version":snapshot.version}))
                } else {
                    let _guard = self.writes.lock().await;
                    let before = self.store.snapshot()?;
                    let snapshot = self.store.save(&update.expected_version, &models)?;
                    Ok(json!({"snapshot":snapshot,"diff":diff(&before.models,&models)}))
                }
            }
            _ => Err(StoreError {
                status: 404,
                message: "未知管理接口".into(),
            }),
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn registration_is_management_only() {
        let m = super::registration();
        assert_eq!(m.routes.len(), 3);
        assert!(m.routes.iter().all(|r| r.path.starts_with("api/")));
        let manifest = crate::manifest().unwrap();
        assert_eq!(manifest.contributes.len(), 1);
        assert_eq!(manifest.state[0].namespace, "routing-groups");
        crate::plugin().unwrap();
    }
}
