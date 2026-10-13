//! 使用宿主受管分派和私有状态，不读取原始凭据或访问宿主数据库

use crate::routing::{
    domain::{Account, Document, GroupState},
    state_codec,
};
use gateway_plugin_sdk::{
    ErrorCode, PluginFault,
    call::{
        host::{StateGetRequest, StateGetResult, StatePutRequest, StatePutResult},
        middleware::{MiddlewareHeader, http::Version},
    },
    client::{HostClient, HttpBody, HttpFrame, HttpRequest, SessionError},
};
use serde_json::{Value, json};

pub const NAMESPACE: &str = "routing-groups";
const KEY: &str = "document";
const MAX_RESPONSE: usize = 8 * 1024 * 1024;

fn fault(message: &'static str) -> PluginFault {
    PluginFault::new(ErrorCode::Fault, message)
}

pub async fn native(
    host: &HostClient,
    method: &str,
    uri: String,
    body: Option<Value>,
) -> Result<Value, PluginFault> {
    let bytes = body
        .map(|b| serde_json::to_vec(&b))
        .transpose()
        .map_err(|_| fault("管理请求编码失败"))?
        .unwrap_or_default();
    let headers = if bytes.is_empty() {
        vec![]
    } else {
        vec![MiddlewareHeader {
            name: "content-type".into(),
            value: b"application/json".to_vec(),
        }]
    };
    let mut response = host
        .dispatch_http(HttpRequest {
            method: method.into(),
            uri,
            version: Version::Http11,
            headers,
            settings: Value::Null,
            timeout_ms: Some(20_000),
            body: HttpBody::from_bytes(bytes),
        })
        .await?;
    let status = response.status;
    let mut data = Vec::new();
    while let Some(frame) = response.body.read().await? {
        if let HttpFrame::Data(chunk) = frame {
            if data.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                response.body.close().await?;
                return Err(fault("宿主管理响应超过读取上限"));
            }
            data.extend_from_slice(&chunk);
        }
    }
    let value: Value =
        serde_json::from_slice(&data).map_err(|_| fault("宿主管理响应不是有效 JSON"))?;
    if !(200..300).contains(&status) || value.get("code").and_then(Value::as_i64) != Some(200) {
        // 不透传原始错误正文，避免账号接口的其他字段或凭据进入插件页面
        return Err(fault("原生管理操作未完成，请检查账号状态并重新加载"));
    }
    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}
pub async fn accounts(host: &HostClient) -> Result<Vec<Account>, PluginFault> {
    let mut result = Vec::new();
    for page in 1..=20 {
        let data = native(
            host,
            "GET",
            format!("/api/admin/accounts?page={page}&pageSize=100"),
            None,
        )
        .await?;
        let items = data
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| fault("账号列表格式不兼容"))?;
        for item in items {
            let account: Account = serde_json::from_value(item.clone())
                .map_err(|_| fault("账号模型权限格式不兼容"))?;
            account
                .model_access
                .normalized()
                .map_err(|_| fault("账号模型权限无效"))?;
            result.push(account);
        }
        if items.len() < 100 {
            return Ok(result);
        }
    }
    Err(fault("账号数量超过本页支持的 2000 个上限"))
}
pub async fn get_state(host: &HostClient) -> Result<GroupState, PluginFault> {
    let reply = host
        .call(
            "host.state.get",
            serde_json::to_value(StateGetRequest {
                namespace: NAMESPACE.into(),
                key: KEY.into(),
            })
            .map_err(|_| fault("分组查询编码失败"))?,
            Vec::new(),
        )
        .await
        .map_err(SessionError::into_plugin_fault)?;
    let result: StateGetResult =
        serde_json::from_value(reply.result).map_err(|_| fault("分组状态格式无效"))?;
    match result.record {
        None => Ok(GroupState {
            version: None,
            value: Document::default(),
        }),
        Some(record) => {
            if record.schema_version != 1 {
                return Err(fault("分组状态版本不受支持"));
            }
            let value: Document =
                state_codec::decode(record.value).map_err(|_| fault("分组内容格式无效"))?;
            value.validate().map_err(|_| fault("分组内容未通过校验"))?;
            Ok(GroupState {
                version: Some(record.version),
                value,
            })
        }
    }
}
pub async fn put_state(
    host: &HostClient,
    value: Document,
    expected_version: Option<u64>,
) -> Result<GroupState, PluginFault> {
    value
        .validate()
        .map_err(|m| PluginFault::new(ErrorCode::InvalidInput, m))?;
    let request = StatePutRequest {
        namespace: NAMESPACE.into(),
        key: KEY.into(),
        expected_version,
        value: state_codec::encode(&value)
            .map_err(|m| PluginFault::new(ErrorCode::InvalidInput, m))?,
    };
    let reply = host
        .call(
            "host.state.put",
            serde_json::to_value(request).map_err(|_| fault("分组编码失败"))?,
            Vec::new(),
        )
        .await
        .map_err(SessionError::into_plugin_fault)?;
    let saved: StatePutResult =
        serde_json::from_value(reply.result).map_err(|_| fault("分组保存结果无效"))?;
    Ok(GroupState {
        version: Some(saved.version),
        value,
    })
}
pub async fn update_policy(
    host: &HostClient,
    account_id: &str,
    policy: &crate::routing::domain::ModelAccess,
) -> Result<(), PluginFault> {
    native(
        host,
        "POST",
        "/api/admin/accounts/batch-update".into(),
        Some(json!({"accountIds":[account_id],"modelAccess":policy})),
    )
    .await?;
    Ok(())
}
