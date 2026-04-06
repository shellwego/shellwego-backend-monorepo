//! Registry mirror configuration types
//!
//! Provides types for configuring priority-based mirror chains with
//! health checking, circuit breaking, and automatic failover for
//! container image registries.

use serde::{Deserialize, Serialize};

/// Priority level for a registry mirror.
///
/// Lower values are tried first during image pulls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MirrorPriority {
    /// Try first (e.g., local Dragonfly P2P cache)
    Critical = 0,
    /// High priority (e.g., regional mirror)
    High = 1,
    /// Normal priority (e.g., cloud registry mirror)
    Normal = 2,
    /// Low priority / fallback (e.g., upstream directly)
    Low = 3,
}

impl Default for MirrorPriority {
    fn default() -> Self {
        MirrorPriority::Normal
    }
}

/// Health status of a mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirrorHealth {
    /// Mirror is healthy and responding within normal latency
    Healthy,
    /// Mirror is degraded (slow responses or intermittent errors)
    Degraded,
    /// Mirror is down (circuit breaker is open)
    Unhealthy,
    /// Mirror has not been probed yet
    Unknown,
}

impl Default for MirrorHealth {
    fn default() -> Self {
        MirrorHealth::Unknown
    }
}

/// Configuration for a single registry mirror.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorConfig {
    /// Unique identifier for this mirror (e.g., "local-dragonfly", "eu-west-mirror")
    pub id: String,
    /// Mirror endpoint URL (e.g., "https://mirror.example.com")
    pub endpoint: String,
    /// Priority level (lower = tried first)
    #[serde(default)]
    pub priority: MirrorPriority,
    /// Whether this mirror is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Override registry host (if mirror serves multiple registries).
    /// When set, this mirror only handles requests for the specified registry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_override: Option<String>,
    /// Authentication for this mirror (if different from upstream)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<crate::oci::RegistryAuth>,
    /// Health check interval in seconds
    #[serde(default = "default_health_interval")]
    pub health_check_interval_secs: u64,
    /// Circuit breaker threshold: consecutive failures before marking unhealthy
    #[serde(default = "default_circuit_breaker")]
    pub circuit_breaker_threshold: u32,
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_true() -> bool {
    true
}
fn default_health_interval() -> u64 {
    30
}
fn default_circuit_breaker() -> u32 {
    3
}
fn default_timeout() -> u64 {
    60
}

/// Ordered list of mirrors with health tracking.
///
/// Mirrors are stored in priority order (lowest priority value first).
/// The `for_registry()` method returns the subset applicable to a given
/// upstream registry, sorted by priority.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MirrorList {
    /// List of mirror configurations (ordered by priority)
    pub mirrors: Vec<MirrorConfig>,
}

impl MirrorList {
    /// Create an empty mirror list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a mirror configuration, maintaining priority-sorted order.
    pub fn add_mirror(mut self, mirror: MirrorConfig) -> Self {
        self.mirrors.push(mirror);
        self.mirrors.sort_by_key(|m| m.priority);
        self
    }

    /// Get mirrors applicable to a specific registry, sorted by priority.
    ///
    /// Filters by:
    /// - Enabled status
    /// - Registry override match (or no override = applies to all)
    pub fn for_registry(&self, registry: &str) -> Vec<&MirrorConfig> {
        self.mirrors
            .iter()
            .filter(|m| m.enabled)
            .filter(|m| {
                m.registry_override
                    .as_ref()
                    .map_or(true, |r| r == registry)
            })
            .collect()
    }

    /// Check if the mirror list is empty.
    pub fn is_empty(&self) -> bool {
        self.mirrors.is_empty()
    }

    /// Number of configured mirrors.
    pub fn len(&self) -> usize {
        self.mirrors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mirror(id: &str, endpoint: &str, priority: MirrorPriority) -> MirrorConfig {
        MirrorConfig {
            id: id.to_string(),
            endpoint: endpoint.to_string(),
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
    fn test_mirror_priority_ordering() {
        assert!(MirrorPriority::Critical < MirrorPriority::High);
        assert!(MirrorPriority::High < MirrorPriority::Normal);
        assert!(MirrorPriority::Normal < MirrorPriority::Low);
    }

    #[test]
    fn test_mirror_list_add_and_sort() {
        let list = MirrorList::new()
            .add_mirror(make_mirror("low", "https://low.example.com", MirrorPriority::Low))
            .add_mirror(make_mirror("critical", "https://critical.example.com", MirrorPriority::Critical))
            .add_mirror(make_mirror("normal", "https://normal.example.com", MirrorPriority::Normal));

        assert_eq!(list.len(), 3);
        assert_eq!(list.mirrors[0].id, "critical");
        assert_eq!(list.mirrors[1].id, "normal");
        assert_eq!(list.mirrors[2].id, "low");
    }

    #[test]
    fn test_mirror_list_for_registry() {
        let list = MirrorList::new()
            .add_mirror(make_mirror("generic", "https://mirror.example.com", MirrorPriority::Normal))
            .add_mirror({
                let mut m = make_mirror("docker-only", "https://docker.example.com", MirrorPriority::High);
                m.registry_override = Some("registry-1.docker.io".to_string());
                m
            });

        // For docker registry, both should match
        let docker_mirrors = list.for_registry("registry-1.docker.io");
        assert_eq!(docker_mirrors.len(), 2);

        // For gcr, only generic should match
        let gcr_mirrors = list.for_registry("gcr.io");
        assert_eq!(gcr_mirrors.len(), 1);
        assert_eq!(gcr_mirrors[0].id, "generic");
    }

    #[test]
    fn test_mirror_list_disabled_filter() {
        let list = MirrorList::new()
            .add_mirror(make_mirror("enabled", "https://enabled.example.com", MirrorPriority::High))
            .add_mirror({
                let mut m = make_mirror("disabled", "https://disabled.example.com", MirrorPriority::Critical);
                m.enabled = false;
                m
            });

        let mirrors = list.for_registry("any");
        assert_eq!(mirrors.len(), 1);
        assert_eq!(mirrors[0].id, "enabled");
    }

    #[test]
    fn test_mirror_health_default() {
        assert_eq!(MirrorHealth::default(), MirrorHealth::Unknown);
    }
}
