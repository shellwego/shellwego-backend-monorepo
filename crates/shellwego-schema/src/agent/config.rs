//! Agent configuration types
//!
//! Configuration for agent nodes and their behavior.

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

use crate::vmm::VirtualizationMode;

/// Agent configuration
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Node ID (assigned after registration)
    pub node_id: Option<Uuid>,
    /// Control plane URL
    pub control_plane_url: String,
    /// Join token for authentication
    pub join_token: Option<SecretString>,
    /// Region where the agent is running
    pub region: String,
    /// Zone within the region
    pub zone: String,
    /// Custom labels for the node
    pub labels: HashMap<String, String>,
    /// Path to standard Firecracker binary (KVM mode)
    pub firecracker_binary: PathBuf,
    /// Path to PVM-enabled Firecracker binary (PVM mode)
    pub firecracker_pvm_binary: PathBuf,
    /// Path to kernel image
    pub kernel_image_path: PathBuf,
    /// Data directory for the agent
    pub data_dir: PathBuf,
    /// Maximum number of microVMs this agent can run
    pub max_microvms: u32,
    /// Reserved memory in MB
    pub reserved_memory_mb: u64,
    /// Reserved CPU percentage
    pub reserved_cpu_percent: f64,
    /// Force a specific virtualization mode
    pub force_mode: Option<VirtualizationMode>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            node_id: None,
            control_plane_url: "127.0.0.1:4433".to_string(),
            join_token: None,
            region: "unknown".to_string(),
            zone: "unknown".to_string(),
            labels: HashMap::new(),
            firecracker_binary: PathBuf::from("/usr/local/bin/firecracker"),
            firecracker_pvm_binary: PathBuf::from("/usr/local/bin/firecracker-pvm"),
            kernel_image_path: PathBuf::from("/var/lib/shellwego/vmlinux"),
            data_dir: PathBuf::from("/var/lib/shellwego"),
            max_microvms: 500,
            reserved_memory_mb: 512,
            reserved_cpu_percent: 10.0,
            force_mode: None,
        }
    }
}

impl AgentConfig {
    /// Create a new agent config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from environment variables
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            node_id: std::env::var("SHELLWEGO__NODE_ID")
                .ok()
                .and_then(|s| s.parse().ok()),
            control_plane_url: std::env::var("SHELLWEGO__CONTROL_PLANE_URL")
                .unwrap_or_else(|_| "127.0.0.1:4433".to_string()),
            join_token: std::env::var("SHELLWEGO__JOIN_TOKEN")
                .ok()
                .map(|s| SecretString::new(s.into())),
            region: std::env::var("SHELLWEGO__REGION")
                .unwrap_or_else(|_| "unknown".to_string()),
            zone: std::env::var("SHELLWEGO__ZONE")
                .unwrap_or_else(|_| "unknown".to_string()),
            labels: HashMap::new(),
            firecracker_binary: PathBuf::from(
                std::env::var("SHELLWEGO__FIRECRACKER_BINARY")
                    .unwrap_or_else(|_| "/usr/local/bin/firecracker".to_string())
            ),
            firecracker_pvm_binary: PathBuf::from(
                std::env::var("SHELLWEGO__FIRECRACKER_PVM_BINARY")
                    .unwrap_or_else(|_| "/usr/local/bin/firecracker-pvm".to_string())
            ),
            kernel_image_path: PathBuf::from(
                std::env::var("SHELLWEGO__KERNEL_IMAGE_PATH")
                    .unwrap_or_else(|_| "/var/lib/shellwego/vmlinux".to_string())
            ),
            data_dir: PathBuf::from(
                std::env::var("SHELLWEGO__DATA_DIR")
                    .unwrap_or_else(|_| "/var/lib/shellwego".to_string())
            ),
            max_microvms: std::env::var("SHELLWEGO__MAX_MICROVMS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(500),
            reserved_memory_mb: std::env::var("SHELLWEGO__RESERVED_MEMORY_MB")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(512),
            reserved_cpu_percent: std::env::var("SHELLWEGO__RESERVED_CPU_PERCENT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10.0),
            force_mode: None,
        })
    }

    /// Set the node ID
    pub fn with_node_id(mut self, id: Uuid) -> Self {
        self.node_id = Some(id);
        self
    }

    /// Set the control plane URL
    pub fn with_control_plane_url(mut self, url: &str) -> Self {
        self.control_plane_url = url.to_string();
        self
    }

    /// Set the join token
    pub fn with_join_token(mut self, token: SecretString) -> Self {
        self.join_token = Some(token);
        self
    }

    /// Set the region
    pub fn with_region(mut self, region: &str) -> Self {
        self.region = region.to_string();
        self
    }

    /// Set the zone
    pub fn with_zone(mut self, zone: &str) -> Self {
        self.zone = zone.to_string();
        self
    }

    /// Add a label
    pub fn with_label(mut self, key: &str, value: &str) -> Self {
        self.labels.insert(key.to_string(), value.to_string());
        self
    }

    /// Set the data directory
    pub fn with_data_dir(mut self, path: PathBuf) -> Self {
        self.data_dir = path;
        self
    }

    /// Set max microvms
    pub fn with_max_microvms(mut self, count: u32) -> Self {
        self.max_microvms = count;
        self
    }

    /// Force a specific virtualization mode
    pub fn with_force_mode(mut self, mode: VirtualizationMode) -> Self {
        self.force_mode = Some(mode);
        self
    }

    /// Get the Firecracker binary path for the given mode
    pub fn firecracker_binary_for_mode(&self, mode: VirtualizationMode) -> &PathBuf {
        match mode {
            VirtualizationMode::Kvm => &self.firecracker_binary,
            VirtualizationMode::Pvm => &self.firecracker_pvm_binary,
            VirtualizationMode::Wasm => &self.firecracker_binary, // Not used for WASM
        }
    }
}

