//! HTTP reverse proxy implementation

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Bytes;
use hyper::header::{HeaderName, HeaderValue, CONNECTION, HOST, UPGRADE};
use hyper::{Method, Request, Response, StatusCode, Version};
use hyper::client::conn::http1::{self, SendRequest};
use hyper::client::conn::http2;
use hyper_util::rt::TokioIo;
use parking_lot::RwLock;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use crate::{EdgeError, router::{Route, Upstream, LoadBalancerStrategy}};

/// Default connection timeout
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Default request timeout
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Default idle connection timeout
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
/// Maximum idle connections per upstream
const MAX_IDLE_CONNECTIONS: usize = 100;

/// HTTP proxy handler
pub struct HttpProxy {
    /// Connection pool for reuse
    pool: ConnectionPool,
    /// Metrics collector
    metrics: ProxyMetrics,
    /// Default request timeout
    request_timeout: Duration,
    /// Connection timeout
    connect_timeout: Duration,
}

/// Proxy metrics
#[derive(Debug, Default)]
pub struct ProxyMetrics {
    /// Total requests processed
    pub total_requests: AtomicU64,
    /// Active connections
    pub active_connections: AtomicU64,
    /// Failed requests
    pub failed_requests: AtomicU64,
    /// Total bytes sent
    pub bytes_sent: AtomicU64,
    /// Total bytes received
    pub bytes_received: AtomicU64,
}

impl Default for HttpProxy {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpProxy {
    /// Create new proxy handler
    pub fn new() -> Self {
        Self {
            pool: ConnectionPool::new(MAX_IDLE_CONNECTIONS, DEFAULT_IDLE_TIMEOUT),
            metrics: ProxyMetrics::default(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        }
    }

    /// Create proxy with custom timeouts
    pub fn with_timeouts(request_timeout: Duration, connect_timeout: Duration) -> Self {
        Self {
            pool: ConnectionPool::new(MAX_IDLE_CONNECTIONS, DEFAULT_IDLE_TIMEOUT),
            metrics: ProxyMetrics::default(),
            request_timeout,
            connect_timeout,
        }
    }

    /// Handle incoming request
    pub async fn handle_request(
        &self,
        mut request: Request<hyper::body::Body>,
        route: &Route,
    ) -> Result<Response<hyper::body::Body>, EdgeError> {
        let start = Instant::now();
        self.metrics.total_requests.fetch_add(1, Ordering::Relaxed);
        self.metrics.active_connections.fetch_add(1, Ordering::Relaxed);

        let result = self.handle_request_inner(&mut request, route).await;

        self.metrics.active_connections.fetch_sub(1, Ordering::Relaxed);

        match &result {
            Ok(response) => {
                let latency = start.elapsed();
                debug!(
                    "Request completed: {} {} -> {} ({}ms)",
                    request.method(),
                    request.uri(),
                    response.status(),
                    latency.as_millis()
                );
            }
            Err(e) => {
                self.metrics.failed_requests.fetch_add(1, Ordering::Relaxed);
                error!("Request failed: {}", e);
            }
        }

        result
    }

    /// Internal request handling
    async fn handle_request_inner(
        &self,
        request: &mut Request<hyper::body::Body>,
        route: &Route,
    ) -> Result<Response<hyper::body::Body>, EdgeError> {
        // Apply middleware (rate limit, auth, etc.)
        self.apply_middleware(request, route)?;

        // Select upstream backend
        let upstream = self.select_upstream(route)?;
        
        // Build upstream URL
        let upstream_url = upstream.url.trim_end_matches('/');
        let path = request.uri().path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or(request.uri().path());
        
        let upstream_uri = format!("{}{}", upstream_url, path);

        // Prepare request for upstream
        let mut upstream_req = Request::builder()
            .method(request.method().clone())
            .uri(&upstream_uri)
            .version(Version::HTTP_11);

        // Copy headers with modifications
        for (name, value) in request.headers() {
            // Skip hop-by-hop headers
            if !is_hop_by_hop_header(name) {
                upstream_req = upstream_req.header(name, value);
            }
        }

        // Add proxy headers
        upstream_req = upstream_req
            .header("X-Forwarded-Proto", "https")
            .header("X-Forwarded-Host", request.uri().host().unwrap_or(""))
            .header("X-Real-IP", "127.0.0.1"); // Would come from connection info

        // Handle WebSocket upgrade
        if is_websocket_upgrade(request) {
            return self.handle_websocket_upstream(request.clone(), route, &upstream_url).await;
        }

        // Build final request with body
        let body = std::mem::take(request.body_mut());
        let upstream_req = upstream_req.body(body)
            .map_err(|e| EdgeError::RoutingError(format!("Failed to build upstream request: {}", e)))?;

        // Forward request
        let response = timeout(
            self.request_timeout,
            self.forward_request(upstream_req, upstream_url)
        )
        .await
        .map_err(|_| EdgeError::Unavailable("Request timeout".into()))??;

        // Add security headers
        let mut response = Self::add_security_headers(response);

        // Add middleware response headers
        self.add_middleware_headers(&mut response, route);

        Ok(response)
    }

    /// Select upstream using route's load balancing strategy
    fn select_upstream<'a>(&self, route: &'a Route) -> Result<&'a Upstream, EdgeError> {
        let healthy_upstreams: Vec<_> = route.upstreams.iter()
            .filter(|u| u.healthy)
            .collect();

