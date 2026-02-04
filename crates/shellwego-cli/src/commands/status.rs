//! CLI status command

use colored::Colorize;

use crate::{CliConfig, OutputFormat};

pub async fn handle(config: &CliConfig, _format: OutputFormat) -> anyhow::Result<()> {
    println!("{}", "ShellWeGo CLI Status".bold().blue());
    println!("Version: {}", env!("CARGO_PKG_VERSION"));
    println!("API URL: {}", config.api_url);
    
    match config.get_token() {
        Some(_) => println!("Auth: {}", "authenticated".green()),
        None => println!("Auth: {}", "not authenticated".red()),
    }
    
    if let Some(org) = config.default_org {
        println!("Default org: {}", org);
    }
    
    // TODO: Check API connectivity
    // TODO: Show rate limit status
    
    Ok(())
}