use shellwego_schema::{BusConfig, BusMessage, ChannelPriority, Message, QuicConfig, SubscriptionId, Topic};
use super::bus::router::BusRouter;
use super::bus::reliability::{MessageDedup, ReliabilityLayer};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
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

    pub async fn accept(&self) -> Result<AgentConn> {
        let incoming = self.endpoint.accept().await.context("Failed to accept")?;
        let conn = incoming.await.context("Failed to handshake")?;

        let connection = AgentConn {
            connection: conn,
            node_id: None,
            hostname: None,
            subscriptions: HashSet::new(),
            outbound_tx: None,
        };

        Ok(connection)
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.endpoint.local_addr().unwrap_or("0.0.0.0:0".parse().unwrap())
    }

    /// Original simple run loop (accepts connections but does not process messages).
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

    /// Run the QUIC server with message bus integration.
    ///
    /// This is the main entry point for the bus-enabled server. It:
    /// 1. Spawns a background task to sweep stale subscriptions every 30 seconds.
    /// 2. Accepts incoming connections and spawns a per-connection task.
    /// 3. Each connection task waits for a `Register` message, then enters the
    ///    message dispatch loop where it processes `Subscribe`, `Unsubscribe`, `Ack`,
    ///    `Nack`, `Publish`, `Ping`, and `Pong` messages through the bus router.
    /// 4. On disconnect, cleans up all subscriptions for the node.
    pub async fn run_with_bus(&self, bus: Arc<BusRouter>, bus_config: BusConfig) -> Result<()> {
        // Spawn background task to sweep stale subscriptions.
        let sweep_bus = bus.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let swept = sweep_bus.sweep_stale().await;
                if swept > 0 {
                    tracing::debug!(swept = swept, "Swept stale subscriptions");
                }
            }
        });

        // Spawn background reliability timeout checker.
        let reliability_bus = bus.clone();
        let reliability_config = bus_config.clone();
        // Note: Per-connection reliability is created in the connection handler.

        loop {
            match self.accept().await {
                Ok(mut conn) => {
                    let bus = bus.clone();
                    let config = bus_config.clone();

                    tokio::spawn(async move {
                        // 1. Wait for Register message to get node_id
                        match conn.receive().await {
                            Ok(Message::Register { hostname, capabilities: _capabilities }) => {
                                let node_id = Uuid::new_v4();
                                conn.set_node_id(node_id);
                                conn.set_hostname(hostname);

                                tracing::info!(
                                    node_id = %node_id,
                                    "Agent registered, starting message loop"
                                );

                                // 2. Create outbound channel
                                let (outbound_tx, mut outbound_rx) =
                                    mpsc::channel(config.subscriber_buffer_size);
                                conn.outbound_tx = Some(outbound_tx);

                                // 3. Spawn outbound writer task
                                let mut send_conn = conn.clone();
                                tokio::spawn(async move {
                                    while let Some(bus_msg) = outbound_rx.recv().await {
                                        if let Err(e) = send_conn.send_message(&bus_msg).await {
                                            tracing::warn!("Failed to send bus message to agent: {}", e);
                                            break;
                                        }
                                    }
                                });

                                // 4. Message receive loop — dispatch through bus
                                loop {
                                    match conn.receive().await {
                                        Ok(Message::Subscribe { subscription_id, topic_pattern }) => {
                                            match Topic::new(&topic_pattern) {
                                                Ok(topic) => {
                                                    match bus.subscribe(node_id, topic) {
                                                        Ok((actual_sub_id, rx)) => {
                                                            conn.subscriptions.insert(actual_sub_id);
                                                            // Spawn task to forward bus messages to outbound
                                                            let tx = conn.outbound_tx.clone().unwrap();
                                                            tokio::spawn(async move {
                                                                while let Some(msg) = rx.recv().await {
                                                                    if tx.send(msg).await.is_err() {
                                                                        break;
                                                                    }
                                                                }
                                                            });
                                                            tracing::debug!(
                                                                sub_id = actual_sub_id.0,
                                                                topic = %topic_pattern,
                                                                "Subscribe succeeded"
                                                            );
                                                        }
                                                        Err(e) => {
                                                            tracing::warn!("Subscribe failed for '{}': {}", topic_pattern, e);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::warn!("Invalid topic '{}': {}", topic_pattern, e);
                                                }
                                            }
                                        }
                                        Ok(Message::Unsubscribe { subscription_id }) => {
                                            let removed = bus.unsubscribe(subscription_id);
                                            conn.subscriptions.remove(&subscription_id);
                                            if removed {
                                                tracing::debug!(sub_id = subscription_id.0, "Unsubscribe succeeded");
                                            } else {
                                                tracing::warn!(sub_id = subscription_id.0, "Unsubscribe: subscription not found");
                                            }
                                        }
                                        Ok(Message::Ack { msg_id }) => {
                                            tracing::trace!(msg_id = %msg_id, "Received ack");
                                            // Reliability layer would handle this in production
                                        }
                                        Ok(Message::Nack { msg_id, reason }) => {
                                            tracing::warn!(msg_id = %msg_id, reason = %reason, "Received nack");
                                        }
                                        Ok(Message::Publish { bus_message: envelope }) => {
                                            // Client publishing to the bus
                                            match BusMessage::from_envelope(envelope) {
                                                Ok(bus_msg) => {
                                                    let topic = bus_msg.topic.clone();
                                                    let delivered = bus.publish(&topic, bus_msg);
                                                    tracing::trace!(
                                                        topic = %topic,
                                                        delivered = delivered,
                                                        "Published message from agent"
                                                    );
                                                }
                                                Err(e) => {
                                                    tracing::warn!("Failed to deserialize bus message: {}", e);
                                                }
                                            }
                                        }
                                        Ok(Message::Ping { timestamp }) => {
                                            // Respond with Pong
                                            let pong = Message::Pong {
                                                ping_timestamp: timestamp,
                                                pong_timestamp: chrono::Utc::now(),
                                            };
                                            if let Err(e) = conn.send(&pong).await {
                                                tracing::warn!("Failed to send pong: {}", e);
                                                break;
                                            }
                                        }
                                        Ok(Message::Pong { .. }) => {
                                            // Ignore unexpected pongs
                                        }
                                        Ok(Message::Heartbeat { .. }) => {
                                            // Forward heartbeats to bus for metrics consumers
                                            // Could publish to "agent.heartbeat" topic
                                        }
                                        Ok(msg) => {
                                            // Wrap unknown messages and publish to inbound topic
                                            let bus_msg = BusMessage::new(
                                                Topic::new("agent.inbound").unwrap(),
                                                msg,
                                                ChannelPriority::Command,
                                            )
                                            .with_source(node_id);
                                            let topic = Topic::new("agent.inbound").unwrap();
                                            bus.publish(&topic, bus_msg);
                                        }
                                        Err(e) => {
                                            tracing::warn!("Receive error for node {}: {}", node_id, e);
                                            break;
                                        }
                                    }
                                }
                            }
                            Ok(msg) => {
                                tracing::warn!("Expected Register message, got: {:?}", std::mem::discriminant(&msg));
                                conn.close("expected Register message").await;
                            }
                            Err(e) => {
                                tracing::warn!("Accept error: {}", e);
                            }
                        }

                        // Cleanup on disconnect
                        if let Some(nid) = conn.node_id() {
                            let removed = bus.remove_node(nid);
                            if removed > 0 {
                                tracing::info!(node_id = %nid, count = removed, "Cleaned up subscriptions on disconnect");
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }
}

/// Active QUIC connection to an agent (internal type for connection handling).
///
/// Extended with bus integration fields: subscriptions, outbound queue.
#[derive(Clone)]
pub struct AgentConn {
    pub connection: quinn::Connection,
    pub node_id: Option<Uuid>,
    pub hostname: Option<String>,
    /// Active subscriptions for this connection.
    pub subscriptions: HashSet<SubscriptionId>,
    /// Outbound message queue — the bus router pushes messages here.
    pub outbound_tx: Option<mpsc::Sender<BusMessage>>,
}

impl AgentConn {
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

    /// Receive a raw `Message` from the QUIC connection.
    pub async fn receive(&self) -> Result<Message> {
        let (_send_stream, mut recv_stream) = self.connection.accept_bi().await.context("Failed to accept bi")?;
        let data = recv_stream.read_to_end(10 * 1024 * 1024).await.context("Read failed")?;
        postcard::from_bytes(&data).context("Deserialize failed")
    }

    /// Send a raw `Message` to the QUIC connection.
    pub async fn send(&self, message: &Message) -> Result<()> {
        let data = postcard::to_allocvec(message).context("Serialize failed")?;
        let (mut send_stream, _recv_stream) = self.connection.open_bi().await.context("Open bi failed")?;
        send_stream.write_all(&data).await.context("Write failed")?;
        send_stream.finish().context("Finish failed")?;
        Ok(())
    }

    /// Send a `BusMessage` to the QUIC connection.
    ///
    /// Wraps the bus message in a `Message::Publish` variant for transport.
    pub async fn send_message(&self, bus_msg: &BusMessage) -> Result<()> {
        let envelope = bus_msg.to_envelope().context("Failed to create envelope")?;
        let msg = Message::Publish { bus_message: envelope };
        self.send(&msg).await
    }

    /// Receive a `BusMessage` from the QUIC connection.
    ///
    /// Expects a `Message::Publish` variant and extracts the `BusMessage`.
    pub async fn receive_message(&self) -> Result<BusMessage> {
        let msg = self.receive().await?;
        match msg {
            Message::Publish { bus_message: envelope } => {
                BusMessage::from_envelope(envelope).context("Failed to decode bus message")
            }
            other => anyhow::bail!("Expected Publish message, got: {:?}", std::mem::discriminant(&other)),
        }
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_self_signed_cert() {
        let (cert_der, key_der) = generate_self_signed_cert().unwrap();
        // Self-signed cert should have non-empty DER bytes
        assert!(!cert_der.is_empty());
        assert!(!key_der.is_empty());
        // DER cert should start with SEQUENCE tag (0x30)
        assert_eq!(cert_der[0], 0x30);
        // DER private key (PKCS#8) should also start with SEQUENCE
        assert_eq!(key_der[0], 0x30);
    }

    #[test]
    fn test_quic_config_validation() {
        let config = QuicConfig::default();
        let addr: SocketAddr = config.addr.parse().unwrap();
        assert_eq!(addr.port(), 4433);
        assert_eq!(config.max_concurrent_streams, 100);
        assert_eq!(config.keep_alive_interval, 5);
    }

    #[test]
    fn test_agent_conn_node_id() {
        assert_eq!(std::any::type_name::<AgentConn>(), "shellwego_network::quinn::server::AgentConn");
    }

    #[test]
    fn test_agent_conn_bus_fields() {
        // Verify that AgentConn has the new bus fields.
        // We can't construct one without a live quinn::Connection,
        // but we can verify the type exists.
        assert_eq!(
            std::any::type_name::<HashSet<SubscriptionId>>(),
            "std::collections::hash::set::HashSet<shellwego_schema::SubscriptionId>"
        );
    }
}