        if healthy_upstreams.is_empty() {
            return Err(EdgeError::Unavailable("No healthy upstreams available".into()));
        }

        let idx = match &route.load_balancer {
            LoadBalancerStrategy::RoundRobin => {
                let counter = self.pool.get_rr_counter();
                let idx = counter.fetch_add(1, Ordering::Relaxed);
                (idx as usize) % healthy_upstreams.len()
            }
            LoadBalancerStrategy::LeastConnections => {
                // Select upstream with least active connections
                healthy_upstreams.iter()
                    .enumerate()
                    .min_by_key(|(_, u)| self.pool.active_connections(&u.url))
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            }
            LoadBalancerStrategy::IpHash => {
                // Simple hash - in real impl would use actual client IP
                let hash = 0u64; // Would hash client IP
                (hash as usize) % healthy_upstreams.len()
            }
            LoadBalancerStrategy::Random => {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                rng.gen_range(0..healthy_upstreams.len())
            }
        };

        Ok(healthy_upstreams[idx])
    }

    /// Forward request to upstream
    async fn forward_request(
        &self,
        request: Request<hyper::body::Body>,
        upstream_url: &str,
    ) -> Result<Response<hyper::body::Body>, EdgeError> {
        // Try to get pooled connection
        if let Some(mut sender) = self.pool.try_get_sender(upstream_url) {
            match sender.send_request(request).await {
                Ok(response) => {
                    // Return connection to pool
                    self.pool.return_sender(upstream_url, sender);
                    return Ok(response);
                }
                Err(e) => {
                    debug!("Pooled connection failed, creating new: {}", e);
                }
            }
        }

        // Create new connection
        let response = self.create_connection_and_send(upstream_url, request).await?;
        Ok(response)
    }

