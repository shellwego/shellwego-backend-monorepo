//! Container image registry cache and pull operations
//! 
//! Integrates with skopeo/umoci for OCI image handling.

use thiserror::Error;
use serde::{Deserialize, Serialize};

pub mod cache;
pub mod pull;

// Re-export commonly used types
pub use cache::{LayerCache, CachedImageInfo, CacheStats, Manifest as CacheManifest, Descriptor, LayerInfo};
pub use pull::{ImagePuller, PulledImage, PullProgress, ImageReference};

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("Image not found: {0}")]
    NotFound(String),
    
    #[error("Pull failed: {0}")]
    PullFailed(String),
    
    #[error("Cache corrupted: {0}")]
    CacheCorrupted(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("HTTP error: {0}")]
    Http(String),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
}

/// Registry backend trait for pluggable image sources
#[async_trait::async_trait]
pub trait RegistryBackend: Send + Sync {
    /// Authenticate to remote registry
    async fn authenticate(&self, creds: &RegistryAuth) -> Result<AuthToken, RegistryError>;
    
    /// Check if image exists in remote
    async fn exists(&self, image_ref: &str) -> Result<bool, RegistryError>;
    
    /// Pull image manifest
    async fn pull_manifest(&self, image_ref: &str) -> Result<Manifest, RegistryError>;
    
    /// Pull layer blob
    async fn pull_layer(&self, digest: &str) -> Result<Vec<u8>, RegistryError>;
    
    /// Get image config
    async fn get_config(&self, image_ref: &str) -> Result<ImageConfig, RegistryError>;
}

/// Authentication token
#[derive(Debug, Clone)]
pub struct AuthToken {
    /// Token value
    pub token: String,
    /// Token type (usually "Bearer")
    pub token_type: String,
    /// Expiration time in seconds
    pub expires_in: Option<u64>,
}

/// Registry authentication credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryAuth {
    /// Username for basic auth
    pub username: Option<String>,
    /// Password for basic auth
    pub password: Option<String>,
    /// Pre-existing token
    pub token: Option<String>,
    /// Registry URL (for ECR, GCR specific auth)
    pub registry_url: Option<String>,
}

impl RegistryAuth {
    /// Create anonymous auth (public images)
    pub fn anonymous() -> Self {
        Self {
            username: None,
            password: None,
            token: None,
            registry_url: None,
        }
    }

    /// Create basic auth
    pub fn basic(username: &str, password: &str) -> Self {
        Self {
            username: Some(username.to_string()),
            password: Some(password.to_string()),
            token: None,
            registry_url: None,
        }
    }

    /// Create token auth
    pub fn token(token: &str) -> Self {
        Self {
            username: None,
            password: None,
            token: Some(token.to_string()),
            registry_url: None,
        }
    }
}

/// OCI image manifest structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Schema version (usually 2)
    #[serde(rename = "schemaVersion", default = "default_schema_version")]
    pub schema_version: i32,
    
    /// Media type
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    
    /// Image config descriptor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ConfigDescriptor>,
    
    /// Layer descriptors
    #[serde(default)]
    pub layers: Vec<LayerDescriptor>,
    
    /// Manifest list entries (for multi-arch)
    #[serde(default)]
    pub manifests: Vec<ManifestDescriptor>,
    
    /// Annotations
    #[serde(default)]
    pub annotations: std::collections::HashMap<String, String>,
}

fn default_schema_version() -> i32 { 2 }

/// Config descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDescriptor {
    /// Media type
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    
    /// Content digest
    pub digest: String,
    
    /// Size in bytes
    pub size: u64,
}

/// Layer descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerDescriptor {
    /// Media type
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    
    /// Content digest
    pub digest: String,
    
    /// Size in bytes
    pub size: u64,
    
    /// URLs for direct download
    #[serde(default)]
    pub urls: Vec<String>,
    
    /// Annotations
    #[serde(default)]
    pub annotations: std::collections::HashMap<String, String>,
}

