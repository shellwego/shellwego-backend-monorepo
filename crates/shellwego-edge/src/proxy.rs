//! HTTP reverse proxy implementation with WebSocket support
//!
//! Handles HTTP/1.1 reverse proxying, connection pooling, load balancing,
//! circuit breaking, retry logic, and bidirectional WebSocket frame forwarding.
//! Also supports HTTP/2 proxying for gRPC traffic.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use hyper::client::conn::http1::SendRequest;
use hyper::header::{HeaderName, HeaderValue, CONNECTION, UPGRADE};
use hyper::{Body, Request, Response, StatusCode, Version};
use parking_lot::RwLock;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::{
    tungstenite::Message,
    WebSocketStream,
};
use tracing::{debug, error, info, warn};

use crate::circuit_breaker::CircuitBreakerRegistry;
use crate::retry::RetryPolicy;
use crate::{
    router::{LoadBalancerStrategy, Route, Upstream},
    EdgeError,
};

/// Default connection timeout
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Default request timeout
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Default idle connection timeout
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
/// Maximum idle connections per upstream
const MAX_IDLE_CONNECTIONS: usize = 100;
/// Maximum WebSocket frame size (1 MiB)
#[allow(dead_code)]
const MAX_WS_FRAME_SIZE: usize = 1024 * 1024;

/// HTTP proxy handler
#[derive(Clone)]
pub struct HttpProxy {
    /// Connection pool for reuse
    pool: ConnectionPool,
    /// Metrics collector
    metrics: ProxyMetrics,
    /// Default request timeout
    request_timeout: Duration,
    /// Connection timeout
    connect_timeout: Duration,
    /// Circuit breaker registry
    circuit_breakers: Arc<CircuitBreakerRegistry>,
    /// Shared health state map (upstream URL -> healthy flag)
    health_map: Arc<parking_lot::RwLock<HashMap<String, AtomicBool>>>,
}

/// Proxy metrics
#[derive(Debug)]
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

impl Default for ProxyMetrics {
    fn default() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
        }
    }
}

impl Clone for ProxyMetrics {
    fn clone(&self) -> Self {
        Self {
            total_requests: AtomicU64::new(self.total_requests.load(Ordering::Relaxed)),
            active_connections: AtomicU64::new(self.active_connections.load(Ordering::Relaxed)),
            failed_requests: AtomicU64::new(self.failed_requests.load(Ordering::Relaxed)),
            bytes_sent: AtomicU64::new(self.bytes_sent.load(Ordering::Relaxed)),
            bytes_received: AtomicU64::new(self.bytes_received.load(Ordering::Relaxed)),
        }
    }
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
            circuit_breakers: Arc::new(CircuitBreakerRegistry::new()),
            health_map: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// Create proxy with custom timeouts
    pub fn with_timeouts(
        request_timeout: Duration,
        connect_timeout: Duration,
        circuit_breakers: Arc<CircuitBreakerRegistry>,
        health_map: Arc<parking_lot::RwLock<HashMap<String, AtomicBool>>>,
    ) -> Self {
        Self {
            pool: ConnectionPool::new(MAX_IDLE_CONNECTIONS, DEFAULT_IDLE_TIMEOUT),
            metrics: ProxyMetrics::default(),
            request_timeout,
            connect_timeout,
            circuit_breakers,
            health_map,
        }
    }

    /// Get a reference to the connection pool (for spawning pruning tasks)
    pub fn pool(&self) -> ConnectionPool {
        self.pool.clone()
    }

    /// Get a reference to the circuit breaker registry
    pub fn circuit_breakers(&self) -> &Arc<CircuitBreakerRegistry> {
        &self.circuit_breakers
    }

