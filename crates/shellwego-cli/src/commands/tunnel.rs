//! Local tunnel to remote apps (like ngrok)
//!
//! Provides secure tunneling from local machine to remote applications
//! using WebSocket or QUIC transport.

use std::net::SocketAddr;
use std::sync::Arc;

use clap::Args;
use colored::Colorize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error, info, warn};

use crate::CliConfig;

/// Tunnel arguments
#[derive(Args)]
pub struct TunnelArgs {
    /// App ID to tunnel to
    app_id: uuid::Uuid,
    
    /// Local port to forward
    #[arg(short, long, default_value = "0")]
    local_port: u16,
    
    /// Remote port on app
    #[arg(short, long, default_value = "80")]
    remote_port: u16,
    
    /// Bind address
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
    
    /// Protocol to use
    #[arg(short = 'P', long, default_value = "ws")]
    protocol: Protocol,
    
    /// Show verbose output
    #[arg(short, long)]
    verbose: bool,
}

/// Tunnel protocol
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Protocol {
    /// WebSocket tunnel
    Ws,
    /// QUIC tunnel (faster, requires UDP)
    Quic,
}

/// Tunnel session
pub struct TunnelSession {
    /// App ID being tunneled
    pub app_id: uuid::Uuid,
    /// Local address
    pub local_addr: SocketAddr,
    /// Remote port
    pub remote_port: u16,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
}

impl TunnelSession {
    /// Signal shutdown
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// Handle tunnel command
pub async fn handle(args: TunnelArgs, config: &CliConfig) -> anyhow::Result<()> {
    let token = config.token.clone()
        .ok_or_else(|| anyhow::anyhow!("Not authenticated. Run `shellwego auth login`"))?;
    
    // Determine local port (0 means auto-assign)
    let bind_addr: SocketAddr = format!("{}:{}", args.bind, args.local_port)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid bind address: {}", e))?;
    
    // Bind local TCP listener
    let listener = TcpListener::bind(&bind_addr).await
        .map_err(|e| anyhow::anyhow!("Failed to bind {}: {}", bind_addr, e))?;
    
    let local_addr = listener.local_addr()?;
    
    println!(
        "{} Tunnel established",
        "✓".green().bold()
    );
    println!("  {} → app {}:{}", local_addr, args.app_id, args.remote_port);
    println!();
    println!("Press {} to disconnect", "Ctrl+C".yellow());
    
    // Create shutdown channel
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let shutdown_tx = Arc::new(shutdown_tx);
    
    // Build tunnel URL
    let tunnel_url = match args.protocol {
        Protocol::Ws => format!(
            "wss://{}/api/v1/tunnel/{}/{}?token={}",
            config.api_url.trim_start_matches("https://")
                .trim_start_matches("http://"),
            args.app_id,
            args.remote_port,
            token
        ),
        Protocol::Quic => {
            // QUIC uses a different endpoint
            format!(
                "{}:{}",
                config.api_url.trim_start_matches("https://")
                    .trim_start_matches("http://"),
                4433 // QUIC port
            )
        }
    };
    
    if args.verbose {
        println!("{} Tunnel URL: {}", "›".dimmed(), tunnel_url.split('?').next().unwrap_or(&tunnel_url));
    }
    
    info!("Starting tunnel session for app {}", args.app_id);
    
    // Accept connections
    let mut shutdown_rx = shutdown_tx.subscribe();
    
    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Shutting down tunnel");
                break;
            }
            
            result = listener.accept() => {
                match result {
                    Ok((stream, peer_addr)) => {
                        debug!("New connection from {}", peer_addr);
                        
                        let url = tunnel_url.clone();
                        let shutdown = shutdown_tx.clone();
                        
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, &url, peer_addr).await {
                                warn!("Connection error from {}: {}", peer_addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                    }
                }
            }
        }
    }
    
    println!();
    println!("{} Tunnel closed", "✓".green().bold());
    
    Ok(())
}

