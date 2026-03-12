//! Drive (block device) and PMEM types

use serde::{Serialize, Deserialize};
use super::network::RateLimiter;

/// Drive descriptor for virtio-block or vhost-user-block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Drive {
    /// Unique drive identifier.
    pub drive_id: String,
    /// Whether this is the root device.
    pub is_root_device: bool,
    /// Partition UUID for boot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partuuid: Option<String>,
    /// Caching strategy for the block device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_type: Option<CacheType>,

    // Virtio-block specific parameters
    /// Is block read only (virtio-block only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_read_only: Option<bool>,
    /// Host level path for the guest drive (virtio-block only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_on_host: Option<String>,
    /// Rate limiter configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiter: Option<RateLimiter>,
    /// IO engine type (virtio-block only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_engine: Option<IoEngine>,

    // Vhost-user-block specific parameters
    /// Path to vhost-user-block backend socket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket: Option<String>,
}

/// Cache type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum CacheType {
    Unsafe,
    Writeback,
}

impl Default for CacheType {
    fn default() -> Self {
        Self::Unsafe
    }
}

/// IO engine type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum IoEngine {
    Sync,
    Async,
}

impl Default for IoEngine {
    fn default() -> Self {
        Self::Sync
    }
}

/// Partial drive for PATCH operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct PartialDrive {
    /// Drive identifier.
    pub drive_id: String,
    /// Host level path for the guest drive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_on_host: Option<String>,
    /// Rate limiter configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiter: Option<RateLimiter>,
}

/// Persistent memory device descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Pmem {
    /// Device identifier.
    pub id: String,
    /// Host level path for the backing file.
    pub path_on_host: String,
    /// Make this device the root device for boot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_device: Option<bool>,
    /// Map backing file in read-only mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
}
