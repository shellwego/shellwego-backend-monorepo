//! Edge proxy and load balancer
//!
//! Traefik replacement written in Rust for lower latency.
//! Handles HTTP/HTTPS routing, TLS termination, ACME certificate provisioning,
//! WebSocket proxying, and load balancing.

pub mod access_log;
pub mod circuit_breaker;
pub mod config_watcher;
pub mod health;
pub mod proxy;
pub mod retry;
pub mod router;
pub mod tls;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Body, Request, Response, StatusCode};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

pub use proxy::{ConnectionPool, HttpProxy, ProxyMetrics, RequestContext};
pub use router::{ConfigSource, Matcher, Middleware, RequestInfo, Route, Router, Upstream};
pub use tls::{AcmeConfig, Certificate, CertificateManager, CertificateResolver};

/// Edge proxy server
#[allow(dead_code)]
pub struct EdgeProxy {
    /// Router for request matching
    router: Arc<RwLock<Router>>,
    /// TLS certificate manager
    tls_manager: Option<Arc<tls::CertificateManager>>,
    /// Certificate resolver for SNI
    cert_resolver: Option<Arc<tls::CertificateResolver>>,
    /// HTTP proxy handler
    proxy: proxy::HttpProxy,
    /// Configuration
    config: EdgeConfig,
    /// Proxy statistics
    stats: ProxyStats,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
    /// Access logger (if enabled)
    access_logger: Option<Arc<access_log::AccessLogger>>,
    /// Health checker handle
    health_checker: Option<Arc<health::HealthChecker>>,
    /// Circuit breaker registry
    circuit_breakers: Arc<circuit_breaker::CircuitBreakerRegistry>,
    /// Shared health state map (upstream URL -> healthy flag)
    health_map: Arc<parking_lot::RwLock<HashMap<String, AtomicBool>>>,
    /// Config file watcher (kept alive for the duration of the proxy)
    _config_watcher: Option<notify::RecommendedWatcher>,
}

/// Edge configuration
#[derive(Debug, Clone)]
pub struct EdgeConfig {
    /// HTTP bind address (for redirects and ACME)
    pub http_bind: Option<String>,
    /// HTTPS bind address
    pub https_bind: Option<String>,
    /// TLS configuration
    pub tls: Option<TlsConfig>,
    /// Routes
    pub routes: Vec<Route>,
    /// Middleware configuration
    pub middleware: Vec<Middleware>,
    /// Connection timeout in seconds
    pub connect_timeout_secs: u64,
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
    /// Idle connection timeout in seconds
    pub idle_timeout_secs: u64,
    /// Maximum connections per upstream
    pub max_connections_per_upstream: usize,
    /// Enable access logging
    pub access_logging: bool,
    /// Access log file path (None = stdout)
    pub access_log_path: Option<String>,
    /// Access log format
    pub access_log_format: Option<access_log::AccessLogFormat>,
    /// Path to config file for hot-reload (None = no file watching)
    pub config_file_path: Option<String>,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            http_bind: Some("0.0.0.0:80".to_string()),
            https_bind: Some("0.0.0.0:443".to_string()),
            tls: None,
            routes: Vec::new(),
            middleware: Vec::new(),
            connect_timeout_secs: 10,
            request_timeout_secs: 30,
            idle_timeout_secs: 90,
            max_connections_per_upstream: 100,
            access_logging: true,
            access_log_path: None,
            access_log_format: None,
            config_file_path: None,
        }
    }
}

/// TLS configuration
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Certificate resolver: "file" or "acme"
    pub cert_resolver: String,
    /// ACME configuration
    pub acme: Option<AcmeConfig>,
    /// Default certificate file (for file resolver)
    pub cert_file: Option<String>,
    /// Default key file (for file resolver)
    pub key_file: Option<String>,
}

/// Server handle for graceful shutdown
pub struct ServerHandle {
    /// Shutdown channel
    shutdown_tx: broadcast::Sender<()>,
    /// Server address
    pub addr: SocketAddr,
}

impl ServerHandle {
    /// Graceful shutdown
    pub async fn shutdown(self) -> Result<(), EdgeError> {
        info!("Initiating graceful shutdown for {}", self.addr);
        let _ = self.shutdown_tx.send(());
        Ok(())
    }
}