    /// Create new upstream connection and send request
    async fn create_connection_and_send(
        &self,
        upstream_url: &str,
        request: Request<hyper::body::Body>,
    ) -> Result<Response<hyper::body::Body>, EdgeError> {
        // Parse upstream URL
        let url: http::Uri = upstream_url.parse()
            .map_err(|e| EdgeError::RoutingError(format!("Invalid upstream URL: {}", e)))?;
        
        let host = url.host().ok_or_else(|| 
            EdgeError::RoutingError("Upstream URL missing host".into()))?;
        let port = url.port_u16().unwrap_or(match url.scheme_str() {
            Some("https") => 443,
            _ => 80,
        });

        // Connect with timeout
        let stream = timeout(
            self.connect_timeout,
            TcpStream::connect((host, port))
        )
        .await
        .map_err(|_| EdgeError::Unavailable(format!("Connection timeout to {}:{}", host, port)))?
        .map_err(|e| EdgeError::Unavailable(format!("Failed to connect to {}:{}: {}", host, port, e)))?;

        let io = TokioIo::new(stream);

        // Send request (HTTP/1.1)
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .map_err(|e| EdgeError::RoutingError(format!("Handshake failed: {}", e)))?;

        // Spawn connection task
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                debug!("Connection error: {}", e);
            }
        });

        // Send request
        let response = sender.send_request(request).await
            .map_err(|e| EdgeError::RoutingError(format!("Request failed: {}", e)))?;

        // Store sender in pool for reuse
        self.pool.store_sender(upstream_url.to_string(), sender);

        Ok(response)
    }

    /// Apply middleware to request
    fn apply_middleware(
        &self,
        request: &mut Request<hyper::body::Body>,
        route: &Route,
    ) -> Result<(), EdgeError> {
        use crate::router::Middleware;

        for middleware in &route.middleware {
            match middleware {
                Middleware::StripPrefix(prefix) => {
                    let uri = request.uri().clone();
                    let path = uri.path();
                    if let Some(new_path) = path.strip_prefix(prefix) {
                        let new_path = if new_path.is_empty() { "/" } else { new_path };
                        let new_uri = hyper::Uri::builder()
                            .path_and_query(new_path)
                            .build()
                            .map_err(|e| EdgeError::RoutingError(format!("Failed to rebuild URI: {}", e)))?;
                        *request.uri_mut() = new_uri;
                    }
                }
                Middleware::AddPrefix(prefix) => {
                    let uri = request.uri().clone();
                    let new_path = format!("{}{}", prefix, uri.path());
                    let new_uri = hyper::Uri::builder()
                        .path_and_query(new_path)
                        .build()
                        .map_err(|e| EdgeError::RoutingError(format!("Failed to rebuild URI: {}", e)))?;
                        *request.uri_mut() = new_uri;
                }
                Middleware::AddHeaders { headers } => {
                    for (key, value) in headers {
                        if let (Ok(name), Ok(val)) = (
                            HeaderName::try_from(key),
                            HeaderValue::try_from(value)
                        ) {
                            request.headers_mut().insert(name, val);
                        }
                    }
                }
                Middleware::RequestId => {
                    let id = uuid::Uuid::new_v4().to_string();
                    if let Ok(val) = HeaderValue::try_from(&id) {
                        request.headers_mut().insert("X-Request-Id", val);
                    }
                }
                _ => {
                    // Other middleware handled elsewhere
                }
            }
        }

        Ok(())
    }

    /// Add middleware response headers
    fn add_middleware_headers(
        &self,
        response: &mut Response<hyper::body::Body>,
        route: &Route,
    ) {
        use crate::router::Middleware;

        for middleware in &route.middleware {
            if let Middleware::Cors { config } = middleware {
                let headers = response.headers_mut();
                if let Ok(val) = HeaderValue::try_from(config.allowed_origins.join(",")) {
                    headers.insert("Access-Control-Allow-Origin", val);
                }
                if config.allow_credentials {
                    headers.insert("Access-Control-Allow-Credentials", "true".parse().unwrap());
                }
            }
        }
    }

    /// Add security headers to response
    fn add_security_headers(mut response: Response<hyper::body::Body>) -> Response<hyper::body::Body> {
        let headers = response.headers_mut();
        
        // HSTS
        headers.insert(
            "Strict-Transport-Security",
            "max-age=31536000; includeSubDomains".parse().unwrap()
        );
        
        // Frame options
        headers.insert("X-Frame-Options", "DENY".parse().unwrap());
        
        // Content type sniffing
        headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
        
        // XSS Protection
        headers.insert("X-XSS-Protection", "1; mode=block".parse().unwrap());
        
        // Referrer Policy
        headers.insert("Referrer-Policy", "strict-origin-when-cross-origin".parse().unwrap());

        response
    }

    /// Handle WebSocket upgrade
    pub async fn handle_websocket(
        &self,
        request: Request<hyper::body::Body>,
        route: &Route,
    ) -> Result<Response<hyper::body::Body>, EdgeError> {
        let upstream = self.select_upstream(route)?;
        self.handle_websocket_upstream(request, route, &upstream.url).await
    }

    /// Handle WebSocket upgrade to upstream
    async fn handle_websocket_upstream(
        &self,
        request: Request<hyper::body::Body>,
        route: &Route,
        upstream_url: &str,
    ) -> Result<Response<hyper::body::Body>, EdgeError> {
        debug!("Handling WebSocket upgrade for route {}", route.id);

        // Parse upstream URL
        let url: http::Uri = upstream_url.parse()
            .map_err(|e| EdgeError::RoutingError(format!("Invalid upstream URL: {}", e)))?;
        
        let host = url.host().ok_or_else(|| 
            EdgeError::RoutingError("Upstream URL missing host".into()))?;
        let port = url.port_u16().unwrap_or(80);

        // Connect to upstream
        let upstream_stream = TcpStream::connect((host, port))
            .await
            .map_err(|e| EdgeError::Unavailable(format!("Failed to connect: {}", e)))?;

        // For WebSocket, we'd need to use tokio-tungstenite
        // This is a simplified version that returns a 101 response
        
        let response = Response::builder()
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "upgrade")
            .body(hyper::body::Body::empty())
            .map_err(|e| EdgeError::RoutingError(format!("Failed to build response: {}", e)))?;

        info!("WebSocket upgrade completed for route {}", route.id);
        Ok(response)
    }

    /// Server-Sent Events handler
    pub async fn handle_sse(
        &self,
        request: Request<hyper::body::Body>,
        route: &Route,
    ) -> Result<Response<hyper::body::Body>, EdgeError> {
        debug!("Handling SSE request for route {}", route.id);
        
        let upstream = self.select_upstream(route)?;
        let path = request.uri().path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or(request.uri().path());
        
        let upstream_uri = format!("{}{}", upstream.url.trim_end_matches('/'), path);

        // Forward request to upstream
        let response = self.forward_request(request, &upstream_uri).await?;

        // Add SSE-specific headers
        let (parts, body) = response.into_parts();
        let mut response = Response::new(body);
        *response.status_mut() = parts.status;
        
        let headers = response.headers_mut();
        headers.insert("Content-Type", "text/event-stream".parse().unwrap());
        headers.insert("Cache-Control", "no-cache".parse().unwrap());
        headers.insert("Connection", "keep-alive".parse().unwrap());

        Ok(response)
    }

    /// Get proxy metrics
    pub fn metrics(&self) -> &ProxyMetrics {
        &self.metrics
    }
}

