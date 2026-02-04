//! Remote execution command

use clap::Args;

use crate::CliConfig;

#[derive(Args)]
pub struct ExecArgs {
    app_id: uuid::Uuid,
    
    #[arg(default_value = "/bin/sh")]
    command: String,
    
    #[arg(short, long)]
    tty: bool,
}

pub async fn handle(args: ExecArgs, _config: &CliConfig) -> anyhow::Result<()> {
    println!("Connecting to {}...", args.app_id);
    println!("Executing: {}", args.command);
    
    if args.tty {
        println!("Interactive shell requested (TODO: WebSocket upgrade)");
    }
    
    // TODO: Implement exec via WebSocket
    println!("{}", "Exec not yet implemented".yellow());
    
    Ok(())
}