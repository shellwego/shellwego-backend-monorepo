//! Dynamic HTTP router with rule-based matching
//!
//! Supports host, path, header, and query-based routing with
//! priority ordering and middleware chains.

use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::EdgeError;

/// Route table with fast lookup
pub struct Router {
    /// All routes indexed by ID
    routes: HashMap<String, Route>,
    /// Host-based index for fast matching
    host_index: HashMap<String, Vec<String>>, // host -> route IDs
    /// Wildcard host patterns
    wildcard_hosts: Vec<(String, String)>, // (pattern, route_id)
    /// Route priorities (sorted)
    priorities: Vec<(i32, String)>, // (priority, route_id)
    /// Round-robin counter for load balancing
    rr_counter: AtomicUsize,
}

impl Router {
    /// Create empty router
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            host_index: HashMap::new(),
            wildcard_hosts: Vec::new(),
            priorities: Vec::new(),
            rr_counter: AtomicUsize::new(0),
        }
    }

    /// Add route to table
    pub fn add_route(&mut self, route: Route) -> Result<(), EdgeError> {
        let route_id = route.id.clone();

        // Validate route
        self.validate_route(&route)?;

        // Index by host matchers
        for matcher in &route.matchers {
            if let Matcher::Host(host) = matcher {
                if host.starts_with("*.") {
                    // Wildcard pattern
                    self.wildcard_hosts.push((host.clone(), route_id.clone()));
                } else {
                    // Exact host match
                    self.host_index
                        .entry(host.clone())
                        .or_default()
                        .push(route_id.clone());
                }
            }
        }

        // Add to priorities
        let priority = route.priority;
        self.priorities.push((priority, route_id.clone()));
        self.priorities.sort_by(|a, b| b.0.cmp(&a.0)); // Descending order

        // Store route
        self.routes.insert(route_id, route);

        debug!("Added route to router");
        Ok(())
    }

    /// Remove route by ID
    pub fn remove_route(&mut self, route_id: &str) -> Result<(), EdgeError> {
        if let Some(route) = self.routes.remove(route_id) {
            // Remove from host index
            for matcher in &route.matchers {
                if let Matcher::Host(host) = matcher {
                    if let Some(ids) = self.host_index.get_mut(host) {
                        ids.retain(|id| id != route_id);
                    }
                    self.wildcard_hosts.retain(|(h, _)| h != host);
                }
            }

            // Remove from priorities
            self.priorities.retain(|(_, id)| id != route_id);

            debug!("Removed route {} from router", route_id);
            Ok(())
        } else {
            Err(EdgeError::RoutingError(format!(
                "Route not found: {}",
                route_id
            )))
        }
    }

    /// Match request to route
    pub fn match_request(&self, req: &RequestInfo) -> Option<&Route> {
        // Try exact host match first
        if let Some(route_ids) = self.host_index.get(&req.host) {
            for route_id in route_ids {
                if let Some(route) = self.routes.get(route_id) {
                    if self.matches_route(route, req) {
                        return Some(route);
                    }
                }
            }
        }

        // Try wildcard host match
        for (pattern, route_id) in &self.wildcard_hosts {
            if self.matches_wildcard(pattern, &req.host) {
                if let Some(route) = self.routes.get(route_id) {
                    if self.matches_route(route, req) {
                        return Some(route);
                    }
                }
            }
        }

        // Try routes without host matcher (catch-all)
        for (_, route_id) in &self.priorities {
            if let Some(route) = self.routes.get(route_id) {
                let has_host_matcher = route.matchers.iter().any(|m| matches!(m, Matcher::Host(_)));
                if !has_host_matcher && self.matches_route(route, req) {
                    return Some(route);
                }
            }
        }

        None
    }

    /// Validate route configuration
    fn validate_route(&self, route: &Route) -> Result<(), EdgeError> {
        if route.id.is_empty() {
            return Err(EdgeError::RoutingError("Route ID cannot be empty".into()));
        }

        if route.upstreams.is_empty() {
            return Err(EdgeError::RoutingError(
                "Route must have at least one upstream".into(),
            ));
        }

        for upstream in &route.upstreams {
            if upstream.url.is_empty() {
                return Err(EdgeError::RoutingError(
                    "Upstream URL cannot be empty".into(),
                ));
            }
        }

        // Validate regex matchers
        for matcher in &route.matchers {
            if let Matcher::PathRegex(pattern) = matcher {
                Regex::new(pattern)
                    .map_err(|e| EdgeError::RoutingError(format!("Invalid path regex: {}", e)))?;
            }
            if let Matcher::HostRegex(pattern) = matcher {
                Regex::new(pattern)
                    .map_err(|e| EdgeError::RoutingError(format!("Invalid host regex: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Check if request matches route
    fn matches_route(&self, route: &Route, req: &RequestInfo) -> bool {
        for matcher in &route.matchers {
            if !self.matches_single(matcher, req) {
                return false;
            }
        }
        true
    }

    /// Match a single matcher against request
    fn matches_single(&self, matcher: &Matcher, req: &RequestInfo) -> bool {
        match matcher {
            Matcher::Host(host) => self.matches_host(host, &req.host),
            Matcher::HostRegex(pattern) => Regex::new(pattern)
                .map(|re| re.is_match(&req.host))
                .unwrap_or(false),
            Matcher::Path(path) => req.path == *path,
            Matcher::PathPrefix(prefix) => req.path.starts_with(prefix),
            Matcher::PathRegex(pattern) => Regex::new(pattern)
                .map(|re| re.is_match(&req.path))
                .unwrap_or(false),
            Matcher::Header(name, value) => {
                req.headers.get(name).map(|v| v == value).unwrap_or(false)
            }
            Matcher::Query(key, value) => req.query.get(key).map(|v| v == value).unwrap_or(false),
            Matcher::Method(method) => req.method == *method,
        }
    }

    /// Match host pattern
    fn matches_host(&self, pattern: &str, host: &str) -> bool {
        let _ = self; // Self not needed for this method
        if pattern.starts_with("*.") {
            // Wildcard match
            let suffix = &pattern[1..]; // Remove the *
            host.ends_with(suffix)
        } else {
            host == pattern
        }
    }

    /// Match wildcard pattern
    fn matches_wildcard(&self, pattern: &str, host: &str) -> bool {
        let _ = self; // Self not needed for this method
        let suffix = &pattern[1..]; // Remove the *
        host.ends_with(suffix) && host.len() > suffix.len()
    }

    /// Clear all routes
    pub fn clear(&mut self) {
        self.routes.clear();
        self.host_index.clear();
        self.wildcard_hosts.clear();
        self.priorities.clear();
    }

    /// Get route by ID
    pub fn get_route(&self, route_id: &str) -> Option<&Route> {
        self.routes.get(route_id)
    }

    /// List all route IDs
    pub fn list_routes(&self) -> Vec<&str> {
        self.routes.keys().map(|s| s.as_str()).collect()
    }

    /// Get round-robin counter
    pub fn rr_counter(&self) -> &AtomicUsize {
        &self.rr_counter
    }

    /// Watch configuration for changes (placeholder)
    pub async fn watch_config(&mut self, source: ConfigSource) -> Result<(), EdgeError> {
        match source {
            ConfigSource::File(path) => {
                // Would watch file for changes
                debug!("Watching config file: {}", path);
            }
            ConfigSource::Quic(channel) => {
                // Would subscribe to QUIC channel
                debug!("Watching QUIC channel: {}", channel);
            }
            ConfigSource::Kubernetes => {
                // Would watch K8s CRDs
                debug!("Watching Kubernetes CRDs");
            }
            ConfigSource::Static => {
                // No watching needed
                debug!("Static configuration, no watching");
            }
        }
        Ok(())
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

/// Route definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    /// Unique route ID
    pub id: String,
    /// Priority (higher = more important)
    pub priority: i32,
    /// Matchers (all must match)
    pub matchers: Vec<Matcher>,
    /// Upstream backends
    pub upstreams: Vec<Upstream>,
    /// Middleware chain
    pub middleware: Vec<Middleware>,
    /// Load balancer strategy
    pub load_balancer: LoadBalancerStrategy,
    /// TLS configuration (optional, for SNI routing)
    pub tls: Option<RouteTlsConfig>,
    /// Route enabled
    pub enabled: bool,
}

impl Default for Route {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            priority: 0,
            matchers: Vec::new(),
            upstreams: Vec::new(),
            middleware: Vec::new(),
            load_balancer: LoadBalancerStrategy::RoundRobin,
            tls: None,
            enabled: true,
        }
    }
}

/// Request info for matching
#[derive(Debug, Clone)]
pub struct RequestInfo {
    /// HTTP method
    pub method: String,
    /// Host header
    pub host: String,
    /// Request path
    pub path: String,
    /// Request headers
    pub headers: HashMap<String, String>,
    /// Query parameters
    pub query: HashMap<String, String>,
    /// Client IP
    pub client_ip: String,
}

impl RequestInfo {
    /// Create from HTTP request
    pub fn from_request<B>(req: &hyper::Request<B>) -> Self {
        let method = req.method().to_string();
        let host = req
            .headers()
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();

        let path = req.uri().path().to_string();

        let mut headers = HashMap::new();
        for (name, value) in req.headers() {
            if let Ok(v) = value.to_str() {
                headers.insert(name.to_string(), v.to_string());
            }
        }

        let mut query = HashMap::new();
        if let Some(query_str) = req.uri().query() {
            for pair in query_str.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    query.insert(key.to_string(), value.to_string());
                }
            }
        }

        let client_ip = req
            .headers()
            .get("X-Real-IP")
            .and_then(|h| h.to_str().ok())
            .or_else(|| {
                req.headers()
                    .get("X-Forwarded-For")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.split(',').next().unwrap_or("").trim())
            })
            .unwrap_or("unknown")
            .to_string();

        Self {
            method,
            host,
            path,
            headers,
            query,
            client_ip,
        }
    }
}

/// Upstream backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upstream {
    /// Upstream URL (http:// or https://)
    pub url: String,
    /// Weight for weighted round-robin
    pub weight: u32,
    /// Whether upstream is healthy
    pub healthy: bool,
    /// Health check configuration
    pub health_check: Option<HealthCheckConfig>,
    /// Circuit breaker configuration
    pub circuit_breaker: Option<CircuitBreakerConfig>,
}

