//! One management identity with isolated routing and capability modules.
pub mod capabilities;
pub mod routing;
use gateway_plugin_sdk::{
    ErrorCode, Manifest, PluginFault,
    call::management::{
        ManagementRegistration, ManagementRequest, ManagementResource, ManagementResponse,
        ManagementRoute,
    },
    client::{AuthorError, ComposedPlugin, PluginBuilder, TypedCall, TypedReply},
};
use std::sync::Arc;
pub const PLUGIN_ID: &str = "donzy.model-routing-groups";
pub const BAT: &[u8] = include_bytes!("../../dist/codex-setup.bat");
pub fn manifest() -> Result<Manifest, gateway_plugin_sdk::ManifestError> {
    Manifest::from_author_slice(include_bytes!("../../plugin.json"))
}
pub fn registration() -> ManagementRegistration {
    let mut r = routing::management::registration();
    for resource in &mut r.resources {
        resource.path = resource.path.replacen("web/", "web/routing/", 1);
    }
    r.pages[0].entry = "web/routing/index.html".into();
    let mut c = capabilities::management::registration();
    for route in &mut c.routes {
        route.path = format!("models/{}", route.path);
    }
    for resource in &mut c.resources {
        resource.path = resource.path.replacen("web/", "web/models/", 1);
    }
    c.pages[0].entry = "web/models/index.html".into();
    r.routes.extend(c.routes);
    r.resources.extend(c.resources);
    r.pages.extend(c.pages);
    r.resources.push(ManagementResource {
        path: "downloads/codex-setup.bat".into(),
        public: false,
    });
    r.routes.push(ManagementRoute {
        method: "GET".into(),
        path: "api/setup-download".into(),
        request_content_types: vec![],
        response_content_types: vec!["application/octet-stream".into()],
    });
    r
}
fn download(
    call: TypedCall<ManagementRequest>,
) -> Result<TypedReply<ManagementResponse>, PluginFault> {
    if !call.request.query.is_empty() || !call.payload.is_empty() {
        return Err(PluginFault::new(
            ErrorCode::InvalidInput,
            "下载不接受查询参数或正文",
        ));
    }
    Ok(TypedReply::new(ManagementResponse {
        status: 200,
        content_type: "application/octet-stream".into(),
        headers: vec![
            gateway_plugin_sdk::call::middleware::MiddlewareHeader {
                name: "content-disposition".into(),
                value: "attachment; filename=\"codex-setup.bat\"".into(),
            },
            gateway_plugin_sdk::call::middleware::MiddlewareHeader {
                name: "cache-control".into(),
                value: "no-store".into(),
            },
        ],
    })
    .with_payload(BAT.to_vec()))
}
pub fn plugin() -> Result<ComposedPlugin, AuthorError> {
    let routes = Arc::new(routing::management::App::default());
    let models = Arc::new(capabilities::management::App::default());
    PluginBuilder::from_json(include_bytes!("../../plugin.json"))?
        .management(registration(), move |mut call| {
            let routes = Arc::clone(&routes);
            let models = Arc::clone(&models);
            async move {
                if call.request.method == "GET" && call.request.path == "api/setup-download" {
                    return download(call);
                }
                if let Some(path) = call.request.path.strip_prefix("models/") {
                    call.request.path = path.to_owned();
                    models.handle(call).await
                } else {
                    routes.handle(call).await
                }
            }
        })?
        .build()
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    #[test]
    fn management_identity_and_state_preserved() {
        let m = manifest().unwrap();
        assert_eq!(format!("{}.{}", m.publisher, m.name), PLUGIN_ID);
        assert_eq!(m.state.len(), 1);
        assert_eq!(m.state[0].namespace, "routing-groups");
        assert_eq!(m.state[0].schema_version, 1);
        assert_eq!(m.contributes.len(), 1);
        plugin().unwrap();
    }
    #[test]
    fn registration_has_distinct_routes_and_pages() {
        let r = registration();
        assert_eq!(r.pages.len(), 2);
        let mut seen = HashSet::new();
        for x in &r.routes {
            assert!(seen.insert((&x.method, &x.path)));
        }
        assert!(r.routes.iter().any(|r| r.path == "api/snapshot"));
        assert!(r.routes.iter().any(|r| r.path == "models/api/snapshot"));
        assert!(r.routes.iter().any(|r| r.path == "api/setup-download"));
        assert!(
            r.resources
                .iter()
                .any(|r| r.path == "downloads/codex-setup.bat" && !r.public)
        );
    }
    #[test]
    fn packaged_bat_is_generic_and_not_a_user_config() {
        assert!(BAT.starts_with(b"@echo off\r\n"));
        let s = std::str::from_utf8(BAT).unwrap();
        assert!(s.contains("API Key:"));
        assert!(s.contains("#--CPR-POWERSHELL--"));
        assert!(!s.contains("us.cpr.dddonzy.xyz"));
    }
}
