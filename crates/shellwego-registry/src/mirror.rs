//! Registry mirror chain with health checking and circuit breaking.
//!
//! Implements a priority-based mirror selector that tries mirrors in order
//! before falling back to the upstream registry. Supports:
//! - Per-mirror HTTP client pools
//! - Health checking via HEAD /v2/
//! - Circuit breaker pattern (consecutive failures → unhealthy)
//! - Automatic failover to next healthy mirror

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use shellwego_schema::oci::{MirrorConfig, MirrorList, MirrorHealth};

/// Mirror chain with health checking and circuit breaking.
///
/// Maintains per-mirror HTTP clients, health status, and failure counts.
/// Provides `next_mirror()` for iterating through healthy mirrors.
pub struct MirrorChain {
    /// Mirror configuration list
    config: MirrorList,
    /// HTTP client (one per mirror for connection pooling)
    clients: HashMap<String, reqwest::Client>,
    /// Current health status of each mirror
    health: Arc<RwLock<HashMap<String, MirrorHealth>>>,
    /// Consecutive failure count per mirror
    failure_counts: Arc<RwLock<HashMap<String, u32>>>,
    /// Last health check time per mirror
    #[allow(dead_code)]
    last_health_check: Arc<RwLock<HashMap<String, Instant>>>,
}

impl MirrorChain {
    /// Create a new mirror chain from a configuration list.
    pub fn new(config: MirrorList) -> Self {
        let mut clients = HashMap::new();
        for mirror in &config.mirrors {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(mirror.timeout_secs))
                .user_agent("shellwego-registry/0.1.0")
                .build()
                .expect("Failed to create HTTP client for mirror");
            clients.insert(mirror.id.clone(), client);
        }

        let health: HashMap<String, MirrorHealth> = config
            .mirrors
            .iter()
            .map(|m| (m.id.clone(), MirrorHealth::Unknown))
            .collect();

