//! ShellWeGo CLI
//! 
//! The hacker's interface to the sovereign cloud.
//! Zero-bullshit deployment from your terminal.

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::process;

mod client;
mod commands;
mod config;

use client::ApiClient;
use config::CliConfig;

/// ShellWeGo - Deploy your own cloud
#[derive(Parser)]
#[command(name = "shellwego")]
#[command(about = "The sovereign cloud CLI", long_about = None)]
#[command(version)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, global = true)]
    config: Option<std::path::PathBuf>,
    
    /// API endpoint URL
    #[arg(short, long, global = true)]
    api_url: Option<String>,
    
    /// Output format
    #[arg(short, long, global = true, value_enum, default_value = "table")]
    output: OutputFormat,
    
    /// Quiet mode (no progress bars)
    #[arg(short, long, global = true)]
    quiet: bool,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum OutputFormat {
    Table,
    Json,
    Yaml,
    Plain,
}

#[derive(Subcommand)]
enum Commands {
    /// Authenticate with a ShellWeGo instance
    #[command(alias = "login")]
    Auth(commands::auth::AuthArgs),
    
    /// Manage applications
    #[command(alias = "app")]
    Apps(commands::apps::AppArgs),
    
    /// Manage worker nodes
    #[command(alias = "node")]
    Nodes(commands::nodes::NodeArgs),
    
    /// Manage persistent volumes
    #[command(alias = "vol")]
    Volumes(commands::volumes::VolumeArgs),
    
    /// Manage domains and TLS
    #[command(alias = "domain")]
    Domains(commands::domains::DomainArgs),
    
    /// Managed databases
    #[command(alias = "db")]
    Databases(commands::databases::DbArgs),
    
    /// Manage secrets
    Secrets(commands::secrets::SecretArgs),
    
    /// Stream logs
    Logs(commands::logs::LogArgs),
    
    /// Execute commands in running apps
    #[command(alias = "ssh")]
    Exec(commands::exec::ExecArgs),
    
    /// Show current status
    Status,

    /// Real-time resource monitoring dashboard
    #[command(alias = "top")]
    Top(commands::top::TopArgs),

    /// Update CLI to latest version
    Update,
}

#[tokio::main]
async fn main() {
    // Fancy panic handler
    std::panic::set_hook(Box::new(|info| {
        eprintln!("{}: {}", "FATAL".red().bold(), info);
        std::process::exit(1);
    }));
    
    let cli = Cli::parse();
    
    // Load or create config
    let mut config = match CliConfig::load(cli.config.as_ref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: Failed to load config: {}", "ERROR".red(), e);
            process::exit(1);
        }
    };
    
    // Override with CLI args
    if let Some(url) = cli.api_url {
        config.api_url = url;
    }
    
    // Execute command
    let result = match cli.command {
        Commands::Auth(args) => commands::auth::handle(args, &mut config).await,
        Commands::Apps(args) => commands::apps::handle(args, &config, cli.output).await,
        Commands::Nodes(args) => commands::nodes::handle(args, &config, cli.output).await,
        Commands::Volumes(args) => commands::volumes::handle(args, &config, cli.output).await,
        Commands::Domains(args) => commands::domains::handle(args, &config, cli.output).await,
        Commands::Databases(args) => commands::databases::handle(args, &config, cli.output).await,
        Commands::Secrets(args) => commands::secrets::handle(args, &config, cli.output).await,
        Commands::Logs(args) => commands::logs::handle(args, &config).await,
        Commands::Exec(args) => commands::exec::handle(args, &config).await,
        Commands::Status => commands::status::handle(&config, cli.output).await,
        Commands::Top(args) => commands::top::handle(args, &config).await,
        Commands::Update => commands::update::handle().await,
    };
    
    if let Err(e) = result {
        eprintln!("{}: {}", "ERROR".red().bold(), e);
        process::exit(1);
    }
}

/// Helper to create API client from config
fn client(config: &CliConfig) -> anyhow::Result<ApiClient> {
    let token = config.token.clone()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated. Run `shellwego auth login`"))?;
        
    ApiClient::new(&config.api_url, &token)
}