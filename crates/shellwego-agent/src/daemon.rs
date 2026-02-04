//! Control plane communication
//! 
//! Heartbeats, state reporting, and command consumption.
//! The agent's link to the brain.

use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, debug, warn, error};
use reqwest::Client;

use shellwego_core::entities::node::{Node, NodeStatus, RegisterNodeRequest, NodeJoinResponse};

use crate::{AgentConfig, Capabilities};
use crate::vmm::VmmManager;

/// Daemon handles all control plane communication
#[derive(Clone)]
pub struct Daemon {
    config: AgentConfig,
    client: Client,
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
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
            
        let daemon = Self {
            config,
            client,
            node_id: Arc::new(tokio::sync::RwLock::new(None)),
            capabilities,
            vmm,
        };
        
        // Register with control plane if no node_id
        if daemon.config.node_id.is_none() {
            daemon.register().await?;
        } else {
            *daemon.node_id.write().await = daemon.config.node_id;
        }
        
        Ok(daemon)
    }

    /// Initial registration with control plane
    async fn register(&self) -> anyhow::Result<()> {
        info!("Registering with control plane...");
        
        let req = RegisterNodeRequest {
            hostname: gethostname::gethostname().to_string_lossy().to_string(),
            region: self.config.region.clone(),
            zone: self.config.zone.clone(),
            labels: self.config.labels.clone(),
            capabilities: shellwego_core::entities::node::NodeCapabilities {
                kvm: self.capabilities.kvm,
                nested_virtualization: self.capabilities.nested_virtualization,
                cpu_features: self.capabilities.cpu_features.clone(),
                gpu: false, // TODO
            },
        };
        
        let url = format!("{}/v1/nodes", self.config.control_plane_url);
        let resp = self.client
            .post(&url)
            .json(&req)
            .send()
            .await?;
            
        if !resp.status().is_success() {
            anyhow::bail!("Registration failed: {}", resp.status());
        }
        
        let join: NodeJoinResponse = resp.json().await?;
        *self.node_id.write().await = Some(join.node_id);
        
        info!("Registered as node {}", join.node_id);
        info!("Join token acquired (length: {})", join.join_token.len());
        
        // TODO: Persist node_id to disk for recovery
        
        Ok(())
    }

    /// Continuous heartbeat loop
    pub async fn heartbeat_loop(&self) -> anyhow::Result<()> {
        let mut ticker = interval(Duration::from_secs(30));
        
        loop {
            ticker.tick().await;
            
            let node_id = self.node_id.read().await;
            let Some(id) = *node_id else {
                warn!("No node_id, skipping heartbeat");
                continue;
            };
            drop(node_id);
            
            if let Err(e) = self.send_heartbeat(id).await {
                error!("Heartbeat failed: {}", e);
                // TODO: Exponential backoff, mark unhealthy after N failures
            }
        }
    }

    async fn send_heartbeat(&self, node_id: uuid::Uuid) -> anyhow::Result<()> {
        // Gather current state
        let running_vms = self.vmm.list_running().await?;
        let capacity_used = self.calculate_capacity_used().await?;
        
        let url = format!("{}/v1/nodes/{}/heartbeat", self.config.control_plane_url, node_id);
        
        let payload = serde_json::json!({
            "status": "ready",
            "running_apps": running_vms.len(),
            "microvm_used": running_vms.len(),
            "capacity": capacity_used,
            "timestamp": chrono::Utc::now(),
        });
        
        let resp = self.client
            .post(&url)
            .json(&payload)
            .send()
            .await?;
            
        if resp.status().as_u16() == 404 {
            // Node was deleted from CP, re-register
            warn!("Node not found in control plane, re-registering...");
            *self.node_id.write().await = None;
            self.register().await?;
            return Ok(());
        }
        
        resp.error_for_status()?;
        debug!("Heartbeat sent: {} VMs running", running_vms.len());
        
        Ok(())
    }

    async fn calculate_capacity_used(&self) -> anyhow::Result<serde_json::Value> {
        // TODO: Sum resources allocated to running microVMs
        // TODO: Include overhead per VM (Firecracker process, CNI, etc)
        
        Ok(serde_json::json!({
            "memory_used_gb": 0,
            "cpu_used": 0.0,
        }))
    }

    /// Consume commands from control plane (NATS or long-polling)
    pub async fn command_consumer(&self) -> anyhow::Result<()> {
        // TODO: Connect to NATS if available
        // TODO: Subscribe to "commands.{node_id}" subject
        // TODO: Fallback to long-polling /v1/nodes/{id}/commands
        
        // Placeholder: just sleep
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    /// Get state client for reconciler
    pub fn state_client(&self) -> StateClient {
        StateClient {
            client: self.client.clone(),
            base_url: self.config.control_plane_url.clone(),
            node_id: self.node_id.clone(),
        }
    }
}

/// Client for fetching desired state
#[derive(Clone)]
pub struct StateClient {
    client: Client,
    base_url: String,
    node_id: Arc<tokio::sync::RwLock<Option<uuid::Uuid>>>,
}

impl StateClient {
    /// Fetch desired state for this node
    pub async fn get_desired_state(&self) -> anyhow::Result<DesiredState> {
        let node_id = self.node_id.read().await;
        let Some(id) = *node_id else {
            anyhow::bail!("Not registered");
        };
        
        let url = format!("{}/v1/nodes/{}/state", self.base_url, id);
        let resp = self.client.get(&url).send().await?;
        
        if resp.status().is_success() {
            let state: DesiredState = resp.json().await?;
            Ok(state)
        } else {
            // Return empty state on error
            Ok(DesiredState::default())
        }
    }
}

/// Desired state from control plane
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct DesiredState {
    pub apps: Vec<DesiredApp>,
    pub volumes: Vec<DesiredVolume>,
    // TODO: Add network policies
    // TODO: Add secrets to inject
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DesiredApp {
    pub app_id: uuid::Uuid,
    pub image: String,
    pub command: Option<Vec<String>>,
    pub memory_mb: u64,
    pub cpu_shares: u64,
    pub env: std::collections::HashMap<String, String>,
    pub volumes: Vec<VolumeMount>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VolumeMount {
    pub volume_id: uuid::Uuid,
    pub mount_path: String,
    pub device: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DesiredVolume {
    pub volume_id: uuid::Uuid,
    pub dataset: String,
    pub snapshot: Option<String>,
}