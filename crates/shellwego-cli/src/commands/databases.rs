//! Database management commands

use clap::{Args, Subcommand};

use crate::{CliConfig, OutputFormat, client::ApiClient};

#[derive(Args)]
pub struct DbArgs {
    #[command(subcommand)]
    command: DbCommands,
}

#[derive(Subcommand)]
enum DbCommands {
    List,
    Create { name: String, engine: String },
    Get { id: uuid::Uuid },
    Delete { id: uuid::Uuid },
    Backup { id: uuid::Uuid },
    Restore { id: uuid::Uuid, backup_id: uuid::Uuid },
}

pub async fn handle(args: DbArgs, config: &CliConfig, _format: OutputFormat) -> anyhow::Result<()> {
    let _client = crate::client(config)?;
    
    match args.command {
        DbCommands::List => println!("Listing databases..."),
        DbCommands::Create { name, engine } => {
            println!("Creating {} database '{}'...", engine, name);
        }
        DbCommands::Get { id } => println!("Database: {}", id),
        DbCommands::Delete { id } => println!("Deleting: {}", id),
        DbCommands::Backup { id } => println!("Backing up: {}", id),
        DbCommands::Restore { id, backup_id } => {
            println!("Restoring {} to backup {}", id, backup_id);
        }
    }
    
    Ok(())
}