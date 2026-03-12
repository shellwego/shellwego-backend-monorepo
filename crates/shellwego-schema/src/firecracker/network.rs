//! Network interface and rate limiting types

use serde::{Serialize, Deserialize};

/// Network interface descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct NetworkInterface {
    /// Network interface identifier.
    pub iface_id: String,
    /// Host level path for the guest network interface (TAP device).
    pub host_dev_name: String,
    /// Guest MAC address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_mac: Option<String>,
    /// RX rate limiter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_rate_limiter: Option<RateLimiter>,
    /// TX rate limiter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_rate_limiter: Option<RateLimiter>,
}

/// Partial network interface for PATCH operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct PartialNetworkInterface {
    /// Network interface identifier.
    pub iface_id: String,
    /// RX rate limiter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_rate_limiter: Option<RateLimiter>,
    /// TX rate limiter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_rate_limiter: Option<RateLimiter>,
}

/// IO rate limiter with independent bytes/s and ops/s limits.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct RateLimiter {
    /// Token bucket with bytes as tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<TokenBucket>,
    /// Token bucket with operations as tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ops: Option<TokenBucket>,
}

/// Token bucket for rate limiting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct TokenBucket {
    /// The total number of tokens this bucket can hold.
    pub size: i64,
    /// The initial size of a token bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_time_burst: Option<i64>,
    /// The amount of milliseconds it takes for the bucket to refill.
    pub refill_time: i64,
}
