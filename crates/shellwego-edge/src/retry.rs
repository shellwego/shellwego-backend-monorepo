//! Retry logic for upstream requests
//!
//! Provides configurable retry policies with support for:
//! - Fixed and exponential backoff strategies
//! - Configurable retryable HTTP status codes
//! - Connection error retry
//! - Maximum retry limit

use std::time::Duration;
use tracing::{debug, warn};

/// Backoff strategy for retries
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BackoffStrategy {
    /// Fixed delay between retries
    #[serde(rename = "fixed")]
    Fixed { delay_ms: u64 },
    /// Exponential backoff: base * 2^attempt
    #[serde(rename = "exponential")]
    Exponential { base_ms: u64, max_ms: u64 },
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        BackoffStrategy::Exponential {
            base_ms: 100,
            max_ms: 5000,
        }
    }
}

/// Retry policy configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries)
    pub max_retries: u32,
    /// Backoff strategy
    pub backoff: BackoffStrategy,
    /// HTTP status codes that trigger retry
    pub retryable_status_codes: Vec<u16>,
    /// Whether to retry on connection errors
    pub retry_on_connection_error: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            backoff: BackoffStrategy::default(),
            retryable_status_codes: vec![502, 503, 504, 429],
            retry_on_connection_error: true,
        }
    }
}

impl RetryPolicy {
    /// Create a no-retry policy
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            backoff: BackoffStrategy::Fixed { delay_ms: 0 },
            retryable_status_codes: vec![],
            retry_on_connection_error: false,
        }
    }

    /// Calculate the delay for the given attempt (0-indexed).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        match self.backoff {
            BackoffStrategy::Fixed { delay_ms } => Duration::from_millis(delay_ms),
            BackoffStrategy::Exponential { base_ms, max_ms } => {
                let delay = base_ms * 2u64.saturating_pow(attempt);
                Duration::from_millis(delay.min(max_ms))
            }
        }
    }

    /// Check if a response status code should trigger a retry.
    pub fn is_retryable_status(&self, status: u16) -> bool {
        self.retryable_status_codes.contains(&status)
    }

    /// Check if retries are enabled (max_retries > 0).
    pub fn is_enabled(&self) -> bool {
        self.max_retries > 0
    }
}

/// Configuration for retry policy at the route level.
///
/// This is a serde-friendly version that mirrors `RetryPolicy` but with
/// optional fields and defaults.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RetryPolicyConfig {
    /// Maximum number of retry attempts
    pub max_retries: Option<u32>,
    /// Backoff strategy
    pub backoff: Option<BackoffStrategy>,
    /// HTTP status codes that trigger retry
    pub retryable_status_codes: Option<Vec<u16>>,
    /// Whether to retry on connection errors
    pub retry_on_connection_error: Option<bool>,
}

impl From<RetryPolicyConfig> for RetryPolicy {
    fn from(config: RetryPolicyConfig) -> Self {
        Self {
            max_retries: config.max_retries.unwrap_or(2),
            backoff: config.backoff.unwrap_or_default(),
            retryable_status_codes: config
                .retryable_status_codes
                .unwrap_or_else(|| vec![502, 503, 504, 429]),
            retry_on_connection_error: config.retry_on_connection_error.unwrap_or(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_retry_policy() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 2);
        assert!(policy.is_enabled());
        assert!(policy.is_retryable_status(502));
        assert!(policy.is_retryable_status(503));
        assert!(policy.is_retryable_status(504));
        assert!(policy.is_retryable_status(429));
        assert!(!policy.is_retryable_status(200));
        assert!(!policy.is_retryable_status(404));
        assert!(!policy.is_retryable_status(500));
    }

    #[test]
    fn test_no_retry_policy() {
        let policy = RetryPolicy::no_retry();
        assert_eq!(policy.max_retries, 0);
        assert!(!policy.is_enabled());
        assert!(!policy.is_retryable_status(502));
    }

    #[test]
    fn test_exponential_backoff() {
        let policy = RetryPolicy {
            backoff: BackoffStrategy::Exponential {
                base_ms: 100,
                max_ms: 5000,
            },
            ..Default::default()
        };

        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));  // 100 * 2^0 = 100
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(200));  // 100 * 2^1 = 200
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(400));  // 100 * 2^2 = 400
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(800));  // 100 * 2^3 = 800
        assert_eq!(policy.delay_for_attempt(4), Duration::from_millis(1600)); // 100 * 2^4 = 1600
        assert_eq!(policy.delay_for_attempt(5), Duration::from_millis(3200)); // 100 * 2^5 = 3200
        assert_eq!(policy.delay_for_attempt(6), Duration::from_millis(5000)); // Capped at max_ms
    }

    #[test]
    fn test_fixed_backoff() {
        let policy = RetryPolicy {
            backoff: BackoffStrategy::Fixed { delay_ms: 250 },
            ..Default::default()
        };

        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(250));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(250));
        assert_eq!(policy.delay_for_attempt(5), Duration::from_millis(250));
    }

    #[test]
    fn test_retry_policy_config_conversion() {
        let config = RetryPolicyConfig {
            max_retries: Some(5),
            backoff: Some(BackoffStrategy::Fixed { delay_ms: 50 }),
            retryable_status_codes: Some(vec![500, 503]),
            retry_on_connection_error: Some(false),
        };

        let policy: RetryPolicy = config.into();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.backoff, BackoffStrategy::Fixed { delay_ms: 50 });
        assert!(policy.is_retryable_status(500));
        assert!(policy.is_retryable_status(503));
        assert!(!policy.is_retryable_status(502));
        assert!(!policy.retry_on_connection_error);
    }

    #[test]
    fn test_retry_policy_config_defaults() {
        let config = RetryPolicyConfig::default();
        let policy: RetryPolicy = config.into();
        assert_eq!(policy.max_retries, 2);
        assert!(policy.retry_on_connection_error);
    }
}