impl Default for Upstream {
    fn default() -> Self {
        Self {
            url: String::new(),
            weight: 1,
            healthy: true,
            health_check: None,
            circuit_breaker: None,
        }
    }
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check path
    pub path: String,
    /// Check interval in seconds
    pub interval_secs: u64,
    /// Check timeout in seconds
    pub timeout_secs: u64,
    /// Unhealthy threshold
    pub unhealthy_threshold: u32,
    /// Healthy threshold
    pub healthy_threshold: u32,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open circuit
    pub failure_threshold: u32,
    /// Success threshold to close circuit
    pub success_threshold: u32,
    /// Timeout before attempting to close (seconds)
    pub timeout_secs: u64,
}

/// Matcher types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Matcher {
    /// Exact host match
    Host(String),
    /// Regex host match
    HostRegex(String),
    /// Exact path match
    Path(String),
    /// Path prefix match
    PathPrefix(String),
    /// Regex path match
    PathRegex(String),
    /// Header match
    Header(String, String),
    /// Query parameter match
    Query(String, String),
    /// HTTP method match
    Method(String),
}

/// Load balancer strategy
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LoadBalancerStrategy {
    /// Round-robin
    RoundRobin,
    /// Weighted round-robin
    WeightedRoundRobin,
    /// Least connections
    LeastConnections,
    /// IP hash
    IpHash,
    /// Random
    Random,
}