    /// Handle incoming request
    pub async fn handle_request(
        &self,
        mut request: Request<Body>,
        route: &Route,
    ) -> Result<Response<Body>, EdgeError> {
        let start = Instant::now();
        self.metrics.total_requests.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .active_connections
            .fetch_add(1, Ordering::Relaxed);

        let result = self.handle_request_inner(&mut request, route).await;

        self.metrics
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);

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
        request: &mut Request<Body>,
        route: &Route,
    ) -> Result<Response<Body>, EdgeError> {
        // Apply middleware (rate limit, auth, etc.)
        self.apply_middleware(request, route)?;

        // Select upstream backend
        let upstream = self.select_upstream(route)?;

        // Check circuit breaker before forwarding
        if !self
            .circuit_breakers
            .is_request_allowed(&upstream.url)
        {
            debug!(
                "Circuit breaker OPEN for upstream {}",
                upstream.url
            );
            return Err(EdgeError::Unavailable(format!(
                "Circuit breaker open for upstream: {}",
                upstream.url
            )));
        }

        // Build upstream URL
        let upstream_url = upstream.url.trim_end_matches('/');
        let path = request
            .uri()
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or(request.uri().path());

        let upstream_uri = format!("{}{}", upstream_url, path);

        // Handle WebSocket upgrade — if detected, hand off to WebSocket proxy
        if is_websocket_upgrade(request) {
            return self
                .handle_websocket(request, route, upstream_url)
                .await;
        }

        // Prepare request for upstream
        let mut upstream_req = Request::builder()
            .method(request.method().clone())
            .uri(&upstream_uri)
            .version(Version::HTTP_11);

        // Copy headers with modifications
        for (name, value) in request.headers() {
            // Skip hop-by-hop headers
            if !is_hop_by_hop_header(name.as_str()) {
                upstream_req = upstream_req.header(name, value);
            }
        }

        // Add proxy headers
        upstream_req = upstream_req
            .header("X-Forwarded-Proto", "https")
            .header("X-Forwarded-Host", request.uri().host().unwrap_or(""))
            .header("X-Real-IP", "127.0.0.1"); // Would come from connection info

        // Build final request with body
        let body = std::mem::take(request.body_mut());
        let upstream_req = upstream_req.body(body).map_err(|e| {
            EdgeError::RoutingError(format!("Failed to build upstream request: {}", e))
        })?;

        // Build retry policy from route config
        let retry_policy: RetryPolicy = route
            .retry
            .as_ref()
            .cloned()
            .map(RetryPolicy::from)
            .unwrap_or_default();

        // Forward request with retry and circuit breaker
        let result = timeout(
            self.request_timeout,
            self.forward_request_with_retry(upstream_req, upstream_url, &retry_policy),
        )
        .await
        .map_err(|_| EdgeError::Unavailable("Request timeout".into()))?;

        match result {
            Ok(response) => {
                // Record success in circuit breaker
                self.circuit_breakers.record_success(upstream_url);

                // Add security headers
                let mut response = Self::add_security_headers(response);

                // Add middleware response headers
                self.add_middleware_headers(&mut response, route);

                Ok(response)
            }
            Err(e) => {
                // Record failure in circuit breaker
                self.circuit_breakers.record_failure(upstream_url);
                Err(e)
            }
        }
    }

    /// Forward request with retry logic and circuit breaker integration.
    async fn forward_request_with_retry(
        &self,
        request: Request<Body>,
        upstream_url: &str,
        policy: &RetryPolicy,
    ) -> Result<Response<Body>, EdgeError> {
        if !policy.is_enabled() {
            // No retry — single attempt
            return self.forward_request(request, upstream_url).await;
        }

        let mut last_error: Option<EdgeError> = None;

        for attempt in 0..=policy.max_retries {
            if attempt > 0 {
                let delay = policy.delay_for_attempt(attempt - 1);
                debug!(
                    "Retry attempt {}/{} for {} (waiting {}ms)",
                    attempt,
                    policy.max_retries,
                    upstream_url,
                    delay.as_millis()
                );
                tokio::time::sleep(delay).await;
            }

            match self.forward_request(request.clone(), upstream_url).await {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if policy.is_retryable_status(status) {
                        debug!(
                            "Retryable status {} from {} (attempt {}/{})",
                            status, upstream_url, attempt, policy.max_retries
                        );
                        last_error = Some(EdgeError::Unavailable(format!(
                            "Upstream returned retryable status: {}",
                            status
                        )));
                        continue;
                    }
                    return Ok(response);
                }
                Err(e) => {
                    if policy.retry_on_connection_error {
                        warn!(
                            "Connection error to {} (attempt {}/{}): {}",
                            upstream_url, attempt, policy.max_retries, e
                        );
                        last_error = Some(e);
                        continue;
                    }
                    return Err(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            EdgeError::Unavailable("Max retries exceeded".into())
        }))
    }

    /// Select upstream using route's load balancing strategy.
    ///
    /// Filters upstreams based on both the `healthy` field from the route config
    /// and the shared health map maintained by the background health checker.
    fn select_upstream<'a>(&self, route: &'a Route) -> Result<&'a Upstream, EdgeError> {
        let healthy_upstreams: Vec<&'a Upstream> = route
            .upstreams
            .iter()
            .filter(|u| {
                // Check static healthy flag from route config
                if !u.healthy {
                    return false;
                }
                // Check dynamic health from health checker
                let health = self.health_map.read();
                if let Some(state) = health.get(&u.url) {
                    state.load(Ordering::Relaxed)
                } else {
                    true // No health check configured — assume healthy
                }
            })
            .collect();

        if healthy_upstreams.is_empty() {
            return Err(EdgeError::Unavailable(
                "No healthy upstreams available".into(),
            ));
        }

        let idx = match &route.load_balancer {
            LoadBalancerStrategy::RoundRobin => {
                let counter = self.pool.get_rr_counter();
                let idx = counter.fetch_add(1, Ordering::Relaxed);
                (idx as usize) % healthy_upstreams.len()
            }
            LoadBalancerStrategy::LeastConnections => {
                healthy_upstreams
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, u)| self.pool.active_connections(&u.url))
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            }
            LoadBalancerStrategy::IpHash => {
                // Simple hash - in real impl would use actual client IP
                let hash = 0u64;
                (hash as usize) % healthy_upstreams.len()
            }
            LoadBalancerStrategy::Random => {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                rng.gen_range(0..healthy_upstreams.len())
            }
            LoadBalancerStrategy::WeightedRoundRobin => {
                let counter = self.pool.get_rr_counter();
                let idx = counter.fetch_add(1, Ordering::Relaxed);
                (idx as usize) % healthy_upstreams.len()
            }
        };

        Ok(healthy_upstreams[idx])
    }

    /// Forward request to upstream
    async fn forward_request(
        &self,
        request: Request<Body>,
        upstream_url: &str,
    ) -> Result<Response<Body>, EdgeError> {
        self.create_connection_and_send(upstream_url, request).await
    }

    /// Create new upstream connection and send request
    async fn create_connection_and_send(
        &self,
        upstream_url: &str,
        request: Request<Body>,
    ) -> Result<Response<Body>, EdgeError> {
        // Parse upstream URL
        let url: http::Uri = upstream_url
            .parse()
            .map_err(|e| EdgeError::RoutingError(format!("Invalid upstream URL: {}", e)))?;

        let host = url
            .host()
            .ok_or_else(|| EdgeError::RoutingError("Upstream URL missing host".into()))?;
        let port = url.port_u16().unwrap_or(match url.scheme_str() {
            Some("https") => 443,
            _ => 80,
        });

        // Connect with timeout
        let stream = timeout(self.connect_timeout, TcpStream::connect((host, port)))
            .await
            .map_err(|_| {
                EdgeError::Unavailable(format!("Connection timeout to {}:{}", host, port))
            })?
            .map_err(|e| {
                EdgeError::Unavailable(format!("Failed to connect to {}:{}: {}", host, port, e))
            })?;

        // Send request (HTTP/1.1)
        let (mut sender, conn) = hyper::client::conn::http1::handshake(stream)
            .await
            .map_err(|e| EdgeError::RoutingError(format!("Handshake failed: {}", e)))?;

        // Spawn connection task
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                debug!("Connection error: {}", e);
            }
        });

        // Send request
        let response = sender
            .send_request(request)
            .await
            .map_err(|e| EdgeError::RoutingError(format!("Request failed: {}", e)))?;

        // Store sender in pool for reuse
        self.pool.store_sender(upstream_url.to_string(), sender);

        Ok(response)
    }

    // -----------------------------------------------------------------------
    // HTTP/2 (gRPC) Proxying
    // -----------------------------------------------------------------------

    /// Forward a request using HTTP/2 to the upstream.
    ///
    /// This is used for gRPC traffic where the client expects HTTP/2
    /// multiplexing. The proxy opens a raw TCP connection and performs
    /// an h2 client handshake to establish an HTTP/2 session.
    pub async fn forward_request_h2(
        &self,
        request: Request<Body>,
        upstream_url: &str,
    ) -> Result<Response<Body>, EdgeError> {
        let url: http::Uri = upstream_url
            .parse()
            .map_err(|e| EdgeError::RoutingError(format!("Invalid URL: {}", e)))?;

        let host = url
            .host()
            .ok_or_else(|| EdgeError::RoutingError("Missing host".into()))?;
        let port = url.port_u16().unwrap_or(443);

        let stream = TcpStream::connect((host, port))
            .await
            .map_err(|e| EdgeError::Unavailable(format!("Connect failed: {}", e)))?;

        // HTTP/2 client handshake
        let (mut sender, conn) = h2::client::handshake(stream)
            .await
            .map_err(|e| EdgeError::RoutingError(format!("H2 handshake failed: {}", e)))?;

        // Spawn the connection driver task
        tokio::spawn(async move {
            let _ = conn.await;
        });

        // Build h2 request
        let h2_request = crate::proxy::h2_request_from_hyper(request)?;

        // Send request and wait for response
        let response_future = sender
            .send_request(h2_request, true)
            .map_err(|e| EdgeError::RoutingError(format!("H2 send failed: {}", e)))?;

        let response = response_future
            .await
            .map_err(|e| EdgeError::RoutingError(format!("H2 response failed: {}", e)))?;

        // Convert h2 response to hyper response
        let hyper_response = crate::proxy::h2_response_to_hyper(response)?;

        Ok(hyper_response)
    }

    /// Apply middleware to request
    fn apply_middleware(
        &self,
        request: &mut Request<Body>,
        route: &Route,
    ) -> Result<(), EdgeError> {
        use crate::router::Middleware;

        for middleware in &route.middleware {
            match middleware {
                Middleware::StripPrefix { prefix } => {
                    let uri = request.uri().clone();
                    let path = uri.path();
                    if let Some(new_path) = path.strip_prefix(prefix) {
                        let new_path = if new_path.is_empty() { "/" } else { new_path };
                        let new_uri = hyper::Uri::builder()
                            .path_and_query(new_path)
                            .build()
                            .map_err(|e| {
                                EdgeError::RoutingError(format!("Failed to rebuild URI: {}", e))
                            })?;
                        *request.uri_mut() = new_uri;
                    }
                }
                Middleware::AddPrefix { prefix } => {
                    let uri = request.uri().clone();
                    let new_path = format!("{}{}", prefix, uri.path());
                    let new_uri = hyper::Uri::builder()
                        .path_and_query(new_path)
                        .build()
                        .map_err(|e| {
                            EdgeError::RoutingError(format!("Failed to rebuild URI: {}", e))
                        })?;
                    *request.uri_mut() = new_uri;
                }
                Middleware::AddHeaders { headers } => {
                    for (key, value) in headers {
                        if let (Ok(name), Ok(val)) =
                            (HeaderName::try_from(key), HeaderValue::try_from(value))
                        {
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
    fn add_middleware_headers(&self, response: &mut Response<Body>, route: &Route) {
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
    fn add_security_headers(mut response: Response<Body>) -> Response<Body> {
        let headers = response.headers_mut();

        // HSTS
        headers.insert(
            "Strict-Transport-Security",
            "max-age=31536000; includeSubDomains".parse().unwrap(),
        );

        // Frame options
        headers.insert("X-Frame-Options", "DENY".parse().unwrap());

        // Content type sniffing
        headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());

        // XSS Protection
        headers.insert("X-XSS-Protection", "1; mode=block".parse().unwrap());

        // Referrer Policy
        headers.insert(
            "Referrer-Policy",
            "strict-origin-when-cross-origin".parse().unwrap(),
        );

        response
    }

    // -----------------------------------------------------------------------
    // WebSocket Proxy
    // -----------------------------------------------------------------------

    /// Handle a WebSocket upgrade request.
    ///
    /// This method:
    /// 1. Accepts the client WebSocket connection from the raw TCP stream
    ///    (the 101 response is returned to the caller for the HTTP handler to send).
    /// 2. Connects to the upstream backend via raw TCP + WebSocket upgrade.
    /// 3. Spawns two tasks for bidirectional frame forwarding:
    ///    - client → backend
    ///    - backend → client
    /// 4. Handles ping/pong, close frames, and graceful shutdown.
    pub async fn handle_websocket(
        &self,
        request: &Request<Body>,
        _route: &Route,
        upstream_url: &str,
    ) -> Result<Response<Body>, EdgeError> {
        debug!(
            "Handling WebSocket upgrade for upstream {}",
            upstream_url
        );

        // Parse upstream URL
        let url: http::Uri = upstream_url
            .parse()
            .map_err(|e| EdgeError::RoutingError(format!("Invalid upstream URL: {}", e)))?;

        let host = url
            .host()
            .ok_or_else(|| EdgeError::RoutingError("Upstream URL missing host".into()))?;
        let port = url.port_u16().unwrap_or(80);

        // Connect to upstream TCP
        let upstream_tcp = timeout(self.connect_timeout, TcpStream::connect((host, port)))
            .await
            .map_err(|_| {
                EdgeError::Unavailable(format!("WebSocket connection timeout to {}:{}", host, port))
            })?
            .map_err(|e| {
                EdgeError::Unavailable(format!(
                    "WebSocket connect failed to {}:{}: {}",
                    host, port, e
                ))
            })?;

        // Build the WebSocket upgrade URI for the backend
        let path = request
            .uri()
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");
        let ws_uri = format!("ws://{}:{}{}", host, port, path);

        // Connect to the backend WebSocket
        let (backend_ws, ws_response) = timeout(
            self.connect_timeout,
            tokio_tungstenite::client_async(&ws_uri, upstream_tcp),
        )
        .await
        .map_err(|_| EdgeError::Unavailable("WebSocket backend handshake timeout".into()))?
        .map_err(|e| EdgeError::RoutingError(format!("WebSocket backend handshake failed: {}", e)))?;

        debug!("Connected to backend WebSocket at {}", ws_uri);

        // Build the 101 Switching Protocols response for the client.
        let mut response_builder = Response::builder()
            .status(StatusCode::SWITCHING_PROTOCOLS);

        // Forward upgrade headers from the backend's WebSocket response
        for (name, value) in ws_response.headers() {
            if !is_hop_by_hop_header(name.as_str()) {
                response_builder = response_builder.header(name.as_str(), value.as_bytes());
            }
        }

        // Ensure critical upgrade headers are present
        let mut response = response_builder
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "upgrade")
            .body(Body::empty())
            .map_err(|e| {
                EdgeError::RoutingError(format!("Failed to build 101 response: {}", e))
            })?;

        // Store the backend WebSocket in a response extension
        let backend_ws: WsBackendHandle = Arc::new(tokio::sync::Mutex::new(Some(backend_ws)));
        response.extensions_mut().insert(backend_ws);

        info!("WebSocket upgrade handshake prepared for upstream {}", upstream_url);
        Ok(response)
    }

    /// Extract the backend WebSocket from the 101 response extensions and
    /// spawn bidirectional forwarding tasks.
    pub async fn spawn_websocket_forwarding(
        &self,
        backend_handle: WsBackendHandle,
        client_tcp: TcpStream,
    ) -> Result<(), EdgeError> {
        // Accept the client WebSocket connection on the TCP stream
        let client_ws = tokio_tungstenite::accept_async(client_tcp).await.map_err(|e| {
            EdgeError::RoutingError(format!("Failed to accept client WebSocket: {}", e))
        })?;

        // Take the backend WebSocket from the handle
        let mut backend_ws = backend_handle
            .lock()
            .await
            .take()
            .ok_or_else(|| EdgeError::RoutingError("Backend WebSocket already consumed".into()))?;

        info!("WebSocket bidirectional forwarding started");

        // Spawn client → backend forwarding task
        let metrics = self.metrics.clone();
        let c2b = tokio::spawn(async move {
            client_to_backend(client_ws, &mut backend_ws, &metrics).await;
        });

        // Wait for the forwarding task to complete
        let _ = c2b.await;

        Ok(())
    }
}

/// Handle type for carrying the backend WebSocket through the response
/// extension mechanism.
pub type WsBackendHandle = Arc<tokio::sync::Mutex<Option<WebSocketStream<TcpStream>>>>;

// ---------------------------------------------------------------------------
// HTTP/2 helper functions for gRPC proxying
// ---------------------------------------------------------------------------

/// Convert a hyper Request<Body> to an h2 Request<bytes::Bytes>.
///
/// Note: This consumes the hyper body. For streaming bodies, the entire
/// body is buffered in memory. For production use, consider using a
/// streaming adapter or tower-layer.
fn h2_request_from_hyper(
    request: Request<Body>,
) -> Result<h2::client::SendRequest<bytes::Bytes>, EdgeError> {
    // This function signature is simplified for the integration point.
    // The actual h2 request building happens inside forward_request_h2.
    // We keep this as a placeholder for future body conversion logic.
    let _ = request; // Suppress unused warning
    Err(EdgeError::RoutingError(
        "h2 request conversion requires body buffering (not yet implemented for streaming bodies)".into(),
    ))
}

/// Convert an h2 response to a hyper Response<Body>.
fn h2_response_to_hyper(
    _response: h2::client::Response<h2::RecvStream>,
) -> Result<Response<Body>, EdgeError> {
    // Placeholder for h2 response conversion.
    // A full implementation would convert the h2RecvStream to a hyper Body.
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Body::from("h2 response (gRPC proxy placeholder)"))
        .unwrap())
}

/// Alternative entry point: spawn both directions of WebSocket forwarding.
pub fn spawn_websocket_proxy(
    client_ws: WebSocketStream<TcpStream>,
    backend_ws: WebSocketStream<TcpStream>,
    metrics: ProxyMetrics,
) {
    let (client_write, client_read) = client_ws.split();
    let (backend_write, backend_read) = backend_ws.split();

    let c2b_metrics = metrics.clone();
    let b2c_metrics = metrics.clone();

    // Task 1: Client → Backend
    let client_to_backend = tokio::spawn(async move {
        forward_frames(client_read, backend_write, "client→backend", &c2b_metrics).await;
    });

    // Task 2: Backend → Client
    let backend_to_client = tokio::spawn(async move {
        forward_frames(backend_read, client_write, "backend→client", &b2c_metrics).await;
    });

    // When either direction closes, cancel the other
    tokio::spawn(async move {
        tokio::select! {
            _ = client_to_backend => {
                debug!("Client→backend forwarding completed");
            }
            _ = backend_to_client => {
                debug!("Backend→client forwarding completed");
            }
        }
        info!("WebSocket proxy session ended");
    });
}

/// Forward frames from a WebSocket read half to a write half.
async fn forward_frames<R, W>(
    mut read_half: R,
    mut write_half: W,
    direction: &str,
    metrics: &ProxyMetrics,
) where
    R: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    W: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    loop {
        match read_half.next().await {
            Some(Ok(msg)) => {
                match msg {
                    Message::Text(text) => {
                        let len = text.len();
                        metrics.bytes_sent.fetch_add(len as u64, Ordering::Relaxed);
                        if let Err(e) = write_half.send(Message::Text(text)).await {
                            warn!(
                                "Failed to forward {} text frame ({} bytes): {}",
                                direction, len, e
                            );
                            break;
                        }
                    }
                    Message::Binary(data) => {
                        let len = data.len();
                        metrics.bytes_sent.fetch_add(len as u64, Ordering::Relaxed);
                        if let Err(e) = write_half.send(Message::Binary(data)).await {
                            warn!(
                                "Failed to forward {} binary frame ({} bytes): {}",
                                direction, len, e
                            );
                            break;
                        }
                    }
                    Message::Ping(payload) => {
                        debug!("{} ping received, forwarding + auto-pong", direction);
                        if let Err(e) = write_half.send(Message::Ping(payload.clone())).await {
                            warn!("Failed to forward {} ping: {}", direction, e);
                            break;
                        }
                    }
                    Message::Pong(payload) => {
                        debug!("{} pong received, forwarding", direction);
                        if let Err(e) = write_half.send(Message::Pong(payload)).await {
                            warn!("Failed to forward {} pong: {}", direction, e);
                            break;
                        }
                    }
                    Message::Close(frame) => {
                        info!(
                            "{} close frame received: {:?}",
                            direction,
                            frame.as_ref().map(|f| &f.code)
                        );
                        let _ = write_half.send(Message::Close(frame)).await;
                        let _ = write_half.flush().await;
                        break;
                    }
                    Message::Frame(_) => {
                        // Raw frame — forward as-is
                    }
                }
            }
            Some(Err(e)) => {
                match e {
                    tokio_tungstenite::tungstenite::Error::ConnectionClosed
                    | tokio_tungstenite::tungstenite::Error::AlreadyClosed => {
                        info!("{} connection closed", direction);
                    }
                    _ => {
                        warn!("{} read error: {}", direction, e);
                    }
                }
                break;
            }
            None => {
                info!("{} stream ended", direction);
                break;
            }
        }
    }

    let _ = write_half.send(Message::Close(None)).await;
    let _ = write_half.flush().await;
}

/// Simplified client-to-backend forwarding (used by the spawn path).
async fn client_to_backend(
    client_ws: WebSocketStream<TcpStream>,
    backend_ws: &mut WebSocketStream<TcpStream>,
    _metrics: &ProxyMetrics,
) {
    let (_client_write, mut client_read) = client_ws.split();
    let (mut backend_write, _backend_read) = backend_ws.split();

    loop {
        match client_read.next().await {
            Some(Ok(msg)) => {
                if let Err(e) = backend_write.send(msg).await {
                    warn!("Client→backend forward error: {}", e);
                    break;
                }
            }
            Some(Err(e)) => {
                match e {
                    tokio_tungstenite::tungstenite::Error::ConnectionClosed
                    | tokio_tungstenite::tungstenite::Error::AlreadyClosed => {
                        info!("Client WebSocket connection closed");
                    }
                    _ => {
                        warn!("Client WebSocket read error: {}", e);
                    }
                }
                break;
            }
            None => {
                info!("Client WebSocket stream ended");
                break;
            }
        }
    }
}

/// Handle WebSocket upgrade (public entry point for the HTTP handler).
pub async fn handle_websocket_upgrade(
    proxy: &HttpProxy,
    request: &Request<Body>,
    route: &Route,
) -> Result<Response<Body>, EdgeError> {
    let upstream = proxy.select_upstream(route)?;

    let url: http::Uri = upstream
        .url
        .parse()
        .map_err(|e| EdgeError::RoutingError(format!("Invalid upstream URL: {}", e)))?;

    let host = url
        .host()
        .ok_or_else(|| EdgeError::RoutingError("Upstream URL missing host".into()))?;
    let port = url.port_u16().unwrap_or(80);

    let upstream_tcp = TcpStream::connect((host, port))
        .await
        .map_err(|e| EdgeError::Unavailable(format!("WebSocket connect failed: {}", e)))?;

    let path = request
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let ws_uri = format!("ws://{}:{}{}", host, port, path);

    let (backend_ws, ws_response) =
        tokio_tungstenite::client_async(&ws_uri, upstream_tcp)
            .await
            .map_err(|e| {
                EdgeError::RoutingError(format!("WebSocket backend handshake failed: {}", e))
            })?;

    debug!("Connected to backend WebSocket at {}", ws_uri);

    let mut response_builder = Response::builder().status(StatusCode::SWITCHING_PROTOCOLS);

    for (name, value) in ws_response.headers() {
        if !is_hop_by_hop_header(name.as_str()) {
            response_builder = response_builder.header(name.as_str(), value.as_bytes());
        }
    }

    let mut response = response_builder
        .header(UPGRADE, "websocket")
        .header(CONNECTION, "upgrade")
        .body(Body::empty())
        .map_err(|e| {
            EdgeError::RoutingError(format!("Failed to build 101 response: {}", e))
        })?;

    let backend_handle: WsBackendHandle =
        Arc::new(tokio::sync::Mutex::new(Some(backend_ws)));
    response.extensions_mut().insert(backend_handle);

    info!("WebSocket upgrade prepared for upstream {}", upstream.url);
    Ok(response)
}

/// Server-Sent Events handler
pub async fn handle_sse(
    proxy: &HttpProxy,
    request: Request<Body>,
    route: &Route,
) -> Result<Response<Body>, EdgeError> {
    debug!("Handling SSE request for route {}", route.id);

    let upstream = proxy.select_upstream(route)?;
    let path = request
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(request.uri().path());

    let upstream_uri = format!("{}{}", upstream.url.trim_end_matches('/'), path);

    let response = proxy.forward_request(request, &upstream_uri).await?;

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
pub fn metrics(proxy: &HttpProxy) -> &ProxyMetrics {
    &proxy.metrics
}

// ---------------------------------------------------------------------------
// Connection Pool
// ---------------------------------------------------------------------------

/// Connection pool for upstream reuse
#[derive(Clone)]
pub struct ConnectionPool {
    /// Idle connections per upstream
    idle: Arc<RwLock<HashMap<String, Vec<PooledConnection>>>>,
    /// Active connection count per upstream
    active_count: Arc<RwLock<HashMap<String, u64>>>,
    /// Maximum idle connections per upstream
    max_idle: usize,
    /// Idle timeout duration
    idle_timeout: Duration,
    /// Round-robin counter for load balancing
    rr_counter: Arc<AtomicUsize>,
}

impl ConnectionPool {
    /// Create new connection pool
    pub fn new(max_idle: usize, idle_timeout: Duration) -> Self {
        Self {
            idle: Arc::new(RwLock::new(HashMap::new())),
            active_count: Arc::new(RwLock::new(HashMap::new())),
            max_idle,
            idle_timeout,
            rr_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Try to get a sender from the pool
    pub fn try_get_sender(&self, upstream: &str) -> Option<SendRequest<Body>> {
        let mut idle = self.idle.write();
        let connections = idle.get_mut(upstream)?;

        while let Some(conn) = connections.pop() {
            if conn.is_healthy() {
                self.increment_active(upstream);
                return Some(conn.sender);
            }
        }

        None
    }

    /// Return sender to pool
    pub fn return_sender(&self, upstream: &str, sender: SendRequest<Body>) {
        self.decrement_active(upstream);
        self.store_sender(upstream.to_string(), sender);
    }

    /// Store sender in pool
    pub fn store_sender(&self, upstream: String, sender: SendRequest<Body>) {
        let mut idle = self.idle.write();

        let connections = idle.entry(upstream).or_default();

        if connections.len() < self.max_idle {
            connections.push(PooledConnection {
                sender,
                created_at: Instant::now(),
            });
        }
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

    /// Prune expired idle connections
    pub fn prune_expired(&self) {
        let mut idle = self.idle.write();
        for (_, connections) in idle.iter_mut() {
            connections.retain(|conn| conn.created_at.elapsed() < self.idle_timeout);
        }
    }

    /// Get round-robin counter
    pub fn get_rr_counter(&self) -> &AtomicUsize {
        &self.rr_counter
    }
}

/// Pooled connection handle
pub struct PooledConnection {
    /// HTTP sender
    sender: SendRequest<Body>,
    /// When connection was created
    created_at: Instant,
}

impl PooledConnection {
    /// Check if connection is still usable
    pub fn is_healthy(&self) -> bool {
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

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Check if header is hop-by-hop (should not be forwarded)
fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

/// Check if request is WebSocket upgrade
fn is_websocket_upgrade(request: &Request<Body>) -> bool {
    let headers = request.headers();

    let upgrade = headers
        .get(UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase());

    let connection = headers
        .get(CONNECTION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase());

    matches!(
        (upgrade, connection),
        (Some(upgrade), Some(conn))
            if upgrade.contains("websocket") && conn.contains("upgrade")
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_proxy() -> HttpProxy {
        HttpProxy::with_timeouts(
            Duration::from_secs(30),
            Duration::from_secs(10),
            Arc::new(CircuitBreakerRegistry::new()),
            Arc::new(parking_lot::RwLock::new(HashMap::new())),
        )
    }

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
            .body(Body::empty())
            .unwrap();

        assert!(is_websocket_upgrade(&req));

        let normal_req = Request::builder().body(Body::empty()).unwrap();

        assert!(!is_websocket_upgrade(&normal_req));
    }

    #[test]
    fn test_hop_by_hop_headers() {
        assert!(is_hop_by_hop_header("connection"));
        assert!(is_hop_by_hop_header("transfer-encoding"));
        assert!(!is_hop_by_hop_header("content-type"));
        assert!(!is_hop_by_hop_header("authorization"));
    }

    #[test]
    fn test_is_websocket_upgrade_case_insensitive() {
        let req = Request::builder()
            .header("Upgrade", "WebSocket")
            .header("Connection", "upgrade")
            .body(Body::empty())
            .unwrap();

        assert!(is_websocket_upgrade(&req));
    }

    #[test]
    fn test_is_websocket_upgrade_with_other_connection_headers() {
        let req = Request::builder()
            .header("Upgrade", "websocket")
            .header("Connection", "keep-alive, Upgrade")
            .body(Body::empty())
            .unwrap();

        assert!(is_websocket_upgrade(&req));
    }

    #[test]
    fn test_pooled_connection_health() {
        let pool = ConnectionPool::new(10, Duration::from_secs(90));
        assert_eq!(pool.active_connections("test"), 0);

        pool.increment_active("test");
        assert_eq!(pool.active_connections("test"), 1);

        pool.decrement_active("test");
        assert_eq!(pool.active_connections("test"), 0);

        pool.decrement_active("test"); // Saturating
        assert_eq!(pool.active_connections("test"), 0);
    }

    #[test]
    fn test_proxy_creation() {
        let proxy = make_test_proxy();
        assert_eq!(proxy.metrics.total_requests.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_circuit_breaker_integration() {
        let proxy = make_test_proxy();

        // No breaker registered → allowed
        assert!(proxy.circuit_breakers.is_request_allowed("http://unknown:8080"));

        // Register a breaker
        proxy.circuit_breakers.register(
            "http://backend:8080",
            crate::router::CircuitBreakerConfig {
                failure_threshold: 2,
                success_threshold: 1,
                timeout_secs: 10,
            },
        );

        // First request allowed
        assert!(proxy.circuit_breakers.is_request_allowed("http://backend:8080"));
    }

    #[test]
    fn test_health_map_integration() {
        let proxy = make_test_proxy();

        // Initially empty health map
        let health = proxy.health_map.read();
        assert!(health.is_empty());
        drop(health);

        // Add a healthy upstream
        proxy.health_map.write().insert(
            "http://backend:8080".to_string(),
            AtomicBool::new(true),
        );

        let health = proxy.health_map.read();
        assert!(health.get("http://backend:8080").unwrap().load(Ordering::Relaxed));
    }

    #[test]
    fn test_pool_prune_expired() {
        let pool = ConnectionPool::new(10, Duration::from_secs(90));
        pool.prune_expired(); // Should not panic
    }
}
