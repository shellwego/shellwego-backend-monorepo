use crate::quinn::common::*;
use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use uuid::Uuid;

pub struct QuinnServer {
    endpoint: Arc<quinn::Endpoint>,
    addr: SocketAddr,
}

impl QuinnServer {
    pub async fn new(config: QuicConfig) -> Result<Self> {
        let addr = config.addr.parse::<SocketAddr>().context("Invalid address")?;

        let (cert, key) = if let (Some(cert_path), Some(key_path)) = (config.cert_path, config.key_path) {
            let cert = std::fs::read(cert_path).context("Failed to read cert")?;
            let key = std::fs::read(key_path).context("Failed to read key")?
        } else {
            generate_self_signed_cert()?
        };

        let tls_config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(
                vec![rustls::Certificate(cert)],
                rustls::PrivateKey(key),
            )
            .context("Failed to create TLS config")?;

        let quinn_config = quinn::ServerConfig::with_crypto(Arc::new(tls_config))
            .max_concurrent_streams(config.max_concurrent_streams.into())
            .keep_alive_interval(Some(std::time::Duration::from_secs(config.keep_alive_interval)));

        let mut endpoint_builder = quinn::Endpoint::builder();
        endpoint_builder.listen(quinn_config);

        let (endpoint, _) = endpoint_builder.bind(&addr).context("Failed to bind")?;

        Ok(Self {
            endpoint: Arc::new(endpoint),
            addr: endpoint.local_addr().unwrap_or(addr),
        })
    }

    pub async fn accept(&self) -> Result<AgentConnection> {
        let incoming = self.endpoint.accept().await.context("Failed to accept")?;
        let conn = incoming.await.context("Failed to handshake")?;

        let connection = AgentConnection {
            connection: conn,
            node_id: None,
            hostname: None,
        };

        Ok(connection)
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            match self.accept().await {
                Ok(conn) => {
                    tracing::info!("Agent connected from {}", conn.remote_addr());
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct AgentConnection {
    pub connection: quinn::Connection,
    pub node_id: Option<Uuid>,
    pub hostname: Option<String>,
}

impl AgentConnection {
    pub fn node_id(&self) -> Option<Uuid> {
        self.node_id
    }

    pub fn set_node_id(&mut self, id: Uuid) {
        self.node_id = Some(id);
    }

    pub fn set_hostname(&mut self, hostname: String) {
        self.hostname = Some(hostname);
    }

    pub fn hostname(&self) -> Option<&str> {
        self.hostname.as_deref()
    }

    pub fn remote_addr(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    pub async fn receive(&self) -> Result<Message> {
        let (mut send_stream, mut recv_stream) = self.connection.accept_bi().await.context("Failed to accept bi")?;
        let mut buf = Vec::new();
        recv_stream.read_to_end(&mut buf).await.context("Read failed")?;
        postcard::from_bytes(&buf).context("Deserialize failed")
    }

    pub async fn send(&self, message: &Message) -> Result<()> {
        let data = postcard::to_allocvec(message).context("Serialize failed")?;
        let (mut send_stream, _) = self.connection.open_bi().await.context("Open bi failed")?;
        send_stream.write_all(&data).await.context("Write failed")?;
        send_stream.finish().await.context("Finish failed")?;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.connection.state() == quinn::ConnectionState::Established
    }

    pub async fn close(&self, reason: &str) {
        self.connection.close(0u8.into(), reason.as_bytes());
    }
}

fn generate_self_signed_cert() -> Result<(Vec<u8>, Vec<u8>)> {
    let cert = rcgen::generate_simple_self_signed(vec!["shellwego".to_string()]).map_err(|e| anyhow::anyhow!("Cert gen failed: {}", e))?;
    let cert_der = cert.serialize_der().map_err(|e| anyhow::anyhow!("Cert serialize failed: {}", e))?;
    let key_der = cert.serialize_private_key_der();

    Ok((cert_der, key_der))
}
