//! Rate limiting service
//!
//! Token bucket rate limiting with memory and Redis backends.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Rate limiter configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimiterConfig {
    /// Default requests per second
    pub default_rps: u32,
    /// Default burst size
    pub default_burst: u32,
    /// Backend type
    pub backend: RateLimitBackend,
    /// Key prefix for distributed storage
    pub key_prefix: String,
    /// Cleanup interval for in-memory tokens
    pub cleanup_interval_secs: u64,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            default_rps: 100,
            default_burst: 200,
            backend: RateLimitBackend::Memory,
            key_prefix: "shellwego:ratelimit".to_string(),
            cleanup_interval_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RateLimitBackend {
    Memory,
    Redis { url: String },
}

/// Rate limit rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitRule {
    pub id: Uuid,
    pub name: String,
    pub key_pattern: String,
    pub requests_per_second: u32,
    pub burst_size: u32,
    pub action: RateLimitAction,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RateLimitAction {
    Reject,
    Delay,
    Throttle { max_delay_ms: u32 },
}

/// Token bucket state
#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_update: Instant,
    rate: f64,
    burst: f64,
}

impl TokenBucket {
    fn new(rate: f64, burst: f64) -> Self {
        Self {
            tokens: burst, // Start full
            last_update: Instant::now(),
            rate,
            burst,
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = (now - self.last_update).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate).min(self.burst);
        self.last_update = now;
    }

    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    fn available(&mut self) -> f64 {
        self.refill();
        self.tokens
    }

    fn time_until_available(&mut self, tokens: f64) -> Duration {
        self.refill();
        if self.tokens >= tokens {
            Duration::ZERO
        } else {
            let needed = tokens - self.tokens;
            Duration::from_secs_f64(needed / self.rate)
        }
    }
}

/// Rate limit result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u32,
    pub reset_after_ms: u64,
    pub retry_after_ms: Option<u64>,
}

