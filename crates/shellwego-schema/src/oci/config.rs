//! OCI Image Configuration types
//!
//! Image configuration defines the runtime behavior of a container,
//! including environment variables, entrypoint, command, and filesystem.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "openapi")]
use schemars::JsonSchema;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

fn default_architecture() -> String {
    "amd64".to_string()
}

fn default_os() -> String {
    "linux".to_string()
}

/// Image configuration
///
/// The configuration blob contains metadata about the image and
/// the runtime configuration for containers created from it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct ImageConfig {
    /// ISO 8601 timestamp when image was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,

    /// Author of the image
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// CPU architecture
    #[serde(default = "default_architecture")]
    pub architecture: String,

    /// Operating system
    #[serde(default = "default_os")]
    pub os: String,

    /// Container runtime configuration
    #[serde(default)]
    pub config: ContainerConfig,

    /// Root filesystem information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rootfs: Option<RootFs>,

    /// Layer history
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
}

impl ImageConfig {
    /// Create a new image config
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with architecture and OS
    pub fn with_platform(architecture: impl Into<String>, os: impl Into<String>) -> Self {
        Self {
            architecture: architecture.into(),
            os: os.into(),
            ..Self::default()
        }
    }

    /// Set environment variables
    pub fn with_env(mut self, env: Vec<String>) -> Self {
        self.config.env = env;
        self
    }

    /// Add an environment variable
    pub fn add_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.env.push(format!("{}={}", key.into(), value.into()));
        self
    }

    /// Set entrypoint
    pub fn with_entrypoint(mut self, entrypoint: Vec<String>) -> Self {
        self.config.entrypoint = Some(entrypoint);
        self
    }

    /// Set command
    pub fn with_cmd(mut self, cmd: Vec<String>) -> Self {
        self.config.cmd = Some(cmd);
        self
    }

    /// Set working directory
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.config.working_dir = Some(dir.into());
        self
    }

    /// Set user
    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.config.user = Some(user.into());
        self
    }

    /// Add a label
    pub fn add_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.labels.insert(key.into(), value.into());
        self
    }

    /// Expose a port
    pub fn expose_port(mut self, port: impl Into<String>) -> Self {
        self.config.exposed_ports.insert(port.into(), serde_json::Value::Object(serde_json::Map::new()));
        self
    }

    /// Add a volume
    pub fn add_volume(mut self, path: impl Into<String>) -> Self {
        self.config.volumes.insert(path.into(), serde_json::Value::Object(serde_json::Map::new()));
        self
    }

    /// Get an environment variable value
    pub fn get_env(&self, key: &str) -> Option<&str> {
        self.config.env.iter()
            .find(|e| e.starts_with(&format!("{}=", key)))
            .map(|e| &e[key.len() + 1..])
    }

    /// Check if the image is for Linux
    pub fn is_linux(&self) -> bool {
        self.os == "linux"
    }

    /// Check if the image is for AMD64
    pub fn is_amd64(&self) -> bool {
        self.architecture == "amd64" || self.architecture == "x86_64"
    }
}

/// Container runtime configuration
///
/// These settings define how a container should be run,
/// including environment, command, and exposed ports.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct ContainerConfig {
    /// Environment variables (KEY=VALUE format)
    #[serde(default)]
    pub Env: Vec<String>,

    /// Entrypoint (the command to run)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub Entrypoint: Option<Vec<String>>,

    /// Command arguments (passed to entrypoint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub Cmd: Option<Vec<String>>,

    /// Working directory inside container
    #[serde(skip_serializing_if = "Option::is_none")]
    pub WorkingDir: Option<String>,

    /// User to run as (uid or uid:gid or username)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub User: Option<String>,

    /// Exposed ports
    #[serde(default)]
    pub ExposedPorts: HashMap<String, serde_json::Value>,

    /// Volume mount points
    #[serde(default)]
    pub Volumes: HashMap<String, serde_json::Value>,

    /// Labels (metadata key-value pairs)
    #[serde(default)]
    pub Labels: HashMap<String, String>,

    /// Stop signal (e.g., "SIGTERM")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub StopSignal: Option<String>,

    /// Args escaped (Windows specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ArgsEscaped: Option<bool>,
}

impl ContainerConfig {
    /// Create a new container config
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with entrypoint
    pub fn with_entrypoint(entrypoint: Vec<String>) -> Self {
        Self {
            Entrypoint: Some(entrypoint),
            ..Self::default()
        }
    }

