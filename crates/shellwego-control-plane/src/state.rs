//! Application state shared across handlers

use std::sync::Arc;
use dashmap::DashMap;
use tracing::info;
use uuid::Uuid;
use chrono::Utc;

use crate::config::Config;
use crate::orm::Database;
use crate::services::{BackupService, CertificateService, HealthCheckService, RateLimiter};
use crate::operators::{OperatorManager, OperatorConfig};
use crate::git::{BuildQueue, BuildQueueConfig};
use crate::kms::{KmsClient, KmsConfig};

// Re-export AgentConnection from schema for convenience
pub use shellwego_schema::network::AgentConnection;

/// Application state container
pub struct AppState {
    /// Configuration
    pub config: Config,
    /// Database pool
    pub db: Arc<Database>,
    /// Connected agents (node_id -> connection)
    pub agents: DashMap<Uuid, AgentConnection>,
    /// Backup service
    pub backup_service: Arc<BackupService>,
    /// Certificate service
    pub cert_service: Arc<CertificateService>,
    /// Health check service
    pub health_service: Arc<HealthCheckService>,
    /// Rate limiter
    pub rate_limiter: Arc<RateLimiter>,
    /// Database operator manager
    pub operator_manager: Arc<OperatorManager>,
    /// Build queue
    pub build_queue: Arc<BuildQueue>,
    /// KMS client
    pub kms_client: Arc<KmsClient>,
}

impl AppState {
    /// Create new application state
    pub async fn new(config: Config, db: Arc<Database>) -> anyhow::Result<Arc<Self>> {
        info!("Initializing application state");
        
        // Initialize backup service
        let backup_service = Arc::new(BackupService::new(Default::default()));
        
        // Initialize certificate service
        let cert_service = Arc::new(CertificateService::new(Default::default()));
        
        // Initialize health check service
        let health_service = Arc::new(HealthCheckService::new(Default::default()));
        
        // Initialize rate limiter
        let rate_limiter = Arc::new(RateLimiter::new(Default::default()));
        
        // Initialize operator manager
        let operator_config = OperatorConfig::default();
        let operator_manager = Arc::new(OperatorManager::new(&operator_config).await?);
        
        // Initialize build queue
        let build_queue_config = BuildQueueConfig::default();
        let build_queue = Arc::new(BuildQueue::new(build_queue_config));
        
        // Initialize KMS client
        let kms_config = KmsConfig::default();
        let kms_client = Arc::new(KmsClient::from_config(kms_config).await?);
        
        info!("All services initialized successfully");

        Ok(Arc::new(Self {
            config,
            db,
            agents: DashMap::new(),
            backup_service,
            cert_service,
            health_service,
            rate_limiter,
            operator_manager,
            build_queue,
            kms_client,
        }))
    }
    
    /// Register a new agent connection
    pub fn register_agent(&self, node_id: Uuid, hostname: String, region: String) {
        let conn = AgentConnection::new(node_id, hostname, region);
        self.agents.insert(node_id, conn);
        info!("Registered agent: {} ({})", node_id, hostname);
    }
    
    /// Deregister an agent
    pub fn deregister_agent(&self, node_id: &Uuid) {
        if let Some((_, conn)) = self.agents.remove(node_id) {
            info!("Deregistered agent: {} ({})", node_id, conn.hostname);
        }
    }
    
    /// Update agent heartbeat
    pub fn update_heartbeat(&self, node_id: &Uuid) {
        if let Some(mut conn) = self.agents.get_mut(node_id) {
            conn.last_heartbeat = Utc::now();
        }
    }
    
    /// Get connected agent count
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }
    
    /// List all connected agents
    pub fn list_agents(&self) -> Vec<AgentConnection> {
        self.agents.iter().map(|r| r.value().clone()).collect()
    }
}
