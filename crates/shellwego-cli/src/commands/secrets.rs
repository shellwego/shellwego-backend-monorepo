//! Secret management commands

use clap::{Args, Subcommand};

use crate::{CliConfig, OutputFormat, client::ApiClient};

#[derive(Args)]
pub struct SecretArgs {
    #[command(subcommand)]
    command: SecretCommands,
}

#[derive(Subcommand)]
enum SecretCommands {
    List,
    Set { name: String, value: Option<String> },
    Get { id: uuid::Uuid },
    Delete { id: uuid::Uuid },
    Rotate { id: uuid::Uuid },
}

pub async fn handle(args: SecretArgs, config: &CliConfig, _format: OutputFormat) -> anyhow::Result<()> {
    let _client = crate::client(config)?;
    
    match args.command {
        SecretCommands::List => println!("Listing secrets..."),
        SecretCommands::Set { name, value } => {
            let val = match value {
                Some(v) => v,
                None => {
                    println!("Enter value (will be hidden):");
                    // TODO: Read hidden input
                    "secret".to_string()
                }
            };
            println!("Setting secret '{}'...", name);
        }
        SecretCommands::Get { id } => println!("Secret: {} (value hidden)", id),
        SecretCommands::Delete { id } => println!("Deleting: {}", id),
        SecretCommands::Rotate { id } => println!("Rotating: {}", id),
    }
    
    Ok(())
}