use crate::quinn::common::*;
use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use uuid::Uuid;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

pub struct QuinnServer {
    endpoint: quinn::Endpoint,
}

impl QuinnServer {
    pub async fn new(config: QuicConfig) -> Result<Self> {
        let addr = config.addr.parse::<SocketAddr>().context("Invalid address")?;

        let (certs, key) = if let (Some(cert_path), Some(key_path)) = (config.cert_path, config.key_path) {
            let cert_file = std::fs::File::open(cert_path).context("Failed to open cert file")?;
            let mut reader = std::io::BufReader::new(cert_file);
            let certs = rustls_pemfile::certs(&mut reader)
                .collect::<Result<Vec<_>, _>>()
                .context("Failed to read certs")?;

            let key_file = std::fs::File::open(key_path).context("Failed to open key file")?;
            let mut reader = std::io::BufReader::new(key_file);
            let key = rustls_pemfile::private_key(&mut reader)
                .context("Failed to read key")?
                .ok_or_else(|| anyhow::anyhow!("No private key found"))?;

            (certs, key)
        } else {
            let (cert, key) = generate_self_signed_cert()?;
            (vec![CertificateDer::from(cert)], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)))
        };

        let mut tls_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .context("Failed to create TLS config")?;

        tls_config.alpn_protocols = vec![config.alpn_protocol];

        let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)
            .context("Failed to create QUIC crypto config")?;
        let mut quinn_config = quinn::ServerConfig::with_crypto(Arc::new(crypto));

        let mut transport_config = quinn::TransportConfig::default();
        transport_config.max_concurrent_bidi_streams(config.max_concurrent_streams.into());
        transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(config.keep_alive_interval)));
        quinn_config.transport_config(Arc::new(transport_config));

        let endpoint = quinn::Endpoint::server(quinn_config, addr).context("Failed to bind")?;

        Ok(Self {
            endpoint,
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
        self.endpoint.local_addr().unwrap_or("0.0.0.0:0".parse().unwrap())
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
        let (_send_stream, mut recv_stream) = self.connection.accept_bi().await.context("Failed to accept bi")?;
        let data = recv_stream.read_to_end(10 * 1024 * 1024).await.context("Read failed")?;
        postcard::from_bytes(&data).context("Deserialize failed")
    }

    pub async fn send(&self, message: &Message) -> Result<()> {
        let data = postcard::to_allocvec(message).context("Serialize failed")?;
        let (mut send_stream, _recv_stream) = self.connection.open_bi().await.context("Open bi failed")?;
        send_stream.write_all(&data).await.context("Write failed")?;
        send_stream.finish().context("Finish failed")?;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.connection.close_reason().is_none()
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
