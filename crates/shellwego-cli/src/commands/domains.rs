//! Domain management commands

use clap::{Args, Subcommand};

use crate::{CliConfig, OutputFormat, client::ApiClient};

#[derive(Args)]
pub struct DomainArgs {
    #[command(subcommand)]
    command: DomainCommands,
}

#[derive(Subcommand)]
enum DomainCommands {
    List,
    Add { hostname: String, app_id: uuid::Uuid },
    Remove { id: uuid::Uuid },
    Validate { id: uuid::Uuid },
}

pub async fn handle(args: DomainArgs, config: &CliConfig, _format: OutputFormat) -> anyhow::Result<()> {
    let _client = crate::client(config)?;
    
    match args.command {
        DomainCommands::List => println!("Listing domains..."),
        DomainCommands::Add { hostname, app_id } => {
            println!("Adding {} to app {}", hostname, app_id);
        }
        DomainCommands::Remove { id } => println!("Removing: {}", id),
        DomainCommands::Validate { id } => println!("Validating DNS for {}", id),
    }
    
    Ok(())
}