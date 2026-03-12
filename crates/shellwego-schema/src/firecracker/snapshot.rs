//! Snapshot types

use serde::{Serialize, Deserialize};

/// Snapshot creation parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct SnapshotCreateParams {
    /// Path to the file for guest memory.
    pub mem_file_path: String,
    /// Path to the file for microVM state.
    pub snapshot_path: String,
    /// Type of snapshot (Full or Diff).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_type: Option<SnapshotType>,
}

/// Snapshot type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum SnapshotType {
    Full,
    Diff,
}

impl Default for SnapshotType {
    fn default() -> Self {
        Self::Full
    }
}

/// Snapshot load parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct SnapshotLoadParams {
    /// Path to the file containing microVM state.
    pub snapshot_path: String,
    /// Path to the file containing guest memory (deprecated, use mem_backend).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_file_path: Option<String>,
    /// Memory backend configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_backend: Option<MemoryBackend>,
    /// Enable dirty page tracking for diff snapshots (deprecated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_diff_snapshots: Option<bool>,
    /// Enable dirty page tracking for diff snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_dirty_pages: Option<bool>,
    /// Resume VM after loading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_vm: Option<bool>,
    /// Network device overrides for snapshot restore.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_overrides: Option<Vec<NetworkOverride>>,
}

/// Memory backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MemoryBackend {
    /// Backend type (File or Uffd).
    pub backend_type: MemoryBackendType,
    /// Path to file or UDS.
    pub backend_path: String,
}

/// Memory backend type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum MemoryBackendType {
    File,
    Uffd,
}

/// Network override for snapshot restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct NetworkOverride {
    /// Interface ID to modify.
    pub iface_id: String,
    /// New host device name.
    pub host_dev_name: String,
}
