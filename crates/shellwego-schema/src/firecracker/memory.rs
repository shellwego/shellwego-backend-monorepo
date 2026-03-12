//! Memory hotplug types

use serde::{Serialize, Deserialize};

/// Memory hotplug configuration (virtio-mem).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MemoryHotplugConfig {
    /// Total size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size_mib: Option<i64>,
    /// Slot size in MiB (min: 128).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_size_mib: Option<i64>,
    /// Block size in MiB (min: 2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_size_mib: Option<i64>,
}

/// Memory hotplug size update.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MemoryHotplugSizeUpdate {
    /// New target region size in MiB.
    pub requested_size_mib: i64,
}

/// Memory hotplug status.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MemoryHotplugStatus {
    /// Total size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size_mib: Option<i64>,
    /// Slot size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_size_mib: Option<i64>,
    /// Block size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_size_mib: Option<i64>,
    /// Plugged size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugged_size_mib: Option<i64>,
    /// Requested size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_size_mib: Option<i64>,
}
