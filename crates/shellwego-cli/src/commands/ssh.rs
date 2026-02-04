//! SSH key management and direct SSH access

use clap::{Args, Subcommand};

use crate::CliConfig;

#[derive(Args)]
pub struct SshArgs {
    #[command(subcommand)]
    command: SshCommands,
}

#[derive(Subcommand)]
enum SshCommands {
    /// List SSH keys
    List,
    
    /// Add SSH key
    Add {
        /// Path to public key file
        key_file: std::path::PathBuf,
        
        /// Key name
        #[arg(short, long)]
        name: Option<String>,
    },
    
    /// Remove SSH key
    Remove { key_id: uuid::Uuid },
    
    /// SSH into app instance (debug)
    Debug {
        app_id: uuid::Uuid,
        
        /// Instance index (default 0)
        #[arg(short, long, default_value = "0")]
        instance: u32,
    },
}

pub async fn handle(args: SshArgs, config: &CliConfig) -> anyhow::Result<()> {
    let _client = crate::client(config)?;
    
    match args.command {
        SshCommands::List => {
            println!("Listing SSH keys...");
            // TODO: Fetch and display keys
        }
        SshCommands::Add { key_file, name } => {
            println!("Adding key from {:?}...", key_file);
            // TODO: Read public key
            // TODO: Validate format
            // TODO: Upload to API
        }
        SshCommands::Remove { key_id } => {
            println!("Removing key {}...", key_id);
        }
        SshCommands::Debug { app_id, instance } => {
            println!("Connecting to {} instance {}...", app_id, instance);
            // TODO: Fetch instance details
            // TODO: Establish SSH connection via jump host
            // TODO: Spawn interactive shell
            println!("Debug SSH not yet implemented");
        }
    }
    Ok(())
}