/// Proxy statistics
#[derive(Debug)]
pub struct ProxyStats {
    /// Total requests processed
    pub total_requests: AtomicU64,
    /// Active connections
    pub active_connections: AtomicU64,
    /// Requests per second (rolling average)
    pub requests_per_second: AtomicU64,
    /// Average latency in microseconds
    pub avg_latency_us: AtomicU64,
    /// Error count
    pub errors: AtomicU64,
    /// Start time
    pub start_time: std::time::Instant,
}

impl Default for ProxyStats {
    fn default() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            requests_per_second: AtomicU64::new(0),
            avg_latency_us: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            start_time: std::time::Instant::now(),
        }
    }
}

impl EdgeProxy {
    /// Create proxy from configuration
    pub async fn new(config: EdgeConfig) -> Result<Self, EdgeError> {
        info!("Initializing EdgeProxy");

        // Initialize router
        let mut router = Router::new();
        for route in &config.routes {
            router.add_route(route.clone())?;
        }
        let router = Arc::new(RwLock::new(router));

        // Initialize TLS manager and resolver if configured
        let (tls_manager, cert_resolver) = if let Some(ref tls_config) = config.tls {
            if tls_config.cert_resolver == "acme" {
                if let Some(ref acme) = tls_config.acme {
                    let cert_config = tls::CertConfig {
                        storage: tls::CertStorage::Memory,
                        acme: Some(tls::AcmeConfigDto {
                            directory_url: acme.directory_url.clone(),
                            contact_email: acme.contact_email.clone(),
                            challenge_type: acme.challenge_type.clone(),
                        }),
                    };
                    let manager =
                        Arc::new(tls::CertificateManager::new(&cert_config).await?);
                    let resolver = Arc::new(tls::CertificateResolver::new(manager.clone()));

                    // Start background renewal worker
                    manager.clone().start_renewal_worker();

                    (Some(manager), Some(resolver))
                } else {
                    (None, None)
                }
            } else {
                // File-based TLS — create manager without ACME
                let cert_config = tls::CertConfig {
                    storage: tls::CertStorage::Memory,
                    acme: None,
                };
                let manager = Arc::new(tls::CertificateManager::new(&cert_config).await?);
                let resolver = Arc::new(tls::CertificateResolver::new(manager.clone()));

                (Some(manager), Some(resolver))
            }
        } else {
            (None, None)
        };

        // Create circuit breaker registry
        let circuit_breakers = Arc::new(circuit_breaker::CircuitBreakerRegistry::new());

        // Register circuit breakers for all upstreams that have CB configs
        for route in &config.routes {
            for upstream in &route.upstreams {
                if let Some(ref cb_config) = upstream.circuit_breaker {
                    circuit_breakers.register(&upstream.url, cb_config.clone());
                }
            }
        }

        // Build health map from routes
        let router_read = router.read().await;
        let health_map = Arc::new(parking_lot::RwLock::new(
            health::build_health_map(&router_read),
        ));
        drop(router_read);

        // Create and start health checker if any upstream has health check config
        let has_health_checks = config.routes.iter().any(|route| {
            route
                .upstreams
                .iter()
                .any(|u| u.health_check.is_some())
        });

        let health_checker = if has_health_checks {
            let checker = Arc::new(health::HealthChecker::new(
                router.clone(),
                Arc::clone(&health_map),
            ));
            checker.clone().start();
            info!("Health checker started");
            Some(checker)
        } else {
            None
        };

        // Create HTTP proxy
        let proxy = proxy::HttpProxy::with_timeouts(
            Duration::from_secs(config.request_timeout_secs),
            Duration::from_secs(config.connect_timeout_secs),
            Arc::clone(&circuit_breakers),
            Arc::clone(&health_map),
        );

        // Create access logger
        let access_logger = if config.access_logging {
            match &config.access_log_path {
                Some(path) => Some(Arc::new(
                    access_log::AccessLogger::file(
                        path,
                        config
                            .access_log_format
                            .unwrap_or(access_log::AccessLogFormat::Combined),
                    )
                    .await?,
                )),
                None => Some(Arc::new(access_log::AccessLogger::stdout(
                    config
                        .access_log_format
                        .unwrap_or(access_log::AccessLogFormat::Combined),
                ))),
            }
        } else {
            None
        };

        if access_logger.is_some() {
            info!("Access logging enabled");
        }

        // Create shutdown channel
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        // Start config file watcher if path is provided
        let mut config_watcher: Option<notify::RecommendedWatcher> = None;
        if let Some(ref config_path) = config.config_file_path {
            match config_watcher::watch_config_file(config_path, router.clone()) {
                Ok(watcher) => {
                    info!("Config file watcher started for: {}", config_path);
                    config_watcher = Some(watcher);
                }
                Err(e) => {
                    warn!("Failed to start config file watcher: {}", e);
                }
            }
        }

        // Spawn connection pool pruning task
        let pool = proxy.pool();
        let idle_timeout = Duration::from_secs(config.idle_timeout_secs);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                pool.prune_expired();
            }
        });

        info!(
            "EdgeProxy initialized with {} routes, TLS={}, access_logging={}, health_checker={}, config_watcher={}",
            config.routes.len(),
            tls_manager.is_some(),
            access_logger.is_some(),
            health_checker.is_some(),
            config_watcher.is_some(),
        );

        Ok(Self {
            router,
            tls_manager,
            cert_resolver,
            proxy,
            config,
            stats: ProxyStats::default(),
            shutdown_tx,
            access_logger,
            health_checker,
            circuit_breakers,
            health_map,
            _config_watcher: config_watcher,
        })
    }

    /// Start listening on HTTP port (redirects to HTTPS + ACME challenge handler)
    pub async fn serve_http(&self, addr: &str) -> Result<ServerHandle, EdgeError> {
        let addr: SocketAddr = addr
            .parse()
            .map_err(|e| EdgeError::ConfigError(format!("Invalid address: {}", e)))?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| EdgeError::Io(e))?;

        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let router = self.router.clone();
        let tls_manager = self.tls_manager.clone();
        let cert_resolver = self.cert_resolver.clone();
        let proxy = self.proxy.clone();
        let stats = Arc::new(self.stats.clone());

        info!("HTTP server listening on {}", addr);

        let handle = ServerHandle {
            shutdown_tx: self.shutdown_tx.clone(),
            addr,
        };

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("HTTP server shutting down");
                        break;
                    }

                    result = listener.accept() => {
                        match result {
                            Ok((stream, _peer_addr)) => {
                                let router = router.clone();
                                let tls_manager = tls_manager.clone();
                                let cert_resolver = cert_resolver.clone();
                                let proxy = proxy.clone();
                                let stats = stats.clone();

                                tokio::spawn(async move {
                                    stats.active_connections.fetch_add(1, Ordering::Relaxed);
                                    stats.total_requests.fetch_add(1, Ordering::Relaxed);

                                    let stats_for_decrement = stats.clone();
                                    let service = service_fn(move |req: Request<Body>| {
                                        let _router = router.clone();
                                        let tls_manager = tls_manager.clone();
                                        let _cert_resolver = cert_resolver.clone();
                                        let _proxy = proxy.clone();
                                        async move {
                                            // Check if this is an ACME HTTP-01 challenge request
                                            if let Some(tls_mgr) = tls_manager.as_ref() {
                                                if let Some(key_auth) = handle_acme_challenge(&req, tls_mgr) {
                                                    return Ok(key_auth);
                                                }
                                            }

                                            // Redirect to HTTPS
                                            let host = req.headers()
                                                .get("host")
                                                .and_then(|h| h.to_str().ok())
                                                .unwrap_or("localhost");

                                            let https_url = format!("https://{}{}", host, req.uri());

                                            Response::builder()
                                                .status(StatusCode::MOVED_PERMANENTLY)
                                                .header("Location", https_url)
                                                .body(Body::empty())
                                                .map_err(|e| EdgeError::RoutingError(e.to_string()))
                                        }
                                    });

                                    let _ = http1::Builder::new()
                                        .serve_connection(stream, service)
                                        .await;

                                    stats_for_decrement.active_connections.fetch_sub(1, Ordering::Relaxed);
                                });
                            }
                            Err(e) => {
                                warn!("Failed to accept connection: {}", e);
                            }
                        }
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Start listening on HTTPS port with TLS termination.
    ///
    /// If a `CertificateResolver` is configured, incoming TCP connections are
    /// wrapped via `tokio_rustls::TlsAcceptor` before being handed to the
    /// HTTP service. The resolver performs SNI-based certificate selection.
    pub async fn serve_https(&self, addr: &str) -> Result<ServerHandle, EdgeError> {
        let addr: SocketAddr = addr
            .parse()
            .map_err(|e| EdgeError::ConfigError(format!("Invalid address: {}", e)))?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| EdgeError::Io(e))?;

        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let router = self.router.clone();
        let proxy = self.proxy.clone();
        let tls_manager = self.tls_manager.clone();
        let cert_resolver = self.cert_resolver.clone();
        let stats = Arc::new(self.stats.clone());
        let access_logger = self.access_logger.clone();

        // Build the TLS acceptor if we have a certificate resolver
        let tls_acceptor = if let Some(resolver) = &self.cert_resolver {
            let server_config = tls::build_rustls_server_config(resolver.clone());
            Some(Arc::new(tokio_rustls::TlsAcceptor::from(
                Arc::new(server_config),
            )))
        } else {
            None
        };

        info!(
            "HTTPS server listening on {} (TLS={})",
            addr,
            tls_acceptor.is_some()
        );

        let handle = ServerHandle {
            shutdown_tx: self.shutdown_tx.clone(),
            addr,
        };

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("HTTPS server shutting down");
                        break;
                    }

                    result = listener.accept() => {
                        match result {
                            Ok((stream, _peer_addr)) => {
                                let router = router.clone();
                                let proxy = proxy.clone();
                                let tls_manager = tls_manager.clone();
                                let _cert_resolver = cert_resolver.clone();
                                let tls_acceptor = tls_acceptor.clone();
                                let stats = stats.clone();
                                let access_logger = access_logger.clone();

                                tokio::spawn(async move {
                                    stats.active_connections.fetch_add(1, Ordering::Relaxed);
                                    stats.total_requests.fetch_add(1, Ordering::Relaxed);

                                    let stats_for_decrement = stats.clone();

                                    if let Some(acceptor) = tls_acceptor.as_ref() {
                                        // Wrap the TCP stream with TLS
                                        match acceptor.accept(stream).await {
                                            Ok(tls_stream) => {
                                                let service = service_fn(move |req: Request<Body>| {
                                                    let router = router.clone();
                                                    let tls_manager = tls_manager.clone();
                                                    let proxy = proxy.clone();
                                                    let stats = stats.clone();
                                                    let access_logger = access_logger.clone();
                                                    async move {
                                                        handle_https_request(req, router, tls_manager, proxy, stats, access_logger).await
                                                    }
                                                });

                                                let _ = http1::Builder::new()
                                                    .serve_connection(tls_stream, service)
                                                    .await;
                                            }
                                            Err(e) => {
                                                warn!("TLS handshake failed: {}", e);
                                                stats.errors.fetch_add(1, Ordering::Relaxed);
                                            }
                                        }
                                    } else {
                                        // No TLS configured — serve plain HTTP (development mode)
                                        let service = service_fn(move |req: Request<Body>| {
                                            let router = router.clone();
                                            let tls_manager = tls_manager.clone();
                                            let proxy = proxy.clone();
                                            let stats = stats.clone();
                                            let access_logger = access_logger.clone();
                                            async move {
                                                handle_https_request(req, router, tls_manager, proxy, stats, access_logger).await
                                            }
                                        });

                                        let _ = http1::Builder::new()
                                            .serve_connection(stream, service)
                                            .await;
                                    }

                                    stats_for_decrement.active_connections.fetch_sub(1, Ordering::Relaxed);
                                });
                            }
                            Err(e) => {
                                warn!("Failed to accept connection: {}", e);
                            }
                        }
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Request a certificate for the given domain via ACME.
    /// After obtaining the cert, warms the resolver cache.
    pub async fn provision_certificate(&self, domain: &str) -> Result<(), EdgeError> {
        let manager = self
            .tls_manager
            .as_ref()
            .ok_or(EdgeError::TlsError("TLS not configured".into()))?;

        manager.request_certificate(domain).await?;

        // Warm the resolver cache
        if let Some(resolver) = &self.cert_resolver {
            resolver.warm_cache(domain).await;
        }

        info!("Certificate provisioned for {}", domain);
        Ok(())
    }

    /// Import a pre-existing certificate for a domain.
    pub async fn import_certificate(
        &self,
        domain: &str,
        cert_pem: &str,
        key_pem: &str,
    ) -> Result<(), EdgeError> {
        let manager = self
            .tls_manager
            .as_ref()
            .ok_or(EdgeError::TlsError("TLS not configured".into()))?;

        manager
            .import_certificate(domain, cert_pem, key_pem)
            .await?;

        // Warm the resolver cache
        if let Some(resolver) = &self.cert_resolver {
            resolver.warm_cache(domain).await;
        }

        info!("Certificate imported for {}", domain);
        Ok(())
    }

    /// Reload configuration without dropping connections.
    pub async fn reload(&self, new_config: EdgeConfig) -> Result<(), EdgeError> {
        info!("Reloading EdgeProxy configuration");

        // Update routes
        let mut router = self.router.write().await;
        router.clear();

        for route in &new_config.routes {
            router.add_route(route.clone())?;
        }

        // Re-sync health map
        health::sync_health_map(&self.health_map, &router);

        // Register new circuit breakers
        for route in &new_config.routes {
            for upstream in &route.upstreams {
                if let Some(ref cb_config) = upstream.circuit_breaker {
                    self.circuit_breakers
                        .register(&upstream.url, cb_config.clone());
                }
            }
        }

        info!(
            "Configuration reloaded with {} routes",
            new_config.routes.len()
        );

        Ok(())
    }

    /// Get routing statistics
    pub async fn stats(&self) -> ProxyStats {
        let stats = self.stats.clone();
        let uptime = stats.start_time.elapsed().as_secs();

        if uptime > 0 {
            let rps = stats.total_requests.load(Ordering::Relaxed) / uptime;
            stats.requests_per_second.store(rps, Ordering::Relaxed);
        }

        stats
    }

    /// Add a route dynamically
    pub async fn add_route(&self, route: Route) -> Result<(), EdgeError> {
        let mut router = self.router.write().await;

        // Register circuit breakers for new upstreams
        for upstream in &route.upstreams {
            if let Some(ref cb_config) = upstream.circuit_breaker {
                self.circuit_breakers
                    .register(&upstream.url, cb_config.clone());
            }
            // Add to health map if health check is configured
            if upstream.health_check.is_some() {
                let mut map = self.health_map.write();
                map.entry(upstream.url.clone())
                    .or_insert_with(|| AtomicBool::new(upstream.healthy));
            }
        }

        router.add_route(route)
    }

    /// Remove a route dynamically
    pub async fn remove_route(&self, route_id: &str) -> Result<(), EdgeError> {
        let mut router = self.router.write().await;
        router.remove_route(route_id)
    }

    /// Get the circuit breaker registry (for monitoring/status)
    pub fn circuit_breakers(&self) -> &Arc<circuit_breaker::CircuitBreakerRegistry> {
        &self.circuit_breakers
    }

    /// Get the health map (for monitoring/status)
    pub fn health_map(&self) -> &Arc<parking_lot::RwLock<HashMap<String, AtomicBool>>> {
        &self.health_map
    }
}

