//! Local tunnel to remote apps (like ngrok)

use clap::Args;

use crate::CliConfig;

#[derive(Args)]
pub struct TunnelArgs {
    /// App ID to tunnel to
    app_id: uuid::Uuid,
    
    /// Local port to forward
    #[arg(short, long)]
    local_port: u16,
    
    /// Remote port on app
    #[arg(short, long)]
    remote_port: u16,
    
    /// Bind address
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
}

pub async fn handle(args: TunnelArgs, config: &CliConfig) -> anyhow::Result<()> {
    let _client = crate::client(config)?;
    
    println!(
        "Creating tunnel {}:{} → {}:{}",
        args.bind, args.local_port, args.app_id, args.remote_port
    );
    
    // TODO: Authenticate with control plane
    // TODO: Request tunnel endpoint
    // TODO: Establish WebSocket or QUIC tunnel
    // TODO: Forward TCP connections
    // TODO: Handle reconnection
    
    println!("{}", "Tunnel not yet implemented".yellow());
    
    Ok(())
}