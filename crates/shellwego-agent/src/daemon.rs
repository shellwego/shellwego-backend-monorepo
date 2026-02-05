use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, debug, warn, error};
use shellwego_network::{QuinnClient, Message, QuicConfig};
use zeroize::{Zeroize, ZeroizeOnDrop};
use uuid::Uuid;

use crate::{AgentConfig, Capabilities};
use crate::vmm::VmmManager;

#[derive(Clone)]
pub struct Daemon {
    pub config: AgentConfig,
    pub quic: Arc<QuinnClient>,
    pub node_id: Arc<tokio::sync::RwLock<Option<Uuid>>>,
    pub capabilities: Capabilities,
    pub vmm: VmmManager,
    pub desired_state: Arc<tokio::sync::RwLock<DesiredState>>,
}

impl Daemon {
    pub async fn new(
        config: AgentConfig,
        capabilities: Capabilities,
        vmm: VmmManager,
    ) -> anyhow::Result<Self> {
        let quic_conf = QuicConfig::default();
        let quic = Arc::new(QuinnClient::new(quic_conf));

        let daemon = Self {
            config,
            quic,
            node_id: Arc::new(tokio::sync::RwLock::new(None)),
            capabilities,
            vmm,
            desired_state: Arc::new(tokio::sync::RwLock::new(DesiredState::default())),
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
                    let mut state = self.desired_state.write().await;
                    if !state.apps.iter().any(|a| a.app_id == app_id) {
                        state.apps.push(DesiredApp {
                            app_id,
                            image,
                            command: None,
                            memory_mb: 256,
                            cpu_shares: 1024,
                            env: std::collections::HashMap::new(),
                            volumes: vec![],
                        });
                    }
                }
                Ok(Message::TerminateApp { app_id }) => {
                    info!("CP ordered: Stop app {}", app_id);
                    let mut state = self.desired_state.write().await;
                    state.apps.retain(|a| a.app_id != app_id);
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
            desired_state: self.desired_state.clone(),
        }
    }
}

#[derive(Clone)]
pub struct StateClient {
    #[allow(dead_code)]
    pub quic: Arc<QuinnClient>,
    #[allow(dead_code)]
    pub node_id: Arc<tokio::sync::RwLock<Option<Uuid>>>,
    pub desired_state: Arc<tokio::sync::RwLock<DesiredState>>,
}

impl StateClient {
    pub async fn get_desired_state(&self) -> anyhow::Result<DesiredState> {
        Ok(self.desired_state.read().await.clone())
    }
}

#[derive(Debug, Clone, Default)]
pub struct DesiredState {
    pub apps: Vec<DesiredApp>,
    pub volumes: Vec<DesiredVolume>,
}

#[derive(Debug, Clone, Zeroize, ZeroizeOnDrop)]
pub struct DesiredApp {
    #[zeroize(skip)]
    pub app_id: Uuid,
    pub image: String,
    pub command: Option<Vec<String>>,
    pub memory_mb: u64,
    pub cpu_shares: u64,
    #[zeroize(skip)]
    pub env: std::collections::HashMap<String, String>,
    #[zeroize(skip)]
    pub volumes: Vec<VolumeMount>,
}

#[derive(Debug, Clone)]
pub struct VolumeMount {
    pub volume_id: Uuid,
    pub mount_path: String,
    pub device: String,
}

#[derive(Debug, Clone)]
pub struct DesiredVolume {
    pub volume_id: Uuid,
    pub dataset: String,
    pub snapshot: Option<String>,
}