/// Middleware chain
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Middleware {
    /// Strip path prefix
    StripPrefix { prefix: String },
    /// Add path prefix
    AddPrefix { prefix: String },
    /// Rate limiting
    RateLimit { config: RateLimitConfig },
    /// Basic authentication
    BasicAuth { users: HashMap<String, String> },
    /// JWT authentication
    JwtAuth { config: JwtConfig },
    /// CORS handling
    Cors { config: CorsConfig },
    /// Response compression
    Compress,
    /// Request ID generation
    RequestId,
    /// Add custom headers
    AddHeaders { headers: HashMap<String, String> },
    /// Remove headers
    RemoveHeaders { headers: Vec<String> },
    /// Redirect
    Redirect { to: String, permanent: bool },
    /// Rewrite path
    RewritePath {
        pattern: String,
        replacement: String,
    },
}

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per second
    pub requests_per_second: f64,
    /// Burst size
    pub burst_size: u32,
    /// Key strategy
    pub key_strategy: RateLimitKeyStrategy,
}

/// Rate limit key strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RateLimitKeyStrategy {
    /// By client IP
    Ip,
    /// By header value
    Header(String),
    /// By cookie value
    Cookie(String),
}

/// JWT validation config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// JWKS URL for key verification
    pub jwks_url: String,
    /// Expected issuer
    pub issuer: Option<String>,
    /// Expected audience
    pub audience: Option<String>,
}

