//! Firecracker metrics data types

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Firecracker metrics data (emitted to FIFO).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct FirecrackerMetrics {
    /// UTC timestamp in milliseconds.
    pub utc_time_ms: u64,
    /// API server metrics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_server: Option<serde_json::Value>,
    /// VMM metrics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vmm: Option<VmmMetrics>,
    /// Network metrics per interface.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<HashMap<String, NetMetrics>>,
    /// Block metrics per drive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<HashMap<String, BlockMetrics>>,
}

/// VMM metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct VmmMetrics {
    /// RX bytes.
    #[serde(default)]
    pub rx_bytes: u64,
    /// TX bytes.
    #[serde(default)]
    pub tx_bytes: u64,
}

/// Network interface metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct NetMetrics {
    /// RX bytes count.
    pub rx_bytes_count: u64,
    /// TX bytes count.
    pub tx_bytes_count: u64,
    /// RX packets count.
    pub rx_packets_count: u64,
    /// TX packets count.
    pub tx_packets_count: u64,
    /// RX drops count.
    pub rx_drops_count: u64,
    /// TX drops count.
    pub tx_drops_count: u64,
}

/// Block device metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BlockMetrics {
    /// Bytes read.
    pub read_bytes: u64,
    /// Bytes written.
    pub write_bytes: u64,
    /// Read operation count.
    pub read_count: u64,
    /// Write operation count.
    pub write_count: u64,
    /// Flush operation count.
    pub flush_count: u64,
}