    /// Create with command
    pub fn with_cmd(cmd: Vec<String>) -> Self {
        Self {
            Cmd: Some(cmd),
            ..Self::default()
        }
    }
}

/// Root filesystem information
///
/// Describes the layered filesystem of the container image.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct RootFs {
    /// Type of filesystem (typically "layers")
    #[serde(rename = "type")]
    pub fs_type: String,

    /// Layer diff IDs (chain IDs, not distribution digests)
    #[serde(default)]
    pub diff_ids: Vec<String>,
}

impl Default for RootFs {
    fn default() -> Self {
        Self {
            fs_type: "layers".to_string(),
            diff_ids: Vec::new(),
        }
    }
}

impl RootFs {
    /// Create a new rootfs
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a layer diff ID
    pub fn add_diff_id(mut self, diff_id: impl Into<String>) -> Self {
        self.diff_ids.push(diff_id.into());
        self
    }

    /// Get the number of layers
    pub fn layer_count(&self) -> usize {
        self.diff_ids.len()
    }
}

/// History entry for a layer
///
/// Describes how a layer was created, useful for understanding
/// the provenance of filesystem changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(JsonSchema, ToSchema))]
pub struct HistoryEntry {
    /// ISO 8601 timestamp when layer was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,

    /// Author of the layer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Command that created this layer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Whether this is an empty layer (no filesystem changes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_layer: Option<bool>,
}

impl HistoryEntry {
    /// Create a new history entry
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from a Dockerfile instruction
    pub fn from_dockerfile(instruction: impl Into<String>) -> Self {
        Self {
            created_by: Some(format!("/bin/sh -c {}", instruction.into())),
            ..Self::default()
        }
    }

    /// Mark as empty layer
    pub fn as_empty(mut self) -> Self {
        self.empty_layer = Some(true);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_config_new() {
        let config = ImageConfig::new();
        assert_eq!(config.architecture, "amd64");
        assert_eq!(config.os, "linux");
    }

    #[test]
    fn test_image_config_env() {
        let config = ImageConfig::new()
            .add_env("PATH", "/usr/bin:/bin")
            .add_env("HOME", "/root");

        assert_eq!(config.get_env("PATH"), Some("/usr/bin:/bin"));
        assert_eq!(config.get_env("HOME"), Some("/root"));
        assert_eq!(config.get_env("MISSING"), None);
    }

    #[test]
    fn test_image_config_builder() {
        let config = ImageConfig::new()
            .with_platform("arm64", "linux")
            .with_entrypoint(vec!["/bin/sh".to_string()])
            .with_cmd(vec!["-c".to_string(), "echo hello".to_string()])
            .with_working_dir("/app")
            .with_user("appuser")
            .add_label("version", "1.0.0");

        assert_eq!(config.architecture, "arm64");
        assert_eq!(config.os, "linux");
        assert!(config.config.entrypoint.is_some());
        assert!(config.config.cmd.is_some());
        assert_eq!(config.config.working_dir, Some("/app".to_string()));
        assert_eq!(config.config.user, Some("appuser".to_string()));
        assert_eq!(config.config.labels.get("version"), Some(&"1.0.0".to_string()));
    }

    #[test]
    fn test_container_config_serialization() {
        let config = ContainerConfig::new();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"Env\":[]"));
    }

    #[test]
    fn test_root_fs() {
        let rootfs = RootFs::new()
            .add_diff_id("sha256:layer1")
            .add_diff_id("sha256:layer2");

        assert_eq!(rootfs.fs_type, "layers");
        assert_eq!(rootfs.layer_count(), 2);
    }

    #[test]
    fn test_history_entry() {
        let entry = HistoryEntry::from_dockerfile("RUN apt-get update")
            .as_empty();

        assert!(entry.created_by.unwrap().contains("apt-get update"));
        assert_eq!(entry.empty_layer, Some(true));
    }

    #[test]
    fn test_image_config_deserialization() {
        let json = r#"{
            "architecture": "arm64",
            "os": "linux",
            "config": {
                "Env": ["PATH=/usr/bin"],
                "Entrypoint": ["/bin/sh"]
            },
            "rootfs": {
                "type": "layers",
                "diff_ids": ["sha256:abc123"]
            }
        }"#;

        let config: ImageConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.architecture, "arm64");
        assert_eq!(config.config.env.len(), 1);
    }
}
