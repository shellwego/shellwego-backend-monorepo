use shellwego_network::{Message, QuicConfig, QuinnClient};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

use crate::metrics::MetricsCollector;
use crate::vmm::VmmManager;
use shellwego_schema::{DesiredApp, DesiredState};
use crate::{AgentConfig, Capabilities};

#[derive(Clone)]
pub struct Daemon {
    config: AgentConfig,
    quic: Arc<Mutex<QuinnClient>>,
    node_id: Arc<tokio::sync::RwLock<Option<uuid::Uuid>>>,
    capabilities: Capabilities,
    _vmm: VmmManager,
    state_cache: Arc<tokio::sync::RwLock<DesiredState>>,
    metrics: Arc<MetricsCollector>,
}

impl Daemon {
    pub async fn new(
        config: AgentConfig,
        capabilities: Capabilities,
        vmm: VmmManager,
        metrics: Arc<MetricsCollector>,
    ) -> anyhow::Result<Self> {
        let quic_conf = QuicConfig::default();
        let quic = Arc::new(Mutex::new(QuinnClient::new(quic_conf)));

        let daemon = Self {
            config,
            quic,
            node_id: Arc::new(tokio::sync::RwLock::new(None)),
            capabilities,
            _vmm: vmm,
            state_cache: Arc::new(tokio::sync::RwLock::new(DesiredState::default())),
            metrics,
        };

        daemon.register().await?;

        Ok(daemon)
    }

    async fn register(&self) -> anyhow::Result<()> {
        info!("Registering with control plane...");
        self.quic
            .lock()
            .await
            .connect(&self.config.control_plane_url)
            .await?;

        let msg = Message::Register {
            hostname: gethostname::gethostname().to_string_lossy().to_string(),
            capabilities: vec![
                format!("kvm={}", self.capabilities.kvm_available),
                format!("pvm={}", self.capabilities.pvm_available),
                format!("wasm={}", self.capabilities.wasm_available),
                format!("cores={}", self.capabilities.cpu_cores),
            ],
        };

        self.quic.lock().await.send(msg).await?;
        info!("Registration sent via QUIC");

        Ok(())
    }

    pub async fn heartbeat_loop(&self) -> anyhow::Result<()> {
        let mut ticker = interval(Duration::from_secs(15));

        loop {
            ticker.tick().await;

            let node_id = *self.node_id.read().await;
            let stats = self.metrics.get_snapshot();

            let msg = Message::Heartbeat {
                node_id: node_id.unwrap_or_default(),
                cpu_usage: stats.cpu_usage_percent as f64,
                memory_usage: (stats.memory_used as f64 / stats.memory_total as f64) * 100.0,
            };

            if let Err(e) = self.quic.lock().await.send(msg).await {
                error!("Heartbeat lost: {}. Reconnecting...", e);
                let _ = self
                    .quic
                    .lock()
                    .await
                    .connect(&self.config.control_plane_url)
                    .await;
            }
        }
    }

    pub async fn command_consumer(&self) -> anyhow::Result<()> {
        loop {
            match self.quic.lock().await.receive().await {
                Ok(Message::ScheduleApp {
                    app_id, image: _, ..
                }) => {
                    info!("CP ordered: Schedule app {}", app_id);
                    // In a full implementation, we would parse the full spec from the message
                    // For now, we update the cache to trigger the reconciler
                    let mut cache = self.state_cache.write().await;
                    if !cache.apps.iter().any(|a| a.app_id == app_id) {
                        // Create a skeleton desired app based on the message
                        // Real impl would have full details in the Message
                        cache.apps.push(DesiredApp {
                            app_id,
                            image: "default".to_string(), // Simplified
                            command: None,
                            memory_mb: 128,
                            cpu_shares: 1024,
                            env: Default::default(),
                            volumes: vec![],
                        });
                    }
                }
                Ok(Message::TerminateApp { app_id }) => {
                    info!("CP ordered: Stop app {}", app_id);
                    // Update cache to remove it
                    let mut cache = self.state_cache.write().await;
                    cache.apps.retain(|a| a.app_id != app_id);
                }
                Err(e) => {
                    warn!("Command stream interrupted: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                _ => {}
            }
        }
    }

    pub fn state_client(&self) -> StateClient {
        StateClient {
            _quic: self.quic.clone(),
            _node_id: self.node_id.clone(),
            state_cache: self.state_cache.clone(),
        }
    }
}

#[derive(Clone)]
pub struct StateClient {
    _quic: Arc<Mutex<QuinnClient>>,
    _node_id: Arc<tokio::sync::RwLock<Option<uuid::Uuid>>>,
    state_cache: Arc<tokio::sync::RwLock<DesiredState>>,
}

impl StateClient {
    pub async fn get_desired_state(&self) -> anyhow::Result<DesiredState> {
        let cache = self.state_cache.read().await;
        Ok(cache.clone())
    }
}

// DesiredState, DesiredApp, VolumeMount, DesiredVolume are now imported from shellwego_schema
// (re-exported via crate::* above)