        Self {
            config,
            clients,
            health: Arc::new(RwLock::new(health)),
            failure_counts: Arc::new(RwLock::new(HashMap::new())),
            last_health_check: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the next healthy mirror for a registry, skipping already-tried mirrors.
    ///
    /// Returns `Some((endpoint, client))` for the next healthy mirror,
    /// or `None` if all mirrors are exhausted.
    pub async fn next_mirror(
        &self,
        registry: &str,
        skip_ids: &[String],
    ) -> Option<(String, reqwest::Client)> {
        let health = self.health.read().await;
        let mirrors = self.config.for_registry(registry);

        for mirror in mirrors {
            if skip_ids.contains(&mirror.id) {
                continue;
            }
            let status = health.get(&mirror.id).copied().unwrap_or(MirrorHealth::Unknown);
            if status != MirrorHealth::Unhealthy {
                let client = self.clients.get(&mirror.id).cloned()?;
                return Some((mirror.endpoint.clone(), client));
            }
        }
        None
    }

    /// Record a successful request to a mirror.
    ///
    /// Resets the failure counter and marks the mirror as healthy.
    pub async fn record_success(&self, mirror_id: &str) {
        let mut failures = self.failure_counts.write().await;
        failures.insert(mirror_id.to_string(), 0);
        let mut health = self.health.write().await;
        health.insert(mirror_id.to_string(), MirrorHealth::Healthy);
    }

    /// Record a failed request to a mirror.
    ///
    /// Increments the failure counter. If it reaches the mirror's circuit
    /// breaker threshold, the mirror is marked as unhealthy.
    pub async fn record_failure(&self, mirror_id: &str, threshold: u32) {
        let mut failures = self.failure_counts.write().await;
        let count = failures.entry(mirror_id.to_string()).or_insert(0);
        *count += 1;
        if *count >= threshold {
            let mut health = self.health.write().await;
            health.insert(mirror_id.to_string(), MirrorHealth::Unhealthy);
            warn!(
                "Circuit breaker OPEN for mirror {} after {} failures",
                mirror_id, count
            );
        }
    }

    /// Run health checks on all mirrors via HEAD /v2/.
    pub async fn health_check_all(&self) {
        for mirror in &self.config.mirrors {
            self.health_check(&mirror).await;
        }
    }

    /// Health check a single mirror (HEAD /v2/).
    async fn health_check(&self, mirror: &MirrorConfig) {
        let client = match self.clients.get(&mirror.id) {
            Some(c) => c,
            None => return,
        };

        let url = format!("{}/v2/", mirror.endpoint);
        let result = client.head(&url).send().await;

        let mut health = self.health.write().await;
        match result {
            Ok(resp) if resp.status().is_success() => {
                health.insert(mirror.id.clone(), MirrorHealth::Healthy);
                debug!("Mirror {} is healthy", mirror.id);
            }
            Ok(resp) => {
                health.insert(mirror.id.clone(), MirrorHealth::Degraded);
                warn!(
                    "Mirror {} returned status {}",
                    mirror.id,
                    resp.status()
                );
            }
            Err(e) => {
                health.insert(mirror.id.clone(), MirrorHealth::Unhealthy);
                warn!("Mirror {} health check failed: {}", mirror.id, e);
            }
        }

        let mut last_check = self.last_health_check.write().await;
        last_check.insert(mirror.id.clone(), Instant::now());
    }

    /// Check if the mirror chain has any mirrors configured.
    pub fn is_empty(&self) -> bool {
        self.config.is_empty()
    }

    /// Get a reference to the mirror configuration.
    pub fn config(&self) -> &MirrorList {
        &self.config
    }

    /// Get the circuit breaker threshold for a specific mirror.
    pub fn get_threshold(&self, mirror_id: &str) -> u32 {
        self.config
            .mirrors
            .iter()
            .find(|m| m.id == mirror_id)
            .map(|m| m.circuit_breaker_threshold)
            .unwrap_or(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shellwego_schema::oci::MirrorPriority;

    fn make_test_mirror(id: &str, priority: MirrorPriority) -> MirrorConfig {
        MirrorConfig {
            id: id.to_string(),
            endpoint: format!("https://{}.example.com", id),
            priority,
            enabled: true,
            registry_override: None,
            auth: None,
            health_check_interval_secs: 30,
            circuit_breaker_threshold: 3,
            timeout_secs: 60,
        }
    }

    #[test]
    fn test_mirror_chain_creation() {
        let list = MirrorList::new()
            .add_mirror(make_test_mirror("m1", MirrorPriority::High))
            .add_mirror(make_test_mirror("m2", MirrorPriority::Low));

        let chain = MirrorChain::new(list);
        assert!(!chain.is_empty());
        assert_eq!(chain.config().len(), 2);
    }

    #[tokio::test]
    async fn test_next_mirror_skips_unhealthy() {
        let list = MirrorList::new()
            .add_mirror(make_test_mirror("m1", MirrorPriority::High))
            .add_mirror(make_test_mirror("m2", MirrorPriority::Low));

        let chain = MirrorChain::new(list);

        // Mark m1 as unhealthy
        {
            let mut health = chain.health.write().await;
            health.insert("m1".to_string(), MirrorHealth::Unhealthy);
        }

        // Should skip m1 and return m2
        let result = chain.next_mirror("docker.io", &[]).await;
        assert!(result.is_some());
        let (endpoint, _) = result.unwrap();
        assert!(endpoint.contains("m2"));
    }

    #[tokio::test]
    async fn test_next_mirror_respects_skip() {
        let list = MirrorList::new()
            .add_mirror(make_test_mirror("m1", MirrorPriority::High))
            .add_mirror(make_test_mirror("m2", MirrorPriority::Low));

        let chain = MirrorChain::new(list);

        // Skip m1
        let result = chain.next_mirror("docker.io", &["m1".to_string()]).await;
        assert!(result.is_some());
        let (endpoint, _) = result.unwrap();
        assert!(endpoint.contains("m2"));
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens() {
        let list = MirrorList::new()
            .add_mirror(make_test_mirror("m1", MirrorPriority::High));

        let chain = MirrorChain::new(list);

        // Record 3 failures (threshold)
        chain.record_failure("m1", 3).await;
        chain.record_failure("m1", 3).await;
        chain.record_failure("m1", 3).await;

        // Mirror should now be unhealthy
        let result = chain.next_mirror("docker.io", &[]).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_success_resets_circuit_breaker() {
        let list = MirrorList::new()
            .add_mirror(make_test_mirror("m1", MirrorPriority::High));

        let chain = MirrorChain::new(list);

        // Mark unhealthy
        {
            let mut health = chain.health.write().await;
            health.insert("m1".to_string(), MirrorHealth::Unhealthy);
        }

        // Record success should reset
        chain.record_success("m1").await;

        let health = chain.health.read().await;
        assert_eq!(health.get("m1"), Some(&MirrorHealth::Healthy));
    }

    #[tokio::test]
    async fn test_empty_chain_returns_none() {
        let chain = MirrorChain::new(MirrorList::new());
        assert!(chain.is_empty());
        let result = chain.next_mirror("docker.io", &[]).await;
        assert!(result.is_none());
    }
}
