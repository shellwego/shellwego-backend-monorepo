//! Node management commands

use clap::{Args, Subcommand};
use colored::Colorize;
use comfy_table::Table;

use crate::{CliConfig, OutputFormat, client::ApiClient, commands::format_output};

#[derive(Args)]
pub struct NodeArgs {
    #[command(subcommand)]
    command: NodeCommands,
}

#[derive(Subcommand)]
enum NodeCommands {
    /// List worker nodes
    List,
    
    /// Register new node (generates join script)
    Register {
        #[arg(short, long)]
        hostname: String,
        #[arg(short, long)]
        region: String,
    },
    
    /// Show node details
    Get { id: uuid::Uuid },
    
    /// Drain node (migrate apps away)
    Drain { id: uuid::Uuid },
    
    /// Delete node
    Delete { id: uuid::Uuid },
}

pub async fn handle(args: NodeArgs, config: &CliConfig, format: OutputFormat) -> anyhow::Result<()> {
    let client = crate::client(config)?;
    
    match args.command {
        NodeCommands::List => list(client, format).await,
        NodeCommands::Register { hostname, region } => register(client, hostname, region).await,
        NodeCommands::Get { id } => get(client, id, format).await,
        NodeCommands::Drain { id } => drain(client, id).await,
        NodeCommands::Delete { id } => delete(client, id).await,
    }
}

async fn list(client: ApiClient, format: OutputFormat) -> anyhow::Result<()> {
    let nodes = client.list_nodes().await?;
    
    match format {
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_header(vec!["ID", "Hostname", "Status", "Region", "Apps", "Capacity"]);
            
            for node in nodes {
                table.add_row(vec![
                    node.id.to_string().chars().take(8).collect(),
                    node.hostname,
                    format!("{:?}", node.status),
                    node.region,
                    node.running_apps.to_string(),
                    format!("{}GB/{:.1} CPU", node.capacity.memory_available_gb, node.capacity.cpu_available),
                ]);
            }
            
            println!("{}", table);
        }
        _ => println!("{}", format_output(&nodes, format)?),
    }
    
    Ok(())
}

async fn register(client: ApiClient, hostname: String, region: String) -> anyhow::Result<()> {
    // TODO: Call register API, get join token
    println!("Registering node '{}' in region '{}'...", hostname, region);
    println!("{}", "Run the following on the new node:".bold());
    println!("  curl -fsSL https://shellwego.com/install.sh | sudo bash -s -- --token=<token>");
    Ok(())
}

async fn get(client: ApiClient, id: uuid::Uuid, format: OutputFormat) -> anyhow::Result<()> {
    // TODO: Implement get node
    println!("Node details: {}", id);
    Ok(())
}

async fn drain(client: ApiClient, id: uuid::Uuid) -> anyhow::Result<()> {
    println!("Draining node {}...", id);
    println!("{}", "Apps will be migrated to other nodes.".yellow());
    Ok(())
}

async fn delete(client: ApiClient, id: uuid::Uuid) -> anyhow::Result<()> {
    println!("Deleting node {}...", id);
    println!("{}", "Ensure node is drained first!".red().bold());
    Ok(())
}