//! Device types (Vsock, Entropy, Serial)

use serde::{Serialize, Deserialize};
use super::network::RateLimiter;

/// Vsock device descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Vsock {
    /// Guest Vsock CID (must be >= 3).
    pub guest_cid: i64,
    /// Path to UNIX domain socket for proxying connections.
    pub uds_path: String,
    /// Deprecated vsock ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vsock_id: Option<String>,
}

/// Entropy device descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct EntropyDevice {
    /// Rate limiter configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiter: Option<RateLimiter>,
}

/// Serial console configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct SerialDevice {
    /// Path to file or named pipe for serial output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_out_path: Option<String>,
}
