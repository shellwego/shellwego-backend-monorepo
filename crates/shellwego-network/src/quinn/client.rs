use crate::quinn::common::*;
use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
pub struct QuinnClient {
    connection: Option<quinn::Connection>,
    config: QuicConfig,
    endpoint: Option<quinn::Endpoint>,
}

impl QuinnClient {
    pub fn new(config: QuicConfig) -> Self {
        Self {
            connection: None,
            config,
            endpoint: None,
        }
    }

    pub async fn connect(&mut self, endpoint_url: &str) -> Result<()> {
        let addr: SocketAddr = endpoint_url.parse().context("Invalid endpoint URL")?;

        let root_store = rustls::RootCertStore::empty();
        // Should add real roots in production

        let mut tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        tls_config.alpn_protocols = vec![self.config.alpn_protocol.clone()];

        let endpoint = quinn::Endpoint::client("[::]:0".parse().unwrap()).context("Create endpoint")?;

        let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)
            .context("Failed to create QUIC crypto config")?;
        let quinn_config = quinn::ClientConfig::new(Arc::new(crypto));

        let connecting = endpoint.connect_with(quinn_config, addr, "control-plane").context("Connect")?;
        let connection = connecting.await.context("Handshake")?;

        self.connection = Some(connection);
        self.endpoint = Some(endpoint);

        Ok(())
    }

    pub async fn send(&self, message: Message) -> Result<()> {
        let connection = self.connection.as_ref().context("Not connected")?;
        let data = postcard::to_allocvec(&message).context("Serialize")?;
        let (mut send_stream, _recv_stream) = connection.open_bi().await.context("Open stream")?;
        send_stream.write_all(&data).await.context("Write")?;
        send_stream.finish().context("Finish")?;
        Ok(())
    }

    pub async fn receive(&self) -> Result<Message> {
        let connection = self.connection.as_ref().context("Not connected")?;
        let (_send_stream, mut recv_stream) = connection.accept_bi().await.context("Accept stream")?;
        let data = recv_stream.read_to_end(10 * 1024 * 1024).await.context("Read")?;
        postcard::from_bytes(&data).context("Deserialize")
    }

    pub fn is_connected(&self) -> bool {
        self.connection.as_ref().map(|c| c.close_reason().is_none()).unwrap_or(false)
    }

    pub async fn close(&self) {
        if let Some(connection) = &self.connection {
            connection.close(0u8.into(), b"close");
        }
    }
}
