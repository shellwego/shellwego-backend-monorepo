//! CLI configuration management
//! 
//! Stores auth tokens and defaults in platform-appropriate locations:
//! - Linux: ~/.config/shellwego/config.toml
//! - macOS: ~/Library/Application Support/shellwego/config.toml
//! - Windows: %APPDATA%/shellwego/config.toml

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CliConfig {
    /// API endpoint (e.g., https://api.mypaas.com)
    pub api_url: String,
    
    /// Active authentication token
    pub token: Option<String>,
    
    /// Default organization ID
    pub default_org: Option<uuid::Uuid>,
    
    /// Preferred region for new resources
    pub default_region: Option<String>,
    
    /// Editor for interactive input
    pub editor: Option<String>,
    
    /// Color output preference
    #[serde(default = "default_true")]
    pub color: bool,
    
    /// Auto-update check
    #[serde(default = "default_true")]
    pub auto_update: bool,
}

fn default_true() -> bool { true }

impl CliConfig {
    /// Load config from disk or create default
    pub fn load(override_path: Option<&PathBuf>) -> anyhow::Result<Self> {
        if let Some(path) = override_path {
            let contents = std::fs::read_to_string(path)?;
            let config: CliConfig = toml::from_str(&contents)?;
            return Ok(config);
        }
        
        // Use confy for platform-appropriate path
        let config: CliConfig = confy::load("shellwego", "config")?;
        
        // If empty (first run), set defaults
        if config.api_url.is_empty() {
            Ok(CliConfig {
                api_url: "http://localhost:8080".to_string(),
                ..config
            })
        } else {
            Ok(config)
        }
    }
    
    /// Save config to disk
    pub fn save(&self) -> anyhow::Result<()> {
        confy::store("shellwego", "config", self)?;
        Ok(())
    }
    
    /// Get path to config file
    pub fn path() -> anyhow::Result<PathBuf> {
        let proj_dirs = directories::ProjectDirs::from("com", "shellwego", "cli")
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
            
        Ok(proj_dirs.config_local_dir().join("config.toml"))
    }
    
    /// Store token in system keyring if available, else plaintext
    pub fn set_token(&mut self, token: String) -> anyhow::Result<()> {
        // Try keyring first
        if let Ok(entry) = keyring::Entry::new("shellwego", "api_token") {
            if entry.set_password(&token).is_ok() {
                self.token = Some("keyring://api_token".to_string());
                return Ok(());
            }
        }
        
        // Fallback to plaintext (dev mode warning)
        self.token = Some(token);
        Ok(())
    }
    
    /// Retrieve token (from keyring or config)
    pub fn get_token(&self) -> Option<String> {
        if let Some(ref token) = self.token {
            if token == "keyring://api_token" {
                if let Ok(entry) = keyring::Entry::new("shellwego", "api_token") {
                    return entry.get_password().ok();
                }
            }
            return Some(token.clone());
        }
        None
    }
    
    /// Clear authentication
    pub fn clear_auth(&mut self) {
        if let Some(ref token) = self.token {
            if token == "keyring://api_token" {
                let _ = keyring::Entry::new("shellwego", "api_token")
                    .and_then(|e| e.delete_password());
            }
        }
        self.token = None;
    }
}