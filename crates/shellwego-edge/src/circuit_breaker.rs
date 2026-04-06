//! Circuit breaker implementation for upstream fault tolerance
//!
//! Implements a per-upstream circuit breaker state machine with three states:
//! - **Closed**: Normal operation, requests pass through
//! - **Open**: Failure threshold exceeded, requests are immediately rejected
//! - **HalfOpen**: Probing state, limited requests allowed to test recovery
//!
//! Transitions:
//! - Closed → Open: after `failure_threshold` consecutive failures
//! - Open → HalfOpen: after `timeout_secs` since the last failure
//! - HalfOpen → Closed: after `success_threshold` consecutive successes
//! - HalfOpen → Open: on any failure (back to Open)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use tracing::{info, warn};

use crate::router::CircuitBreakerConfig;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — requests pass through
    Closed,
    /// Failures exceeded threshold — requests are rejected
    Open,
    /// Probing — allow limited requests to test recovery
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "CLOSED"),
            CircuitState::Open => write!(f, "OPEN"),
            CircuitState::HalfOpen => write!(f, "HALF-OPEN"),
        }
    }
}

/// Per-upstream circuit breaker
struct Circuit {
    /// Current state
    state: CircuitState,
    /// Consecutive failure count (reset on success)
    failure_count: u32,
    /// Consecutive success count in HalfOpen (reset on failure)
    success_count: u32,
    /// Time of the last failure (used for Open → HalfOpen transition)
    last_failure_time: Option<Instant>,
    /// Configuration thresholds
    config: CircuitBreakerConfig,
}

impl Circuit {
    fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            config,
        }
    }

    /// Record a successful request.
    fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                // Reset failure count on any success in Closed state
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                // In HalfOpen, count consecutive successes
                self.success_count += 1;
                if self.success_count >= self.config.success_threshold {
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                    info!(
                        "Circuit breaker CLOSED (recovered after {} successes)",
                        self.config.success_threshold
                    );
                }
            }
            CircuitState::Open => {
                // Should not happen — check_timeout should transition to HalfOpen first.
                // But handle gracefully.
                self.check_timeout();
            }
        }
    }

    /// Record a failed request.
    fn record_failure(&mut self) {
        self.last_failure_time = Some(Instant::now());

        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.config.failure_threshold {
                    self.state = CircuitState::Open;
                    warn!(
                        "Circuit breaker OPEN ({} consecutive failures, threshold={})",
                        self.failure_count,
                        self.config.failure_threshold
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in HalfOpen immediately reopens the circuit
                self.state = CircuitState::Open;
                warn!("Circuit breaker OPEN (probe request failed)");
            }
            CircuitState::Open => {
                // Already open, just update the failure time
            }
        }
    }

    /// Check if the Open → HalfOpen timeout has elapsed.
    fn check_timeout(&mut self) {
        if self.state == CircuitState::Open {
            if let Some(last_fail) = self.last_failure_time {
                if last_fail.elapsed() >= Duration::from_secs(self.config.timeout_secs) {
                    self.state = CircuitState::HalfOpen;
                    self.success_count = 0;
                    info!(
                        "Circuit breaker HALF-OPEN (timeout of {}s elapsed)",
                        self.config.timeout_secs
                    );
                }
            }
        }
    }

    /// Check if a request is allowed through the circuit breaker.
    fn is_request_allowed(&mut self) -> bool {
        self.check_timeout();
        match self.state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => true, // Allow probe request
            CircuitState::Open => false,
        }
    }

    /// Get the current state (after checking timeout).
    fn state(&mut self) -> CircuitState {
        self.check_timeout();
        self.state
    }
}

/// Circuit breaker registry — one breaker per upstream URL.
///
/// Thread-safe via `parking_lot::RwLock`. Each upstream URL gets its own
/// `Circuit` instance with independent state.
pub struct CircuitBreakerRegistry {
    breakers: RwLock<HashMap<String, Circuit>>,
}

impl CircuitBreakerRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            breakers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a circuit breaker for the given upstream URL.
    ///
    /// Called when routes are loaded. If a breaker already exists for this URL,
    /// it is NOT replaced (preserving its runtime state). To update the config,
    /// call `update_config()` first.
    pub fn register(&self, upstream_url: &str, config: CircuitBreakerConfig) {
        let mut breakers = self.breakers.write();
        breakers
            .entry(upstream_url.to_string())
            .or_insert_with(|| Circuit::new(config));
    }

    /// Update the configuration for an existing circuit breaker.
    pub fn update_config(&self, upstream_url: &str, config: CircuitBreakerConfig) {
        let mut breakers = self.breakers.write();
        if let Some(circuit) = breakers.get_mut(upstream_url) {
            circuit.config = config;
        }
    }

    /// Remove the circuit breaker for the given upstream URL.
    pub fn unregister(&self, upstream_url: &str) {
        let mut breakers = self.breakers.write();
        breakers.remove(upstream_url);
    }

    /// Check if a request is allowed through the circuit breaker.
    ///
    /// Returns `true` if no breaker is configured for this URL (fail-open).
    pub fn is_request_allowed(&self, upstream_url: &str) -> bool {
        let mut breakers = self.breakers.write();
        if let Some(circuit) = breakers.get_mut(upstream_url) {
            circuit.is_request_allowed()
        } else {
            true // No breaker configured — allow all requests
        }
    }

    /// Record a successful request for the given upstream.
    pub fn record_success(&self, upstream_url: &str) {
        let mut breakers = self.breakers.write();
        if let Some(circuit) = breakers.get_mut(upstream_url) {
            circuit.record_success();
        }
    }

    /// Record a failed request for the given upstream.
    pub fn record_failure(&self, upstream_url: &str) {
        let mut breakers = self.breakers.write();
        if let Some(circuit) = breakers.get_mut(upstream_url) {
            circuit.record_failure();
        }
    }

    /// Get the current state for an upstream.
    ///
    /// Returns `None` if no breaker is configured for this URL.
    pub fn state(&self, upstream_url: &str) -> Option<CircuitState> {
        let mut breakers = self.breakers.write();
        breakers.get_mut(upstream_url).map(|c| c.state())
    }

    /// Get the number of registered circuit breakers.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.breakers.read().len()
    }

    /// Check if the registry is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.breakers.read().is_empty()
    }
}