/// Manifest descriptor (for index/list)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestDescriptor {
    /// Media type
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    
    /// Content digest
    pub digest: String,
    
    /// Size in bytes
    pub size: u64,
    
    /// Platform
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
    
    /// Annotations
    #[serde(default)]
    pub annotations: std::collections::HashMap<String, String>,
}

/// Platform specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    /// CPU architecture
    pub architecture: String,
    
    /// Operating system
    pub os: String,
    
    /// CPU variant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    
    /// OS version
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "os.version")]
    pub os_version: Option<String>,
    
    /// OS features
    #[serde(default)]
    #[serde(rename = "os.features")]
    pub os_features: Vec<String>,
}

/// Image configuration (env, cmd, entrypoint, etc)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageConfig {
    /// Created timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    
    /// Author
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    
    /// Architecture
    #[serde(default = "default_architecture")]
    pub architecture: String,
    
    /// OS
    #[serde(default = "default_os")]
    pub os: String,
    
    /// Container configuration
    #[serde(default)]
    pub config: ContainerConfig,
    
    /// Root filesystem
    #[serde(default)]
    pub rootfs: Option<RootFs>,
    
    /// History entries
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
}

fn default_architecture() -> String { "amd64".to_string() }
fn default_os() -> String { "linux".to_string() }

/// Container configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Environment variables
    #[serde(default)]
    pub Env: Vec<String>,
    
    /// Entry point
    #[serde(skip_serializing_if = "Option::is_none")]
    pub Entrypoint: Option<Vec<String>>,
    
    /// Command arguments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub Cmd: Option<Vec<String>>,
    
    /// Working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub WorkingDir: Option<String>,
    
    /// User
    #[serde(skip_serializing_if = "Option::is_none")]
    pub User: Option<String>,
    
    /// Exposed ports
    #[serde(default)]
    pub ExposedPorts: std::collections::HashMap<String, serde_json::Value>,
    
    /// Volumes
    #[serde(default)]
    pub Volumes: std::collections::HashMap<String, serde_json::Value>,
    
    /// Labels
    #[serde(default)]
    pub Labels: std::collections::HashMap<String, String>,
    
    /// Stop signal
    #[serde(skip_serializing_if = "Option::is_none")]
    pub StopSignal: Option<String>,
    
    /// Args escaped (Windows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ArgsEscaped: Option<bool>,
}

/// Root filesystem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootFs {
    /// Type (usually "layers")
    #[serde(rename = "type")]
    pub fs_type: String,
    
    /// Layer diff IDs
    #[serde(default)]
    pub diff_ids: Vec<String>,
}

/// History entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Created timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    
    /// Author
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    
    /// Command that created this layer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    
    /// Comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    
    /// Whether layer is empty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_layer: Option<bool>,
}

impl Default for RootFs {
    fn default() -> Self {
        Self {
            fs_type: "layers".to_string(),
            diff_ids: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_auth_anonymous() {
        let auth = RegistryAuth::anonymous();
        assert!(auth.username.is_none());
        assert!(auth.token.is_none());
    }

    #[test]
    fn test_registry_auth_basic() {
        let auth = RegistryAuth::basic("user", "pass");
        assert_eq!(auth.username, Some("user".to_string()));
        assert_eq!(auth.password, Some("pass".to_string()));
    }

    #[test]
    fn test_registry_auth_token() {
        let auth = RegistryAuth::token("mytoken");
        assert_eq!(auth.token, Some("mytoken".to_string()));
    }

    #[test]
    fn test_manifest_deserialization() {
        let json = r#"{
            "schemaVersion": 2,
            "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
            "config": {
                "mediaType": "application/vnd.docker.container.image.v1+json",
                "digest": "sha256:abc123",
                "size": 1234
            },
            "layers": []
        }"#;

        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.schema_version, 2);
    }

    #[test]
    fn test_image_config_default() {
        let config = ImageConfig::default();
        assert_eq!(config.architecture, "amd64");
        assert_eq!(config.os, "linux");
    }

    #[test]
    fn test_container_config_env() {
        let mut config = ContainerConfig::default();
        config.Env.push("PATH=/usr/bin".to_string());
        assert_eq!(config.Env.len(), 1);
    }
}
