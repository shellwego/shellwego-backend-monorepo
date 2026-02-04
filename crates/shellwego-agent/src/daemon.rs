use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, debug, warn, error};
use shellwego_network::{QuinnClient, Message, QuicConfig};

use crate::{AgentConfig, Capabilities};
use crate::vmm::VmmManager;

#[derive(Clone)]
pub struct Daemon {
    config: AgentConfig,
    quic: Arc<QuinnClient>,
    node_id: Arc<tokio::sync::RwLock<Option<uuid::Uuid>>>,
    capabilities: Capabilities,
    vmm: VmmManager,
}

impl Daemon {
    pub async fn new(
        config: AgentConfig,
        capabilities: Capabilities,
        vmm: VmmManager,
    ) -> anyhow::Result<Self> {
        let quic_conf = QuicConfig::default();
        let quic = Arc::new(QuinnClient::new(quic_conf));

        let mut daemon = Self {
            config,
            quic,
            node_id: Arc::new(tokio::sync::RwLock::new(None)),
            capabilities,
            vmm,
        };

        daemon.register().await?;

        Ok(daemon)
    }

    async fn register(&self) -> anyhow::Result<()> {
        info!("Registering with control plane...");
        self.quic.connect(&self.config.control_plane_url).await?;

        let msg = Message::Register {
            hostname: gethostname::gethostname().to_string_lossy().to_string(),
            capabilities: vec!["kvm".to_string()],
        };

        self.quic.send(msg).await?;
        info!("Registration sent via QUIC");

        Ok(())
    }

    pub async fn heartbeat_loop(&self) -> anyhow::Result<()> {
        let mut ticker = interval(Duration::from_secs(15));

        loop {
            ticker.tick().await;

            let node_id = *self.node_id.read().await;

            let msg = Message::Heartbeat {
                node_id: node_id.unwrap_or_default(),
                cpu_usage: 0.0,
                memory_usage: 0.0,
            };

            if let Err(e) = self.quic.send(msg).await {
                error!("Heartbeat lost: {}. Reconnecting...", e);
                let _ = self.quic.connect(&self.config.control_plane_url).await;
            }
        }
    }

    pub async fn command_consumer(&self) -> anyhow::Result<()> {
        loop {
            match self.quic.receive().await {
                Ok(Message::ScheduleApp { app_id, image, .. }) => {
                    info!("CP ordered: Start app {}", app_id);
                }
                Ok(Message::TerminateApp { app_id }) => {
                    info!("CP ordered: Stop app {}", app_id);
                    let _ = self.vmm.stop(app_id).await;
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
            quic: self.quic.clone(),
            node_id: self.node_id.clone(),
        }
    }
}

#[derive(Clone)]
pub struct StateClient {
    quic: Arc<QuinnClient>,
    node_id: Arc<tokio::sync::RwLock<Option<uuid::Uuid>>>,
}

impl StateClient {
    pub async fn get_desired_state(&self) -> anyhow::Result<DesiredState> {
        Ok(DesiredState::default())
    }
}

#[derive(Debug, Clone, Default)]
pub struct DesiredState {
    pub apps: Vec<DesiredApp>,
    pub volumes: Vec<DesiredVolume>,
}

#[derive(Debug, Clone)]
pub struct DesiredApp {
    pub app_id: uuid::Uuid,
    pub image: String,
    pub command: Option<Vec<String>>,
    pub memory_mb: u64,
    pub cpu_shares: u64,
    pub env: std::collections::HashMap<String, String>,
    pub volumes: Vec<VolumeMount>,
}

#[derive(Debug, Clone)]
pub struct VolumeMount {
    pub volume_id: uuid::Uuid,
    pub mount_path: String,
    pub device: String,
}

#[derive(Debug, Clone)]
pub struct DesiredVolume {
    pub volume_id: uuid::Uuid,
    pub dataset: String,
    pub snapshot: Option<String>,
}
