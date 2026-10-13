//! 从标准输入输出运行 SDK 会话，显式诊断参数只输出规范化清单

use cpr_config_plugin::{PLUGIN_ID, manifest, plugin};
use gateway_plugin_sdk::client::{PluginSession, SessionConfig, SessionError};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.as_slice() == ["--print-manifest"] {
        println!("{}", serde_json::to_string(&manifest()?)?);
        return Ok(());
    }
    if !args.is_empty() {
        return Err("unsupported command-line arguments".into());
    }
    let session = PluginSession::accept(
        tokio::io::stdin(),
        tokio::io::stdout(),
        SessionConfig::default(),
    )
    .await?;
    let manifest = manifest()?;
    if session.handshake().plugin_id != PLUGIN_ID
        || session.handshake().contributes != manifest.contributes
        || !session
            .handshake()
            .configuration
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    {
        return Err(SessionError::Handshake.into());
    }
    session.run(plugin()?).await?;
    Ok(())
}
