//! Balloon device types

use serde::{Serialize, Deserialize};

/// Balloon device descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Balloon {
    /// Target balloon size in MiB.
    pub amount_mib: i64,
    /// Whether the balloon should deflate on OOM.
    pub deflate_on_oom: bool,
    /// Interval in seconds between refreshing statistics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats_polling_interval_s: Option<i64>,
    /// Enable free page hinting feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_page_hinting: Option<bool>,
    /// Enable free page reporting feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_page_reporting: Option<bool>,
}

/// Balloon update for PATCH operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BalloonUpdate {
    /// Target balloon size in MiB.
    pub amount_mib: i64,
}

/// Balloon device statistics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BalloonStats {
    /// Target number of pages.
    pub target_pages: i64,
    /// Actual number of pages.
    pub actual_pages: i64,
    /// Target memory in MiB.
    pub target_mib: i64,
    /// Actual memory in MiB.
    pub actual_mib: i64,
    /// Memory swapped in (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_in: Option<i64>,
    /// Memory swapped out (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_out: Option<i64>,
    /// Major page faults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub major_faults: Option<i64>,
    /// Minor page faults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor_faults: Option<i64>,
    /// Free memory (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_memory: Option<i64>,
    /// Total memory (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_memory: Option<i64>,
    /// Available memory (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_memory: Option<i64>,
    /// Disk caches (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_caches: Option<i64>,
    /// Successful hugetlb allocations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hugetlb_allocations: Option<i64>,
    /// Failed hugetlb allocations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hugetlb_failures: Option<i64>,
    /// OOM killer invocations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oom_kill: Option<i64>,
}

/// Balloon statistics update.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BalloonStatsUpdate {
    /// Statistics polling interval in seconds.
    pub stats_polling_interval_s: i64,
}

/// Command to start free page hinting.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BalloonStartCmd {
    /// Auto-acknowledge when guest submits done cmd.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledge_on_stop: Option<bool>,
}

/// Free page hinting status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BalloonHintingStatus {
    /// Last command from host.
    pub host_cmd: i64,
    /// Last command from guest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_cmd: Option<i64>,
}
