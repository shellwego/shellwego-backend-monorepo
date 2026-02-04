//! Authentication commands

use clap::{Args, Subcommand};
use colored::Colorize;
use dialoguer::{Input, Password};

use crate::{CliConfig, client::ApiClient};

#[derive(Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommands,
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Login to a ShellWeGo instance
    Login,
    
    /// Logout and clear credentials
    Logout,
    
    /// Show current authentication status
    Status,
    
    /// Switch active organization
    #[command(name = "switch-org")]
    SwitchOrg { org_id: uuid::Uuid },
}

pub async fn handle(args: AuthArgs, config: &mut CliConfig) -> anyhow::Result<()> {
    match args.command {
        AuthCommands::Login => login(config).await,
        AuthCommands::Logout => {
            config.clear_auth();
            config.save()?;
            println!("{}", "Logged out successfully".green());
            Ok(())
        }
        AuthCommands::Status => status(config).await,
        AuthCommands::SwitchOrg { org_id } => {
            config.default_org = Some(org_id);
            config.save()?;
            println!("Switched to organization {}", org_id);
            Ok(())
        }
    }
}

async fn login(config: &mut CliConfig) -> anyhow::Result<()> {
    println!("{}", "ShellWeGo Login".bold().blue());
    println!("API URL: {}", config.api_url);
    
    let email: String = Input::new()
        .with_prompt("Email")
        .interact_text()?;
        
    let password: String = Password::new()
        .with_prompt("Password")
        .interact()?;
        
    println!("{}", "Authenticating...".dimmed());
    
    let client = ApiClient::new(&config.api_url, "")?; // No token yet
    
    match client.login(&email, &password).await {
        Ok(token) => {
            config.set_token(token)?;
            config.save()?;
            
            // Fetch and display user info
            let authed_client = ApiClient::new(&config.api_url, config.get_token().unwrap_or_default())?;
            let user = authed_client.get_user().await?;
            
            println!("{}", "Login successful!".green().bold());
            if let Some(name) = user.get("name").and_then(|n| n.as_str()) {
                println!("Welcome, {}!", name);
            }
            
            Ok(())
        }
        Err(e) => {
            Err(anyhow::anyhow!("Login failed: {}", e))
        }
    }
}

async fn status(config: &CliConfig) -> anyhow::Result<()> {
    match config.get_token() {
        Some(token) => {
            let client = ApiClient::new(&config.api_url, &token)?;
            
            match client.get_user().await {
                Ok(user) => {
                    println!("{}", "Authenticated".green().bold());
                    if let Some(email) = user.get("email").and_then(|e| e.as_str()) {
                        println!("Email: {}", email);
                    }
                    if let Some(org) = config.default_org {
                        println!("Default org: {}", org);
                    }
                }
                Err(e) => {
                    println!("{}", "Token invalid or expired".red());
                    println!("Error: {}", e);
                }
            }
        }
        None => {
            println!("{}", "Not authenticated".yellow());
            println!("Run `shellwego auth login` to authenticate");
        }
    }
    
    Ok(())
}