// ---------------------------------------------------------------------------
// Request handling
// ---------------------------------------------------------------------------

/// Handle an HTTPS request (or plain HTTP if TLS is not configured).
async fn handle_https_request(
    req: Request<Body>,
    router: Arc<RwLock<Router>>,
    tls_manager: Option<Arc<tls::CertificateManager>>,
    proxy: HttpProxy,
    stats: Arc<ProxyStats>,
    access_logger: Option<Arc<access_log::AccessLogger>>,
) -> Result<Response<Body>, EdgeError> {
    let start = Instant::now();

    // Check for ACME HTTP-01 challenge requests (also served on HTTPS port)
    if let Some(ref manager) = tls_manager {
        if let Some(response) = handle_acme_challenge(&req, manager) {
            return Ok(response);
        }
    }

    // Match route
    let router_guard = router.read().await;
    let request_info = RequestInfo::from_request(&req);

    match router_guard.match_request(&request_info) {
        Some(route) => {
            let upstream_url = route
                .upstreams
                .first()
                .map(|u| u.url.clone())
                .unwrap_or_default();

            // Get the request ID if set by middleware
            let request_id = req
                .headers()
                .get("X-Request-Id")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let user_agent = req
                .headers()
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let client_ip = request_info.client_ip.clone();
            let method = request_info.method.clone();
            let path = request_info.path.clone();

            let result = proxy.handle_request(req, route).await;

            // Update stats
            let latency = start.elapsed().as_micros() as u64;
            stats.avg_latency_us.store(latency, Ordering::Relaxed);

            if result.is_err() {
                stats.errors.fetch_add(1, Ordering::Relaxed);
            }

            // Log access
            if let Some(ref logger) = access_logger {
                let entry = access_log::AccessLogEntry {
                    client_ip,
                    method,
                    path,
                    protocol: "HTTP/1.1".to_string(),
                    status: result
                        .as_ref()
                        .map(|r| r.status().as_u16())
                        .unwrap_or(502),
                    response_size: 0,
                    latency_ms: start.elapsed().as_millis() as u64,
                    user_agent,
                    request_id,
                    upstream_url: Some(upstream_url),
                };
                logger.log(&entry).await;
            }

            result
        }
        None => {
            // No matching route — still log the 404
            if let Some(ref logger) = access_logger {
                let entry = access_log::AccessLogEntry {
                    client_ip: request_info.client_ip,
                    method: request_info.method,
                    path: request_info.path,
                    protocol: "HTTP/1.1".to_string(),
                    status: 404,
                    response_size: 9, // "Not Found"
                    latency_ms: start.elapsed().as_millis() as u64,
                    user_agent: req
                        .headers()
                        .get("user-agent")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string()),
                    request_id: None,
                    upstream_url: None,
                };
                logger.log(&entry).await;
            }

            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap())
        }
    }
}

