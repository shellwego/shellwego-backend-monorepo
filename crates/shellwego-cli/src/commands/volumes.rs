//! Volume management commands

use clap::{Args, Subcommand};

use crate::{CliConfig, OutputFormat, client::ApiClient};

#[derive(Args)]
pub struct VolumeArgs {
    #[command(subcommand)]
    command: VolumeCommands,
}

#[derive(Subcommand)]
enum VolumeCommands {
    List,
    Create { name: String, size_gb: u64 },
    Get { id: uuid::Uuid },
    Delete { id: uuid::Uuid },
    Attach { id: uuid::Uuid, app_id: uuid::Uuid },
    Detach { id: uuid::Uuid },
    Snapshot { id: uuid::Uuid, name: String },
}

pub async fn handle(args: VolumeArgs, config: &CliConfig, _format: OutputFormat) -> anyhow::Result<()> {
    let _client = crate::client(config)?;
    
    match args.command {
        VolumeCommands::List => println!("Listing volumes..."),
        VolumeCommands::Create { name, size_gb } => {
            println!("Creating volume '{}' ({}GB)...", name, size_gb);
        }
        VolumeCommands::Get { id } => println!("Volume: {}", id),
        VolumeCommands::Delete { id } => println!("Deleting: {}", id),
        VolumeCommands::Attach { id, app_id } => {
            println!("Attaching {} to app {}", id, app_id);
        }
        VolumeCommands::Detach { id } => println!("Detaching: {}", id),
        VolumeCommands::Snapshot { id, name } => {
            println!("Creating snapshot '{}' of volume {}", name, id);
        }
    }
    
    Ok(())
}