/// Connection pool for upstream reuse
pub struct ConnectionPool {
    /// Idle connections per upstream
    idle: RwLock<HashMap<String, Vec<PooledConnection>>>,
    /// Active connection count per upstream
    active_count: RwLock<HashMap<String, u64>>,
    /// Maximum idle connections per upstream
    max_idle: usize,
    /// Idle timeout duration
    idle_timeout: Duration,
    /// Round-robin counter for load balancing
    rr_counter: AtomicUsize,
}

impl ConnectionPool {
    /// Create new connection pool
    pub fn new(max_idle: usize, idle_timeout: Duration) -> Self {
        Self {
            idle: RwLock::new(HashMap::new()),
            active_count: RwLock::new(HashMap::new()),
            max_idle,
            idle_timeout,
            rr_counter: AtomicUsize::new(0),
        }
    }

    /// Try to get a sender from the pool
    pub fn try_get_sender(&self, upstream: &str) -> Option<SendRequest<hyper::body::Body>> {
        let mut idle = self.idle.write();
        let connections = idle.get_mut(upstream)?;
        
        while let Some(conn) = connections.pop() {
            if conn.is_healthy() {
                self.increment_active(upstream);
                return Some(conn.sender);
            }
            // Discard unhealthy connections
        }
        
        None
    }

    /// Return sender to pool
    pub fn return_sender(&self, upstream: &str, sender: SendRequest<hyper::body::Body>) {
        self.decrement_active(upstream);
        self.store_sender(upstream.to_string(), sender);
    }