/// Check if the request is an ACME HTTP-01 challenge and, if so, return
/// the appropriate response with the key-authorization value.
///
/// The ACME challenge path is `/.well-known/acme-challenge/{token}`.
fn handle_acme_challenge(
    req: &Request<Body>,
    tls_manager: &tls::CertificateManager,
) -> Option<Response<Body>> {
    let path = req.uri().path();

    // Check for ACME challenge path
    if let Some(token) = path.strip_prefix("/.well-known/acme-challenge/") {
        let token = token.trim_matches('/');

        if let Some(key_auth) = tls_manager.get_challenge_token(token) {
            info!("Serving ACME challenge token: {}", token);
            return Some(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/plain")
                    .body(Body::from(key_auth))
                    .unwrap(),
            );
        }
    }

    None
}

impl Clone for ProxyStats {
    fn clone(&self) -> Self {
        Self {
            total_requests: AtomicU64::new(self.total_requests.load(Ordering::Relaxed)),
            active_connections: AtomicU64::new(self.active_connections.load(Ordering::Relaxed)),
            requests_per_second: AtomicU64::new(self.requests_per_second.load(Ordering::Relaxed)),
            avg_latency_us: AtomicU64::new(self.avg_latency_us.load(Ordering::Relaxed)),
            errors: AtomicU64::new(self.errors.load(Ordering::Relaxed)),
            start_time: self.start_time,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EdgeError {
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("Routing error: {0}")]
    RoutingError(String),

    #[error("Upstream unavailable: {0}")]
    Unavailable(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Certificate error: {0}")]
    CertError(#[from] crate::tls::CertError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_edge_proxy_creation() {
        let config = EdgeConfig::default();
        let proxy = EdgeProxy::new(config).await;
        assert!(proxy.is_ok());
    }

    #[tokio::test]
    async fn test_edge_proxy_stats() {
        let config = EdgeConfig::default();
        let proxy = EdgeProxy::new(config).await.unwrap();

        let stats = proxy.stats().await;
        assert_eq!(stats.total_requests.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_acme_challenge_path() {
        // Test that the ACME challenge path is correctly detected
        let req = Request::builder()
            .uri("/.well-known/acme-challenge/test-token")
            .body(Body::empty())
            .unwrap();

        let path = req.uri().path();
        assert!(path.starts_with("/.well-known/acme-challenge/"));
        let token = path.strip_prefix("/.well-known/acme-challenge/").unwrap();
        assert_eq!(token, "test-token");
    }

    #[test]
    fn test_acme_challenge_path_with_slash() {
        let req = Request::builder()
            .uri("/.well-known/acme-challenge/test-token/")
            .body(Body::empty())
            .unwrap();

        let path = req.uri().path();
        let token = path.strip_prefix("/.well-known/acme-challenge/").unwrap();
        let token = token.trim_matches('/');
        assert_eq!(token, "test-token");
    }

    #[tokio::test]
    async fn test_edge_proxy_with_tls_config() {
        let config = EdgeConfig {
            tls: Some(TlsConfig {
                cert_resolver: "acme".to_string(),
                acme: Some(AcmeConfig {
                    directory_url: "https://acme-v02.api.letsencrypt.org/directory".to_string(),
                    contact_email: "admin@example.com".to_string(),
                    challenge_type: "http01".to_string(),
                }),
                cert_file: None,
                key_file: None,
            }),
            ..Default::default()
        };

        let proxy = EdgeProxy::new(config).await;
        assert!(proxy.is_ok());
        let proxy = proxy.unwrap();
        assert!(proxy.tls_manager.is_some());
        assert!(proxy.cert_resolver.is_some());
    }
}
