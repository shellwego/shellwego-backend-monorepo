//! App management commands

use clap::{Args, Subcommand};
use colored::Colorize;
use comfy_table::{Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL};
use dialoguer::{Input, Select, Confirm};
use shellwego_core::entities::app::{CreateAppRequest, ResourceRequest, ResourceSpec, UpdateAppRequest};

use crate::{CliConfig, OutputFormat, client::ApiClient, commands::format_output};

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[derive(Args)]
pub struct AppArgs {
    #[command(subcommand)]
    command: AppCommands,
}

#[derive(Subcommand)]
enum AppCommands {
    /// List all apps
    List {
        #[arg(short, long)]
        org: Option<uuid::Uuid>,
    },
    
    /// Create new app
    Create {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        image: Option<String>,
    },
    
    /// Show app details
    Get { id: uuid::Uuid },
    
    /// Update app configuration
    Update { id: uuid::Uuid },
    
    /// Delete app
    Delete {
        id: uuid::Uuid,
        #[arg(short, long)]
        force: bool,
    },
    
    /// Deploy new version
    Deploy {
        id: uuid::Uuid,
        image: String,
    },
    
    /// Scale replicas
    Scale {
        id: uuid::Uuid,
        replicas: u32,
    },
    
    /// Start stopped app
    Start { id: uuid::Uuid },
    
    /// Stop running app
    Stop { id: uuid::Uuid },
    
    /// Restart app
    Restart { id: uuid::Uuid },
}

pub async fn handle(args: AppArgs, config: &CliConfig, format: OutputFormat) -> anyhow::Result<()> {
    let client = crate::client(config)?;
    
    match args.command {
        AppCommands::List { org } => list(client, org, format).await,
        AppCommands::Create { name, image } => create(client, name, image).await,
        AppCommands::Get { id } => get(client, id, format).await,
        AppCommands::Update { id } => update(client, id).await,
        AppCommands::Delete { id, force } => delete(client, id, force).await,
        AppCommands::Deploy { id, image } => deploy(client, id, image).await,
        AppCommands::Scale { id, replicas } => scale(client, id, replicas).await,
        AppCommands::Start { id } => start(client, id).await,
        AppCommands::Stop { id } => stop(client, id).await,
        AppCommands::Restart { id } => restart(client, id).await,
    }
}

async fn list(client: ApiClient, _org: Option<uuid::Uuid>, format: OutputFormat) -> anyhow::Result<()> {
    let apps = client.list_apps().await?;
    
    match format {
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_header(vec!["ID", "Name", "Status", "Image", "Replicas"]);
            
            for app in apps {
                table.add_row(vec![
                    app.id.to_string().chars().take(8).collect::<String>(),
                    app.name,
                    format!("{:?}", app.status),
                    app.image.chars().take(30).collect::<String>(),
                    format!("{}/{}", app.replicas.current, app.replicas.desired),
                ]);
            }
            
            println!("{}", table);
        }
        _ => println!("{}", format_output(&apps, format)?),
    }
    
    Ok(())
}

async fn create(client: ApiClient, name: Option<String>, image: Option<String>) -> anyhow::Result<()> {
    // Interactive mode if args not provided
    let name = match name {
        Some(n) => n,
        None => Input::new()
            .with_prompt("App name")
            .interact_text()?,
    };
    
    let image = match image {
        Some(i) => i,
        None => Input::new()
            .with_prompt("Container image")
            .default("nginx:latest".to_string())
            .interact_text()?,
    };
    
    let req = CreateAppRequest {
        name: name.clone(),
        image,
        command: None,
        resources: ResourceSpec::from(ResourceRequest::default()),
        env: vec![],
        domains: vec![],
        volumes: vec![],
        health_check: None,
        replicas: 1,
    };
    
    let app = client.create_app(&req).await?;
    println!("{} Created app '{}' with ID {}", 
        "✓".green().bold(), 
        name, 
        app.id
    );
    
    Ok(())
}

async fn get(client: ApiClient, id: uuid::Uuid, format: OutputFormat) -> anyhow::Result<()> {
    let app = client.get_app(id).await?;
    
    match format {
        OutputFormat::Table => {
            println!("{} {}", "App:".bold(), app.name);
            println!("{} {}", "ID:".bold(), app.id);
            println!("{} {:?}", "Status:".bold(), app.status);
            println!("{} {}", "Image:".bold(), app.image);
            println!("{} {}/{}", "Replicas:".bold(), app.replicas.current, app.replicas.desired);
            println!("{} {} ({} milli-CPU)", "Resources:".bold(), 
                format_bytes(app.resources.memory_bytes), app.resources.cpu_milli);
            
            if !app.domains.is_empty() {
                println!("\n{}", "Domains:".bold());
                for d in &app.domains {
                    println!("  - {} (TLS: {})", d.hostname, d.tls_status);
                }
            }
        }
        _ => println!("{}", format_output(&app, format)?),
    }
    
    Ok(())
}

async fn update(client: ApiClient, id: uuid::Uuid) -> anyhow::Result<()> {
    // Interactive editor for app config
    let app = client.get_app(id).await?;
    
    println!("Updating app: {}", app.name);
    
    // TODO: Open in $EDITOR with current config as JSON
    // For now, just placeholder
    
    let req = UpdateAppRequest {
        name: None,
        resources: None,
        replicas: Some(2),
        env: None,
    };
    
    let updated = client.update_app(id, &req).await?;
    println!("{} Updated app", "✓".green());
    
    Ok(())
}

async fn delete(client: ApiClient, id: uuid::Uuid, force: bool) -> anyhow::Result<()> {
    if !force {
        let confirm = Confirm::new()
            .with_prompt(format!("Delete app {}?", id))
            .default(false)
            .interact()?;
            
        if !confirm {
            println!("Cancelled");
            return Ok(());
        }
    }
    
    client.delete_app(id).await?;
    println!("{} Deleted app {}", "✓".green().bold(), id);
    
    Ok(())
}

async fn deploy(client: ApiClient, id: uuid::Uuid, image: String) -> anyhow::Result<()> {
    println!("Deploying {} to app {}...", image.dimmed(), id);
    client.deploy_app(id, &image).await?;
    println!("{} Deployment queued", "✓".green());
    Ok(())
}

async fn scale(client: ApiClient, id: uuid::Uuid, replicas: u32) -> anyhow::Result<()> {
    client.scale_app(id, replicas).await?;
    println!("{} Scaled to {} replicas", "✓".green(), replicas);
    Ok(())
}

async fn start(client: ApiClient, id: uuid::Uuid) -> anyhow::Result<()> {
    println!("Starting app {}...", id);
    // TODO: Implement in client
    Ok(())
}

async fn stop(client: ApiClient, id: uuid::Uuid) -> anyhow::Result<()> {
    println!("Stopping app {}...", id);
    // TODO: Implement in client
    Ok(())
}

async fn restart(client: ApiClient, id: uuid::Uuid) -> anyhow::Result<()> {
    println!("Restarting app {}...", id);
    // TODO: Implement in client
    Ok(())
}