/// Serializable version of AgentConfig (without secrets)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct AgentConfigJson {
    /// Node ID
    pub node_id: Option<Uuid>,
    /// Control plane URL
    pub control_plane_url: String,
    /// Whether join token is set
    pub has_join_token: bool,
    /// Region
    pub region: String,
    /// Zone
    pub zone: String,
    /// Labels
    pub labels: HashMap<String, String>,
    /// Firecracker binary path
    pub firecracker_binary: String,
    /// PVM binary path
    pub firecracker_pvm_binary: String,
    /// Kernel image path
    pub kernel_image_path: String,
    /// Data directory
    pub data_dir: String,
    /// Max microVMs
    pub max_microvms: u32,
    /// Reserved memory MB
    pub reserved_memory_mb: u64,
    /// Reserved CPU percent
    pub reserved_cpu_percent: f64,
    /// Force mode
    pub force_mode: Option<VirtualizationMode>,
}

impl From<&AgentConfig> for AgentConfigJson {
    fn from(config: &AgentConfig) -> Self {
        Self {
            node_id: config.node_id,
            control_plane_url: config.control_plane_url.clone(),
            has_join_token: config.join_token.is_some(),
            region: config.region.clone(),
            zone: config.zone.clone(),
            labels: config.labels.clone(),
            firecracker_binary: config.firecracker_binary.to_string_lossy().to_string(),
            firecracker_pvm_binary: config.firecracker_pvm_binary.to_string_lossy().to_string(),
            kernel_image_path: config.kernel_image_path.to_string_lossy().to_string(),
            data_dir: config.data_dir.to_string_lossy().to_string(),
            max_microvms: config.max_microvms,
            reserved_memory_mb: config.reserved_memory_mb,
            reserved_cpu_percent: config.reserved_cpu_percent,
            force_mode: config.force_mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert!(config.node_id.is_none());
        assert_eq!(config.control_plane_url, "127.0.0.1:4433");
        assert_eq!(config.max_microvms, 500);
    }

    #[test]
    fn test_agent_config_builder() {
        let config = AgentConfig::new()
            .with_region("us-west-2")
            .with_zone("us-west-2a")
            .with_label("environment", "production")
            .with_max_microvms(100);

        assert_eq!(config.region, "us-west-2");
        assert_eq!(config.zone, "us-west-2a");
        assert_eq!(config.labels.get("environment"), Some(&"production".to_string()));
        assert_eq!(config.max_microvms, 100);
    }

    #[test]
    fn test_agent_config_binary_for_mode() {
        let config = AgentConfig::default();

        let kvm_path = config.firecracker_binary_for_mode(VirtualizationMode::Kvm);
        assert!(kvm_path.to_str().unwrap().contains("firecracker"));

        let pvm_path = config.firecracker_binary_for_mode(VirtualizationMode::Pvm);
        assert!(pvm_path.to_str().unwrap().contains("firecracker-pvm"));
    }

    #[test]
    fn test_agent_config_json_from_config() {
        let config = AgentConfig::new()
            .with_region("us-east-1")
            .with_label("tier", "compute");

        let json = AgentConfigJson::from(&config);
        assert_eq!(json.region, "us-east-1");
        assert_eq!(json.labels.get("tier"), Some(&"compute".to_string()));
        assert!(!json.has_join_token);
    }
}