/// CORS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// Allowed origins
    pub allowed_origins: Vec<String>,
    /// Allowed methods
    pub allowed_methods: Vec<String>,
    /// Allowed headers
    pub allowed_headers: Vec<String>,
    /// Allow credentials
    pub allow_credentials: bool,
    /// Max age in seconds
    pub max_age: Option<u32>,
}

/// Route TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteTlsConfig {
    /// Certificate file path
    pub cert_file: Option<String>,
    /// Key file path
    pub key_file: Option<String>,
    /// Use ACME for this route
    pub acme: bool,
}

/// Configuration source
#[derive(Debug, Clone)]
pub enum ConfigSource {
    /// Watch file for changes
    File(String),
    /// Subscribe to QUIC channel
    Quic(String),
    /// Read from Kubernetes CRDs
    Kubernetes,
    /// No changes
    Static,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_creation() {
        let router = Router::new();
        assert_eq!(router.routes.len(), 0);
    }

    #[test]
    fn test_add_route() {
        let mut router = Router::new();

        let route = Route {
            id: "test-route".to_string(),
            priority: 100,
            matchers: vec![Matcher::Host("example.com".to_string())],
            upstreams: vec![Upstream {
                url: "http://localhost:8080".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let result = router.add_route(route);
        assert!(result.is_ok());
        assert_eq!(router.routes.len(), 1);
    }

    #[test]
    fn test_match_request() {
        let mut router = Router::new();

        let route = Route {
            id: "test-route".to_string(),
            priority: 100,
            matchers: vec![
                Matcher::Host("example.com".to_string()),
                Matcher::PathPrefix("/api".to_string()),
            ],
            upstreams: vec![Upstream {
                url: "http://localhost:8080".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        router.add_route(route).unwrap();

        let req = RequestInfo {
            method: "GET".to_string(),
            host: "example.com".to_string(),
            path: "/api/users".to_string(),
            headers: HashMap::new(),
            query: HashMap::new(),
            client_ip: "127.0.0.1".to_string(),
        };

        let matched = router.match_request(&req);
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().id, "test-route");
    }

    #[test]
    fn test_wildcard_host() {
        let mut router = Router::new();

        let route = Route {
            id: "wildcard-route".to_string(),
            priority: 100,
            matchers: vec![Matcher::Host("*.example.com".to_string())],
            upstreams: vec![Upstream {
                url: "http://localhost:8080".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        router.add_route(route).unwrap();

        let req = RequestInfo {
            method: "GET".to_string(),
            host: "api.example.com".to_string(),
            path: "/".to_string(),
            headers: HashMap::new(),
            query: HashMap::new(),
            client_ip: "127.0.0.1".to_string(),
        };

        let matched = router.match_request(&req);
        assert!(matched.is_some());
    }
}
