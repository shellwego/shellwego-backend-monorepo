//! Health checker for upstream backends
//!
//! Periodically polls upstream endpoints to determine their health status.
//! Uses a shared health state map to communicate health to the proxy's
//! upstream selection logic without requiring mutable access to the route table.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::router::{HealthCheckConfig, Route, Router};

/// Background health checker that polls upstreams.
///
/// Maintains its own `HashMap<String, AtomicBool>` keyed by upstream URL so that
/// the proxy's `select_upstream()` can check health without mutating the `Route`
/// struct. This avoids the serde complexity of `Arc<AtomicBool>` on the `Upstream`
/// struct itself.
pub struct HealthChecker {
    /// Shared reference to the router (read-only, for route/upstream iteration)
    router: Arc<RwLock<Router>>,
    /// Health state per upstream URL: true = healthy, false = unhealthy.
    /// Populated from route configs on construction and on reload.
    health_map: Arc<parking_lot::RwLock<HashMap<String, AtomicBool>>>,
    /// Flag to signal the background task to stop
    running: Arc<AtomicBool>,
}

impl HealthChecker {
    /// Create a new health checker.
    ///
    /// The `health_map` is pre-populated from the current routes.
    pub fn new(
        router: Arc<RwLock<Router>>,
        health_map: Arc<parking_lot::RwLock<HashMap<String, AtomicBool>>>,
    ) -> Self {
        Self {
            router,
            health_map,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the health check loop. Spawns a background tokio task.
    pub fn start(self: Arc<Self>) {
        self.running.store(true, Ordering::Relaxed);
        tokio::spawn(async move {
            self.run().await;
        });
    }

    /// Stop the health check loop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Check if the background task is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    async fn run(&self) {
        // Initial check interval: 10 seconds
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            if !self.running.load(Ordering::Relaxed) {
                break;
            }

            self.check_all_upstreams().await;
        }

        info!("Health checker stopped");
    }

    async fn check_all_upstreams(&self) {
        let router_guard = self.router.read().await;

        // Collect all upstreams that have health check configs
        let mut checks: Vec<(String, String, HealthCheckConfig)> = Vec::new();

        for route_id in router_guard.list_routes() {
            if let Some(route) = router_guard.get_route(route_id) {
                for upstream in &route.upstreams {
                    if let Some(ref config) = upstream.health_check {
                        checks.push((
                            upstream.url.clone(),
                            route.id.clone(),
                            config.clone(),
                        ));
                    }
                }
            }
        }

        // Drop the router read lock before making HTTP requests
        drop(router_guard);

        for (url, route_id, config) in checks {
            let healthy = self.check_upstream(&url, &config).await;

            // Update the shared health map
            {
                let mut map = self.health_map.write();
                if let Some(state) = map.get(&url) {
                    let was_healthy = state.load(Ordering::Relaxed);
                    state.store(healthy, Ordering::Relaxed);
                    if was_healthy && !healthy {
                        warn!(
                            "Health check FAILED for upstream {} (route {})",
                            url, route_id
                        );
                    } else if !was_healthy && healthy {
                        info!(
                            "Health check PASSED for upstream {} (route {})",
                            url, route_id
                        );
                    }
                }
            }

            debug!(
                "Health check {} → upstream {} (route {}): {}",
                route_id,
                url,
                if healthy { "healthy" } else { "unhealthy" }
            );
        }
    }

    async fn check_upstream(&self, url: &str, config: &HealthCheckConfig) -> bool {
        let check_url = format!("{}{}", url.trim_end_matches('/'), config.path);

        let result = tokio::time::timeout(
            Duration::from_secs(config.timeout_secs),
            reqwest::get(&check_url),
        )
        .await;

        match result {
            Ok(Ok(resp)) => resp.status().is_success(),
            Ok(Err(e)) => {
                warn!("Health check failed for {}: {}", url, e);
                false
            }
            Err(_) => {
                warn!("Health check timeout for {}", url);
                false
            }
        }
    }

    /// Get a reference to the health map (shared with the proxy for upstream selection).
    pub fn health_map(&self) -> Arc<parking_lot::RwLock<HashMap<String, AtomicBool>>> {
        Arc::clone(&self.health_map)
    }
}

/// Build the initial health map from the router's routes.
///
/// Called during `EdgeProxy::new()` to pre-populate the health state
/// for all upstreams that have health check configs.
pub fn build_health_map(router: &Router) -> HashMap<String, AtomicBool> {
    let mut map = HashMap::new();

    for route_id in router.list_routes() {
        if let Some(route) = router.get_route(route_id) {
            for upstream in &route.upstreams {
                if upstream.health_check.is_some() {
                    map.insert(
                        upstream.url.clone(),
                        AtomicBool::new(upstream.healthy),
                    );
                }
            }
        }
    }

    map
}

/// Re-sync the health map when routes are reloaded.
///
/// Adds new upstreams, preserves existing health state for unchanged upstreams.
pub fn sync_health_map(
    health_map: &parking_lot::RwLock<HashMap<String, AtomicBool>>,
    router: &Router,
) {
    let mut map = health_map.write();

    // Collect current upstream URLs from routes
    let mut current_urls: std::collections::HashSet<String> = std::collections::HashSet::new();

    for route_id in router.list_routes() {
        if let Some(route) = router.get_route(route_id) {
            for upstream in &route.upstreams {
                if upstream.health_check.is_some() {
                    current_urls.insert(upstream.url.clone());
                    map.entry(upstream.url.clone())
                        .or_insert_with(|| AtomicBool::new(upstream.healthy));
                }
            }
        }
    }

    // Remove entries for upstreams that no longer exist
    map.retain(|url, _| current_urls.contains(url));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_health_map_empty() {
        let router = Router::new();
        let map = build_health_map(&router);
        assert!(map.is_empty());
    }

    #[test]
    fn test_build_health_map_with_routes() {
        let mut router = Router::new();

        let route = crate::router::Route {
            id: "test-route".to_string(),
            priority: 100,
            matchers: vec![crate::router::Matcher::Host("example.com".to_string())],
            upstreams: vec![
                crate::router::Upstream {
                    url: "http://localhost:8080".to_string(),
                    healthy: true,
                    health_check: Some(HealthCheckConfig {
                        path: "/health".to_string(),
                        interval_secs: 10,
                        timeout_secs: 5,
                        unhealthy_threshold: 3,
                        healthy_threshold: 2,
                    }),
                    ..Default::default()
                },
                crate::router::Upstream {
                    url: "http://localhost:8081".to_string(),
                    healthy: false,
                    health_check: Some(HealthCheckConfig {
                        path: "/healthz".to_string(),
                        interval_secs: 10,
                        timeout_secs: 5,
                        unhealthy_threshold: 3,
                        healthy_threshold: 2,
                    }),
                    ..Default::default()
                },
                // Upstream without health check should not be in the map
                crate::router::Upstream {
                    url: "http://localhost:8082".to_string(),
                    healthy: true,
                    health_check: None,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        router.add_route(route).unwrap();

        let map = build_health_map(&router);
        assert_eq!(map.len(), 2);
        assert!(map.get("http://localhost:8080").unwrap().load(Ordering::Relaxed));
        assert!(!map.get("http://localhost:8081").unwrap().load(Ordering::Relaxed));
    }
}
