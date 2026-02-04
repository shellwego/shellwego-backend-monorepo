//! Configuration management
//! 
//! Hierarchical: defaults < config file < env vars < CLI args

use serde::Deserialize;
use std::net::SocketAddr;

// TODO: Add clap for CLI arg parsing
// TODO: Add validation (e.g., database_url required in production)

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    
    pub database_url: String,
    
    #[serde(default)]
    pub nats_url: Option<String>,
    
    #[serde(default = "default_log_level")]
    pub log_level: String,
    
    // JWT signing key (HS256 for dev, RS256 for prod)
    pub jwt_secret: String,
    
    #[serde(default)]
    pub encryption_key_id: Option<String>,
    
    // Cloudflare or custom
    #[serde(default)]
    pub dns_provider: Option<DnsProviderConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsProviderConfig {
    pub provider: String, // "cloudflare", "route53", "custom"
    pub api_token: String,
    pub zone_id: Option<String>,
}

fn default_bind_addr() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        // TODO: Implement figment-based loading with env prefix SHELLWEGO_
        // TODO: Validate required fields
        
        // Placeholder for now - reads from env only
        let cfg = config::Config::builder()
            .add_source(config::Environment::with_prefix("SHELLWEGO"))
            .build()?;
            
        cfg.try_deserialize()
            .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))
    }
}