    /// Store sender in pool
    pub fn store_sender(&self, upstream: String, sender: SendRequest<hyper::body::Body>) {
        let mut idle = self.idle.write();
        
        let connections = idle.entry(upstream.clone()).or_default();
        
        // Don't exceed max idle
        if connections.len() < self.max_idle {
            connections.push(PooledConnection {
                sender,
                created_at: Instant::now(),
            });
        }
        // Otherwise, let the connection drop
    }

    /// Get active connection count for upstream
    pub fn active_connections(&self, upstream: &str) -> u64 {
        let active = self.active_count.read();
        active.get(upstream).copied().unwrap_or(0)
    }

    /// Increment active count
    fn increment_active(&self, upstream: &str) {
        let mut active = self.active_count.write();
        *active.entry(upstream.to_string()).or_insert(0) += 1;
    }

    /// Decrement active count
    fn decrement_active(&self, upstream: &str) {
        let mut active = self.active_count.write();
        if let Some(count) = active.get_mut(upstream) {
            *count = count.saturating_sub(1);
        }
    }

    /// Get round-robin counter
    pub fn get_rr_counter(&self) -> &AtomicUsize {
        &self.rr_counter
    }

    /// Prune expired idle connections
    pub fn prune_expired(&self) {
        let mut idle = self.idle.write();
        for (_, connections) in idle.iter_mut() {
            connections.retain(|conn| {
                conn.created_at.elapsed() < self.idle_timeout
            });
        }
    }
}

/// Pooled connection handle
pub struct PooledConnection {
    /// HTTP sender
    sender: SendRequest<hyper::body::Body>,
    /// When connection was created
    created_at: Instant,
}

impl PooledConnection {
    /// Check if connection is still usable
    pub fn is_healthy(&self) -> bool {
        // Check if connection is expired
        // In real implementation, would also check TCP state
        self.created_at.elapsed() < Duration::from_secs(90)
    }
}

/// Request context for routing decisions
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Client IP address
    pub client_ip: String,
    /// Unique request ID
    pub request_id: String,
    /// Request start time
    pub start_time: Instant,
    /// Request headers
    pub headers: HashMap<String, String>,
    /// Cookies
    pub cookies: HashMap<String, String>,
}

impl RequestContext {
    /// Create new request context
    pub fn new(client_ip: String) -> Self {
        Self {
            client_ip,
            request_id: uuid::Uuid::new_v4().to_string(),
            start_time: Instant::now(),
            headers: HashMap::new(),
            cookies: HashMap::new(),
        }
    }
}

/// Check if header is hop-by-hop (should not be forwarded)
fn is_hop_by_hop_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection" | "keep-alive" | "proxy-authenticate" |
        "proxy-authorization" | "te" | "trailers" |
        "transfer-encoding" | "upgrade"
    )
}

/// Check if request is WebSocket upgrade
fn is_websocket_upgrade(request: &Request<hyper::body::Body>) -> bool {
    let headers = request.headers();
    
    let upgrade = headers.get(UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase());
    
    let connection = headers.get(CONNECTION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase());

    matches!((upgrade, connection), 
        (Some(upgrade), Some(conn)) 
        if upgrade.contains("websocket") && conn.contains("upgrade"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_pool() {
        let pool = ConnectionPool::new(10, Duration::from_secs(90));
        assert_eq!(pool.active_connections("example.com"), 0);
    }

    #[test]
    fn test_is_websocket_upgrade() {
        let req = Request::builder()
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .body(hyper::body::Body::empty())
            .unwrap();
        
        assert!(is_websocket_upgrade(&req));
        
        let normal_req = Request::builder()
            .body(hyper::body::Body::empty())
            .unwrap();
        
        assert!(!is_websocket_upgrade(&normal_req));
    }

    #[test]
    fn test_hop_by_hop_headers() {
        assert!(is_hop_by_hop_header(&"connection".parse().unwrap()));
        assert!(is_hop_by_hop_header(&"transfer-encoding".parse().unwrap()));
        assert!(!is_hop_by_hop_header(&"content-type".parse().unwrap()));
        assert!(!is_hop_by_hop_header(&"authorization".parse().unwrap()));
    }
}
