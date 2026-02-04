//! Log streaming command

use clap::Args;

use crate::{CliConfig, client::ApiClient};

#[derive(Args)]
pub struct LogArgs {
    app_id: uuid::Uuid,
    
    #[arg(short, long)]
    follow: bool,
    
    #[arg(short, long, default_value = "100")]
    tail: usize,
    
    #[arg(short, long)]
    since: Option<String>,
}

pub async fn handle(args: LogArgs, config: &CliConfig) -> anyhow::Result<()> {
    let client = crate::client(config)?;
    
    println!("Fetching logs for app {}...", args.app_id);
    
    let logs = client.get_logs(args.app_id, args.follow).await?;
    print!("{}", logs);
    
    if args.follow {
        println!("{}", "\n[Following logs... Ctrl+C to exit]".dimmed());
        // TODO: WebSocket streaming
    }
    
    Ok(())
}