/// Rate limiter service
pub struct RateLimiter {
    config: RateLimiterConfig,
    rules: Arc<RwLock<HashMap<Uuid, RateLimitRule>>>,
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimiterConfig) -> Self {
        info!("Initializing rate limiter with backend: {:?}", config.backend);
        
        Self {
            config,
            rules: Arc::new(RwLock::new(HashMap::new())),
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a rate limit rule
    pub async fn add_rule(&self, rule: RateLimitRule) -> Result<(), RateLimitError> {
        // Validate key pattern
        self.validate_key_pattern(&rule.key_pattern)?;

        let mut rules = self.rules.write().await;
        rules.insert(rule.id, rule.clone());
        
        info!("Added rate limit rule: {} ({})", rule.name, rule.id);
        Ok(())
    }

    /// Remove a rate limit rule
    pub async fn remove_rule(&self, rule_id: &Uuid) -> Result<(), RateLimitError> {
        let mut rules = self.rules.write().await;
        rules.remove(rule_id)
            .ok_or_else(|| RateLimitError::NotFound(*rule_id))?;
        
        info!("Removed rate limit rule: {}", rule_id);
        Ok(())
    }

    /// Get all rules
    pub async fn list_rules(&self) -> Vec<RateLimitRule> {
        let rules = self.rules.read().await;
        rules.values().cloned().collect()
    }

    /// Check if a request is allowed
    pub async fn check(&self, key: &str) -> RateLimitResult {
        self.check_with_cost(key, 1).await
    }

    /// Check if a request with a specific cost is allowed
    pub async fn check_with_cost(&self, key: &str, cost: u32) -> RateLimitResult {
        let full_key = format!("{}:{}", self.config.key_prefix, key);
        
        // Find matching rule
        let rule = self.find_matching_rule(key).await;
        let (rate, burst, action) = match rule {
            Some(r) => (r.requests_per_second as f64, r.burst_size as f64, r.action.clone()),
            None => (self.config.default_rps as f64, self.config.default_burst as f64, RateLimitAction::Reject),
        };

        let mut buckets = self.buckets.write().await;
        let bucket = buckets
            .entry(full_key.clone())
            .or_insert_with(|| TokenBucket::new(rate, burst));

        // Update bucket parameters if they changed
        bucket.rate = rate;
        bucket.burst = burst;

        let cost_f64 = cost as f64;
        
        if bucket.try_consume(cost_f64) {
            let remaining = bucket.available() as u32;
            RateLimitResult {
                allowed: true,
                remaining,
                reset_after_ms: (cost_f64 / rate * 1000.0) as u64,
                retry_after_ms: None,
            }
        } else {
            let time_until = bucket.time_until_available(cost_f64);
            
            match action {
                RateLimitAction::Reject => {
                    RateLimitResult {
                        allowed: false,
                        remaining: 0,
                        reset_after_ms: time_until.as_millis() as u64,
                        retry_after_ms: Some(time_until.as_millis() as u64),
                    }
                }
                RateLimitAction::Delay => {
                    // Wait and then allow
                    bucket.try_consume(cost_f64); // Force consume
                    RateLimitResult {
                        allowed: true,
                        remaining: 0,
                        reset_after_ms: (cost_f64 / rate * 1000.0) as u64,
                        retry_after_ms: Some(time_until.as_millis() as u64),
                    }
                }
                RateLimitAction::Throttle { max_delay_ms } => {
                    if time_until.as_millis() <= max_delay_ms as u128 {
                        bucket.try_consume(cost_f64);
                        RateLimitResult {
                            allowed: true,
                            remaining: 0,
                            reset_after_ms: (cost_f64 / rate * 1000.0) as u64,
                            retry_after_ms: Some(time_until.as_millis() as u64),
                        }
                    } else {
                        RateLimitResult {
                            allowed: false,
                            remaining: 0,
                            reset_after_ms: time_until.as_millis() as u64,
                            retry_after_ms: Some(time_until.as_millis() as u64),
                        }
                    }
                }
            }
        }
    }

    /// Reserve tokens for future use
    pub async fn reserve(&self, key: &str, tokens: u32) -> Option<Duration> {
        let full_key = format!("{}:{}", self.config.key_prefix, key);
        
        let rule = self.find_matching_rule(key).await;
        let (rate, burst) = match rule {
            Some(r) => (r.requests_per_second as f64, r.burst_size as f64),
            None => (self.config.default_rps as f64, self.config.default_burst as f64),
        };

        let mut buckets = self.buckets.write().await;
        let bucket = buckets
            .entry(full_key)
            .or_insert_with(|| TokenBucket::new(rate, burst));

        let time_until = bucket.time_until_available(tokens as f64);
        if time_until.is_zero() {
            None
        } else {
            Some(time_until)
        }
    }

    /// Reset rate limit for a key
    pub async fn reset(&self, key: &str) {
        let full_key = format!("{}:{}", self.config.key_prefix, key);
        
        let mut buckets = self.buckets.write().await;
        buckets.remove(&full_key);
        
        debug!("Reset rate limit for key: {}", key);
    }

    /// Get current rate limit state for a key
    pub async fn get_state(&self, key: &str) -> Option<(u32, u32)> {
        let full_key = format!("{}:{}", self.config.key_prefix, key);
        
        let mut buckets = self.buckets.write().await;
        buckets.get_mut(&full_key).map(|b| {
            (b.available() as u32, b.burst as u32)
        })
    }

    /// Find matching rule for a key
    async fn find_matching_rule(&self, key: &str) -> Option<RateLimitRule> {
        let rules = self.rules.read().await;
        
        for rule in rules.values() {
            if !rule.enabled {
                continue;
            }
            
            if self.key_matches_pattern(key, &rule.key_pattern) {
                return Some(rule.clone());
            }
        }
        
        None
    }

    /// Check if key matches pattern (supports wildcards)
    fn key_matches_pattern(&self, key: &str, pattern: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        
        // Simple glob matching
        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                return key.starts_with(prefix) && key.ends_with(suffix);
            }
        }
        
        key == pattern
    }

    /// Validate key pattern
    fn validate_key_pattern(&self, pattern: &str) -> Result<(), RateLimitError> {
        if pattern.is_empty() {
            return Err(RateLimitError::InvalidPattern("Pattern cannot be empty".to_string()));
        }
        
        // Check for valid characters
        for c in pattern.chars() {
            if !c.is_alphanumeric() && c != '_' && c != '-' && c != ':' && c != '*' && c != '.' {
                return Err(RateLimitError::InvalidPattern(
                    format!("Invalid character '{}' in pattern", c)
                ));
            }
        }
        
        Ok(())
    }

    /// Clean up stale buckets
    pub async fn cleanup(&self) -> usize {
        let mut buckets = self.buckets.write().await;
        let before = buckets.len();
        
        // Remove buckets that haven't been used in 5 minutes
        buckets.retain(|_, b| {
            b.last_update.elapsed() < Duration::from_secs(300)
        });
        
        let removed = before - buckets.len();
        if removed > 0 {
            debug!("Cleaned up {} stale rate limit buckets", removed);
        }
        removed
    }

    /// Get statistics
    pub async fn get_stats(&self) -> RateLimitStats {
        let rules = self.rules.read().await;
        let buckets = self.buckets.read().await;
        
        RateLimitStats {
            total_rules: rules.len(),
            active_buckets: buckets.len(),
            enabled_rules: rules.values().filter(|r| r.enabled).count(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStats {
    pub total_rules: usize,
    pub active_buckets: usize,
    pub enabled_rules: usize,
}

#[derive(thiserror::Error, Debug)]
pub enum RateLimitError {
    #[error("Rule not found: {0}")]
    NotFound(Uuid),
    
    #[error("Invalid key pattern: {0}")]
    InvalidPattern(String),
    
    #[error("Backend error: {0}")]
    BackendError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limit_check() {
        let limiter = RateLimiter::new(RateLimiterConfig::default());
        
        let result = limiter.check("test-key").await;
        assert!(result.allowed);
        assert!(result.remaining > 0);
    }

    #[tokio::test]
    async fn test_rate_limit_exhaust() {
        let config = RateLimiterConfig {
            default_rps: 1,
            default_burst: 2,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);
        
        // Exhaust the bucket
        limiter.check("test-key").await;
        limiter.check("test-key").await;
        
        // Should be rate limited
        let result = limiter.check("test-key").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_add_rule() {
        let limiter = RateLimiter::new(RateLimiterConfig::default());
        
        let rule = RateLimitRule {
            id: Uuid::new_v4(),
            name: "api-limit".to_string(),
            key_pattern: "api:*".to_string(),
            requests_per_second: 10,
            burst_size: 20,
            action: RateLimitAction::Reject,
            enabled: true,
        };
        
        limiter.add_rule(rule).await.unwrap();
        
        let rules = limiter.list_rules().await;
        assert_eq!(rules.len(), 1);
    }

    #[tokio::test]
    async fn test_key_pattern_matching() {
        let limiter = RateLimiter::new(RateLimiterConfig::default());
        
        let rule = RateLimitRule {
            id: Uuid::new_v4(),
            name: "api-limit".to_string(),
            key_pattern: "api:users:*".to_string(),
            requests_per_second: 5,
            burst_size: 10,
            action: RateLimitAction::Reject,
            enabled: true,
        };
        
        limiter.add_rule(rule).await.unwrap();
        
        // Matching key
        let matching = limiter.find_matching_rule("api:users:123").await;
        assert!(matching.is_some());
        
        // Non-matching key
        let non_matching = limiter.find_matching_rule("api:posts:123").await;
        assert!(non_matching.is_none());
    }
}
