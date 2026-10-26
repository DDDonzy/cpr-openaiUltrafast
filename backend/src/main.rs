use cpr_ultrafast::{manifest, plugin};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--print-manifest"] {
        println!("{}", serde_json::to_string(&manifest()?)?);
        return Ok(());
    }
    if !args.is_empty() {
        return Err("unsupported command-line arguments".into());
    }
    let session = gateway_plugin_sdk::client::PluginSession::accept(
        tokio::io::stdin(),
        tokio::io::stdout(),
        gateway_plugin_sdk::client::SessionConfig::default(),
    )
    .await?;
    if session.handshake().plugin_id != cpr_ultrafast::PLUGIN_ID
        || session.handshake().contributes != manifest()?.contributes
    {
        return Err(gateway_plugin_sdk::client::SessionError::Handshake.into());
    }
    let configuration = session.handshake().configuration.clone();
    session.run(plugin(configuration)?).await?;
    Ok(())
}
