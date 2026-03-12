//! Edge proxy and load balancer
//!
//! Traefik replacement written in Rust for lower latency.
//! Handles HTTP/HTTPS routing, TLS termination, and load balancing.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

pub mod proxy;
pub mod router;
pub mod tls;

pub use proxy::{ConnectionPool, HttpProxy, ProxyMetrics, RequestContext};
pub use router::{ConfigSource, Matcher, Middleware, RequestInfo, Route, Router, Upstream};
pub use tls::{AcmeConfig, Certificate, CertificateManager};

/// Edge proxy server
pub struct EdgeProxy {
    /// Router for request matching
    router: Arc<RwLock<Router>>,
    /// TLS certificate manager
    tls_manager: Option<Arc<tls::CertificateManager>>,
    /// HTTP proxy handler
    proxy: proxy::HttpProxy,
    /// Configuration
    config: EdgeConfig,
    /// Proxy statistics
    stats: ProxyStats,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
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
#[derive(Debug, Default)]
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

        // Initialize TLS manager if configured
        let tls_manager = if let Some(ref tls_config) = config.tls {
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
                    Some(Arc::new(tls::CertificateManager::new(&cert_config).await?))
                } else {
                    None
                }
            } else {
                // File-based TLS would be initialized here
                None
            }
        } else {
            None
        };

        // Create HTTP proxy
        let proxy = proxy::HttpProxy::with_timeouts(
            Duration::from_secs(config.request_timeout_secs),
            Duration::from_secs(config.connect_timeout_secs),
        );

        // Create shutdown channel
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        info!("EdgeProxy initialized with {} routes", config.routes.len());

        Ok(Self {
            router,
            tls_manager,
            proxy,
            config,
            stats: ProxyStats::default(),
            shutdown_tx,
        })
    }

    /// Start listening on HTTP port (redirects to HTTPS)
    pub async fn serve_http(&self, addr: &str) -> Result<ServerHandle, EdgeError> {
        let addr: SocketAddr = addr
            .parse()
            .map_err(|e| EdgeError::ConfigError(format!("Invalid address: {}", e)))?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| EdgeError::Io(e))?;

        let shutdown_rx = self.shutdown_tx.subscribe();
        let router = self.router.clone();
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
                                let proxy = proxy.clone();
                                let stats = stats.clone();

                                tokio::spawn(async move {
                                    stats.active_connections.fetch_add(1, Ordering::Relaxed);
                                    stats.total_requests.fetch_add(1, Ordering::Relaxed);

                                    let io = hyper_util::rt::TokioIo::new(stream);

                                    let service = service_fn(move |req: Request<hyper::body::Body>| {
                                        let router = router.clone();
                                        let proxy = proxy.clone();
                                        async move {
                                            // Redirect to HTTPS
                                            let host = req.headers()
                                                .get("host")
                                                .and_then(|h| h.to_str().ok())
                                                .unwrap_or("localhost");

                                            let https_url = format!("https://{}{}", host, req.uri());

                                            Response::builder()
                                                .status(StatusCode::MOVED_PERMANENTLY)
                                                .header("Location", https_url)
                                                .body(hyper::body::Body::empty())
                                                .map_err(|e| EdgeError::RoutingError(e.to_string()))
                                        }
                                    });

                                    let _ = http1::Builder::new()
                                        .serve_connection(io, service)
                                        .await;

                                    stats.active_connections.fetch_sub(1, Ordering::Relaxed);
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

    /// Start listening on HTTPS port
    pub async fn serve_https(&self, addr: &str) -> Result<ServerHandle, EdgeError> {
        let addr: SocketAddr = addr
            .parse()
            .map_err(|e| EdgeError::ConfigError(format!("Invalid address: {}", e)))?;

        // TLS acceptor would be created here if configured
        // Currently placeholder for future implementation

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| EdgeError::Io(e))?;

        let shutdown_rx = self.shutdown_tx.subscribe();
        let router = self.router.clone();
        let proxy = self.proxy.clone();
        let stats = Arc::new(self.stats.clone());

        info!("HTTPS server listening on {}", addr);

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
                                let stats = stats.clone();

                                tokio::spawn(async move {
                                    stats.active_connections.fetch_add(1, Ordering::Relaxed);
                                    stats.total_requests.fetch_add(1, Ordering::Relaxed);

                                    let io = hyper_util::rt::TokioIo::new(stream);

                                    let service = service_fn(move |req: Request<hyper::body::Body>| {
                                        let router = router.clone();
                                        let proxy = proxy.clone();
                                        let stats = stats.clone();
                                        async move {
                                            let start = Instant::now();

                                            // Get route for request
                                            let router_guard = router.read().await;
                                            let request_info = RequestInfo::from_request(&req);

                                            match router_guard.match_request(&request_info) {
                                                Some(route) => {
                                                    let result = proxy.handle_request(req, route).await;

                                                    // Update stats
                                                    let latency = start.elapsed().as_micros() as u64;
                                                    stats.avg_latency_us.store(latency, Ordering::Relaxed);

                                                    if result.is_err() {
                                                        stats.errors.fetch_add(1, Ordering::Relaxed);
                                                    }

                                                    result.map_err(|e| {
                                                        hyper::Error::new(std::io::Error::new(
                                                            std::io::ErrorKind::Other,
                                                            e.to_string()
                                                        ))
                                                    })
                                                }
                                                None => {
                                                    // No matching route
                                                    Response::builder()
                                                        .status(StatusCode::NOT_FOUND)
                                                        .body(hyper::body::Body::from("Not Found"))
                                                        .map_err(|e| {
                                                            hyper::Error::new(std::io::Error::new(
                                                                std::io::ErrorKind::Other,
                                                                e.to_string()
                                                            ))
                                                        })
                                                }
                                            }
                                        }
                                    });

                                    let _ = http1::Builder::new()
                                        .serve_connection(io, service)
                                        .await;

                                    stats.active_connections.fetch_sub(1, Ordering::Relaxed);
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

    /// Reload configuration without dropping connections
    pub async fn reload(&self, new_config: EdgeConfig) -> Result<(), EdgeError> {
        info!("Reloading EdgeProxy configuration");

        // Update routes
        let mut router = self.router.write().await;
        router.clear();

        for route in &new_config.routes {
            router.add_route(route.clone())?;
        }

        info!(
            "Configuration reloaded with {} routes",
            new_config.routes.len()
        );

        Ok(())
    }

    /// Get routing statistics
    pub async fn stats(&self) -> ProxyStats {
        let mut stats = self.stats.clone();
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
        router.add_route(route)
    }

    /// Remove a route dynamically
    pub async fn remove_route(&self, route_id: &str) -> Result<(), EdgeError> {
        let mut router = self.router.write().await;
        router.remove_route(route_id)
    }
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
}
