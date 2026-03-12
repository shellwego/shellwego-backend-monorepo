//! Control plane configuration
//!
//! Loads configuration from environment variables and config files.

use std::net::SocketAddr;
use std::path::Path;

use serde::Deserialize;
use tracing::info;

/// Main configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// HTTP bind address
    pub bind_addr: String,
    
    /// Database configuration
    pub database: DatabaseConfig,
    
    /// JWT configuration
    pub jwt: JwtConfig,
    
    /// Default region for deployments
    pub default_region: String,
    
    /// Log level
    pub log_level: String,
    
    /// Federation configuration
    pub federation: FederationConfig,
    
    /// Build queue configuration
    pub build: BuildConfig,
    
    /// KMS configuration
    pub kms: KmsConfigEntry,
}

/// Database configuration
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// Database connection URL
    pub url: String,
    
    /// Maximum connections in pool
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    
    /// Minimum connections in pool
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    
    /// Connection timeout in seconds
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    
    /// Run migrations on startup
    #[serde(default = "default_true")]
    pub auto_migrate: bool,
    
    /// Enable query logging
    #[serde(default)]
    pub logging: bool,
}

fn default_max_connections() -> u32 { 10 }
fn default_min_connections() -> u32 { 1 }
fn default_connect_timeout() -> u64 { 30 }
fn default_true() -> bool { true }

/// JWT configuration
#[derive(Debug, Clone, Deserialize)]
pub struct JwtConfig {
    /// JWT secret key
    pub secret: String,
    
    /// JWT issuer
    #[serde(default = "default_jwt_issuer")]
    pub issuer: String,
    
    /// JWT expiration time in seconds
    #[serde(default = "default_jwt_expiry")]
    pub expiry_secs: u64,
    
    /// Refresh token expiration in seconds
    #[serde(default = "default_refresh_expiry")]
    pub refresh_expiry_secs: u64,
}

fn default_jwt_issuer() -> String { "shellwego".to_string() }
fn default_jwt_expiry() -> u64 { 3600 } // 1 hour
fn default_refresh_expiry() -> u64 { 604800 } // 7 days

/// Federation configuration
#[derive(Debug, Clone, Deserialize)]
pub struct FederationConfig {
    /// Local region identifier
    #[serde(default = "default_region")]
    pub local_region: String,
    
    /// Known peer regions
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
    
    /// Enable cross-region deployments
    #[serde(default)]
    pub cross_region_deploy: bool,
    
    /// Gossip interval in seconds
    #[serde(default = "default_gossip_interval")]
    pub gossip_interval_secs: u64,
}

fn default_gossip_interval() -> u64 { 1 }

/// Peer configuration
#[derive(Debug, Clone, Deserialize)]
pub struct PeerConfig {
    pub region: String,
    pub address: String,
    pub port: u16,
}

/// Build configuration
#[derive(Debug, Clone, Deserialize)]
pub struct BuildConfig {
    /// Maximum concurrent builds
    #[serde(default = "default_max_builds")]
    pub max_concurrent_builds: usize,
    
    /// Build timeout in seconds
    #[serde(default = "default_build_timeout")]
    pub timeout_secs: u64,
    
    /// Build log directory
    #[serde(default = "default_build_log_dir")]
    pub log_dir: String,
    
    /// Docker socket path
    #[serde(default = "default_docker_socket")]
    pub docker_socket: String,
}

fn default_max_builds() -> usize { 4 }
fn default_build_timeout() -> u64 { 600 }
fn default_build_log_dir() -> String { "/var/lib/shellwego/builds/logs".to_string() }
fn default_docker_socket() -> String { "/var/run/docker.sock".to_string() }

/// KMS configuration entry
#[derive(Debug, Clone, Deserialize)]
pub struct KmsConfigEntry {
    /// KMS backend type
    #[serde(default = "default_kms_backend")]
    pub backend: String,
    
    /// Key ID for encryption
    #[serde(default)]
    pub key_id: String,
    
    /// Vault address (if using Vault)
    #[serde(default)]
    pub vault_address: Option<String>,
    
    /// Vault token (if using Vault)
    #[serde(default)]
    pub vault_token: Option<String>,
}

fn default_kms_backend() -> String { "file".to_string() }
fn default_region() -> String { "default".to_string() }
fn default_log_level() -> String { "info".to_string() }

impl Config {
    /// Load configuration from environment and files
    pub fn load() -> anyhow::Result<Self> {
        let bind_addr = std::env::var("BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string());
        
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:/var/lib/shellwego/control-plane.db".to_string());
        
        let jwt_secret = std::env::var("JWT_SECRET")
            .unwrap_or_else(|_| {
                info!("Using development JWT secret - DO NOT USE IN PRODUCTION");
                "dev-secret-change-in-production".to_string()
            });
        
        let default_region = std::env::var("DEFAULT_REGION")
            .unwrap_or_else(|_| "default".to_string());
        
        Ok(Self {
            bind_addr,
            database: DatabaseConfig {
                url: database_url,
                max_connections: default_max_connections(),
                min_connections: default_min_connections(),
                connect_timeout_secs: default_connect_timeout(),
                auto_migrate: true,
                logging: false,
            },
            jwt: JwtConfig {
                secret: jwt_secret,
                issuer: default_jwt_issuer(),
                expiry_secs: default_jwt_expiry(),
                refresh_expiry_secs: default_refresh_expiry(),
            },
            default_region: default_region.clone(),
            log_level: std::env::var("LOG_LEVEL")
                .unwrap_or_else(|_| default_log_level()),
            federation: FederationConfig {
                local_region: default_region,
                peers: Vec::new(),
                cross_region_deploy: false,
                gossip_interval_secs: default_gossip_interval(),
            },
            build: BuildConfig {
                max_concurrent_builds: default_max_builds(),
                timeout_secs: default_build_timeout(),
                log_dir: default_build_log_dir(),
                docker_socket: default_docker_socket(),
            },
            kms: KmsConfigEntry {
                backend: default_kms_backend(),
                key_id: String::new(),
                vault_address: None,
                vault_token: None,
            },
        })
    }
    
    /// Load from file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8080".to_string(),
            database: DatabaseConfig {
                url: "sqlite:/var/lib/shellwego/control-plane.db".to_string(),
                max_connections: default_max_connections(),
                min_connections: default_min_connections(),
                connect_timeout_secs: default_connect_timeout(),
                auto_migrate: true,
                logging: false,
            },
            jwt: JwtConfig {
                secret: "dev-secret-change-in-production".to_string(),
                issuer: default_jwt_issuer(),
                expiry_secs: default_jwt_expiry(),
                refresh_expiry_secs: default_refresh_expiry(),
            },
            default_region: default_region(),
            log_level: default_log_level(),
            federation: FederationConfig {
                local_region: default_region(),
                peers: Vec::new(),
                cross_region_deploy: false,
                gossip_interval_secs: default_gossip_interval(),
            },
            build: BuildConfig {
                max_concurrent_builds: default_max_builds(),
                timeout_secs: default_build_timeout(),
                log_dir: default_build_log_dir(),
                docker_socket: default_docker_socket(),
            },
            kms: KmsConfigEntry {
                backend: default_kms_backend(),
                key_id: String::new(),
                vault_address: None,
                vault_token: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.bind_addr, "0.0.0.0:8080");
        assert_eq!(config.database.max_connections, 10);
        assert_eq!(config.jwt.expiry_secs, 3600);
    }

    #[test]
    fn test_config_load() {
        let config = Config::load();
        assert!(config.is_ok());
    }
}