impl Default for CircuitBreakerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_secs: 5,
        }
    }

    #[test]
    fn test_circuit_starts_closed() {
        let mut circuit = Circuit::new(default_config());
        assert_eq!(circuit.state(), CircuitState::Closed);
        assert!(circuit.is_request_allowed());
    }

    #[test]
    fn test_circuit_opens_after_failures() {
        let mut circuit = Circuit::new(default_config());
        assert!(circuit.is_request_allowed());

        circuit.record_failure();
        assert_eq!(circuit.state(), CircuitState::Closed); // 1/3 failures

        circuit.record_failure();
        assert_eq!(circuit.state(), CircuitState::Closed); // 2/3 failures

        circuit.record_failure();
        assert_eq!(circuit.state(), CircuitState::Open); // 3/3 → OPEN
        assert!(!circuit.is_request_allowed());
    }

    #[test]
    fn test_circuit_success_resets_failure_count() {
        let mut circuit = Circuit::new(default_config());

        circuit.record_failure();
        circuit.record_failure();
        circuit.record_success(); // Reset failure count
        circuit.record_failure();

        assert_eq!(circuit.state(), CircuitState::Closed); // Only 1 consecutive failure
    }

    #[test]
    fn test_circuit_half_open_on_failure() {
        let mut circuit = Circuit::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            timeout_secs: 0,
        });

        circuit.record_failure();
        assert_eq!(circuit.state(), CircuitState::Open);

        // Simulate timeout elapsed by manipulating last_failure_time
        circuit.last_failure_time = Some(Instant::now() - Duration::from_secs(1));
        assert_eq!(circuit.state(), CircuitState::HalfOpen);

        // Failure in HalfOpen → back to Open
        circuit.record_failure();
        assert_eq!(circuit.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_half_open_to_closed_on_success() {
        let mut circuit = Circuit::new(CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout_secs: 0,
        });

        circuit.record_failure();
        circuit.last_failure_time = Some(Instant::now() - Duration::from_secs(1));
        assert_eq!(circuit.state(), CircuitState::HalfOpen);

        circuit.record_success(); // 1/2 successes
        assert_eq!(circuit.state(), CircuitState::HalfOpen);

        circuit.record_success(); // 2/2 successes → Closed
        assert_eq!(circuit.state(), CircuitState::Closed);
    }

    #[test]
    fn test_registry_is_request_allowed_no_breaker() {
        let registry = CircuitBreakerRegistry::new();
        assert!(registry.is_request_allowed("http://unknown:8080"));
    }

    #[test]
    fn test_registry_register_and_check() {
        let registry = CircuitBreakerRegistry::new();
        registry.register(
            "http://backend:8080",
            CircuitBreakerConfig {
                failure_threshold: 2,
                success_threshold: 1,
                timeout_secs: 10,
            },
        );

        assert!(registry.is_request_allowed("http://backend:8080"));
        assert!(!registry.is_request_allowed("http://other:8080")); // No breaker → allowed

        registry.record_failure("http://backend:8080");
        assert!(registry.is_request_allowed("http://backend:8080")); // 1/2 failures

        registry.record_failure("http://backend:8080");
        assert!(!registry.is_request_allowed("http://backend:8080")); // 2/2 → OPEN
    }

    #[test]
    fn test_registry_state() {
        let registry = CircuitBreakerRegistry::new();
        assert!(registry.state("http://unknown:8080").is_none());

        registry.register(
            "http://backend:8080",
            default_config(),
        );
        assert_eq!(
            registry.state("http://backend:8080"),
            Some(CircuitState::Closed)
        );
    }

    #[test]
    fn test_registry_unregister() {
        let registry = CircuitBreakerRegistry::new();
        registry.register("http://backend:8080", default_config());
        assert_eq!(registry.len(), 1);

        registry.unregister("http://backend:8080");
        assert_eq!(registry.len(), 0);
        // No breaker → allowed (fail-open)
        assert!(registry.is_request_allowed("http://backend:8080"));
    }
}