/// Handle a single TCP connection through the tunnel
async fn handle_connection(
    mut local_stream: TcpStream,
    tunnel_url: &str,
    peer_addr: SocketAddr,
) -> anyhow::Result<()> {
    debug!("Connecting to tunnel endpoint for {}", peer_addr);
    
    // Establish WebSocket connection to control plane
    let (ws_stream, _) = connect_async(tunnel_url).await
        .map_err(|e| anyhow::anyhow!("Failed to connect tunnel: {}", e))?;
    
    let (ws_sink, ws_stream) = ws_stream.split();
    
    debug!("WebSocket tunnel established for {}", peer_addr);
    
    // Wrap for concurrent read/write
    let ws_sink = Arc::new(Mutex::new(ws_sink));
    let mut ws_stream = ws_stream;
    
    // Split TCP stream
    let (mut tcp_read, mut tcp_write) = local_stream.split();
    
    // Forward data in both directions
    let mut tcp_buffer = vec![0u8; 8192];
    let mut ws_buffer = Vec::with_capacity(8192);
    
    loop {
        tokio::select! {
            // Read from TCP, send to WebSocket
            result = tcp_read.read(&mut tcp_buffer) => {
                match result {
                    Ok(0) => {
                        // TCP connection closed
                        debug!("TCP connection closed by {}", peer_addr);
                        break;
                    }
                    Ok(n) => {
                        debug!("TCP → WS: {} bytes from {}", n, peer_addr);
                        
                        let data = tcp_buffer[..n].to_vec();
                        let sink = ws_sink.clone();
                        
                        // Send to WebSocket
                        {
                            let mut sink = sink.lock().await;
                            sink.send(Message::Binary(data)).await?;
                        }
                    }
                    Err(e) => {
                        error!("TCP read error from {}: {}", peer_addr, e);
                        break;
                    }
                }
            }
            
            // Read from WebSocket, send to TCP
            result = ws_stream.next() => {
                match result {
                    Some(Ok(Message::Binary(data))) => {
                        debug!("WS → TCP: {} bytes to {}", data.len(), peer_addr);
                        tcp_write.write_all(&data).await?;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        // Respond to ping
                        let sink = ws_sink.clone();
                        let mut sink = sink.lock().await;
                        sink.send(Message::Pong(data)).await?;
                    }
                    Some(Ok(Message::Close(_))) => {
                        debug!("WebSocket closed by server for {}", peer_addr);
                        break;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Ignore pong
                    }
                    Some(Ok(msg)) => {
                        debug!("Unexpected WebSocket message: {:?}", msg);
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error for {}: {}", peer_addr, e);
                        break;
                    }
                    None => {
                        debug!("WebSocket stream ended for {}", peer_addr);
                        break;
                    }
                }
            }
        }
    }
    
    Ok(())
}

/// Request tunnel endpoint from control plane
async fn request_tunnel_endpoint(
    client: &reqwest::Client,
    base_url: &str,
    app_id: uuid::Uuid,
    port: u16,
    token: &str,
) -> anyhow::Result<TunnelEndpoint> {
    let url = format!("{}/api/v1/apps/{}/tunnel", base_url, app_id);
    
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "port": port,
        }))
        .send()
        .await?;
    
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Failed to request tunnel: {} - {}", status, body));
    }
    
    let endpoint: TunnelEndpoint = response.json().await?;
    Ok(endpoint)
}

/// Tunnel endpoint information
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TunnelEndpoint {
    /// WebSocket URL for tunnel
    pub websocket_url: String,
    /// Tunnel ID
    pub tunnel_id: String,
    /// Expires at
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// QUIC tunnel handler (for faster transport)
pub struct QuicTunnel {
    endpoint: quinn::Endpoint,
    connection: Option<quinn::Connection>,
}

impl QuicTunnel {
    /// Create new QUIC tunnel
    pub fn new() -> anyhow::Result<Self> {
        let mut certs = rustls::RootCertStore::empty();
        certs.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
            rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
                ta.subject,
                ta.spki,
                ta.name_constraints,
            )
        }));
        
        let client_crypto = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(certs)
            .with_no_client_auth();
        
        let client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
        
        let mut endpoint = quinn::Endpoint::client("[::]:0".parse()?)?;
        endpoint.set_default_client_config(client_config);
        
        Ok(Self {
            endpoint,
            connection: None,
        })
    }
    
    /// Connect to remote endpoint
    pub async fn connect(&mut self, host: &str, port: u16) -> anyhow::Result<()> {
        let addr = format!("{}:{}", host, port);
        let conn = self.endpoint.connect(addr.parse()?, host)?.await?;
        self.connection = Some(conn);
        Ok(())
    }
    
    /// Open a bidirectional stream
    pub async fn open_stream(&self) -> anyhow::Result<quinn::SendStream> {
        let conn = self.connection.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;
        
        let (send, _recv) = conn.open_bi().await?;
        Ok(send)
    }
}

// Helper trait extension for colored output
trait ColorizeExt: Sized {
    fn dimmed(self) -> colored::ColoredString;
}

impl ColorizeExt for &str {
    fn dimmed(self) -> colored::ColoredString {
        self.dimmed()
    }
}
