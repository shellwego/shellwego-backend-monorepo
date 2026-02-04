use crate::quinn::common::*;
use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
pub struct QuinnClient {
    connection: Option<quinn::Connection>,
    config: QuicConfig,
    endpoint: Option<Arc<quinn::Endpoint>>,
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

        let mut tls_config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth();

        tls_config.alpn_protocols = vec![b"shellwego/1"];

        let endpoint = quinn::Endpoint::client("[::]:0".parse().unwrap()).context("Create endpoint")?;
        let endpoint = Arc::new(endpoint);

        let quinn_config = quinn::ClientConfig::with_tls(Arc::new(tls_config));

        let connecting = endpoint.connect_with(quinn_config, &addr, "control-plane").context("Connect")?;
        let connection = connecting.await.context("Handshake")?;

        self.connection = Some(connection);
        self.endpoint = Some(endpoint);

        Ok(())
    }

    pub async fn send(&self, message: Message) -> Result<()> {
        let connection = self.connection.as_ref().context("Not connected")?;
        let data = postcard::to_allocvec(&message).context("Serialize")?;
        let (mut send_stream, _) = connection.open_bi().await.context("Open stream")?;
        send_stream.write_all(&data).await.context("Write")?;
        send_stream.finish().await.context("Finish")?;
        Ok(())
    }

    pub async fn receive(&self) -> Result<Message> {
        let connection = self.connection.as_ref().context("Not connected")?;
        let (mut recv_stream, _) = connection.accept_bi().await.context("Accept stream")?;
        let mut buf = Vec::new();
        recv_stream.read_to_end(&mut buf).await.context("Read")?;
        postcard::from_bytes(&buf).context("Deserialize")
    }

    pub fn is_connected(&self) -> bool {
        self.connection.as_ref().map(|c| c.state() == quinn::ConnectionState::Established).unwrap_or(false)
    }

    pub async fn close(&self) {
        if let Some(connection) = &self.connection {
            connection.close(0u8.into(), b"close");
        }
    }
}
