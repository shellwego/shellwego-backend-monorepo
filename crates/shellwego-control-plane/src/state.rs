//! Application state shared across handlers

use std::sync::Arc;
use dashmap::DashMap;
use tracing::info;
use uuid::Uuid;
use chrono::Utc;

use crate::config::Config;
use crate::orm::Database;
use crate::services::{BackupService, CertificateService, HealthCheckService, RateLimiter};
use crate::services::agent_client::AgentClient;
use crate::services::scheduler::{Scheduler, SchedulerConfig};
use crate::services::deploy_pipeline::{DeployPipeline, DeployPipelineConfig};
use crate::services::guardian::{Guardian, GuardianConfig};
use crate::operators::{OperatorManager, OperatorConfig};
use crate::git::builder::{BuildQueue, BuildQueueConfig};
use crate::kms::{KmsClient, KmsConfig};
use crate::auth::AuthService;
use crate::audit::AuditService;

// Re-export AgentConnection from schema for convenience
pub use shellwego_schema::network::AgentConnection;

/// Application state container
pub struct AppState {
    /// Configuration
    pub config: Config,
    /// Database pool
    pub db: Arc<Database>,
    /// Connected agents (node_id -> connection)
    pub agents: Arc<DashMap<Uuid, AgentConnection>>,
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
    /// Authentication service
    pub auth_service: Arc<AuthService>,
    /// Audit logging service
    pub audit: Arc<AuditService>,
    /// Scheduler for app-to-node placement
    pub scheduler: Arc<Scheduler>,
    /// Deploy pipeline for driving deployments
    pub deploy_pipeline: Arc<DeployPipeline>,
    /// Guardian watchdog
    pub guardian: Arc<Guardian>,
    /// Agent client for sending commands
    pub agent_client: Arc<AgentClient>,
}

impl AppState {
    /// Create new application state
    pub async fn new(config: Config, db: Arc<Database>) -> anyhow::Result<Arc<Self>> {
        info!("Initializing application state");

        // Create agents as Arc<DashMap> first so we can share with services
        let agents = Arc::new(DashMap::new());

        // Initialize agent client (needs Arc<DashMap>)
        let agent_client = Arc::new(AgentClient::new(agents.clone()));

        // Initialize scheduler (needs Arc<Database> and Arc<DashMap>)
        let scheduler = Arc::new(Scheduler::new(
            config.scheduler.clone(),
            db.clone(),
            agents.clone(),
        ));

        // Initialize deploy pipeline
        let deploy_pipeline = Arc::new(DeployPipeline::new(
            config.deploy.clone(),
            db.clone(),
            agent_client.clone(),
            scheduler.clone(),
        ));

        // Initialize guardian
        let guardian = Arc::new(Guardian::new(
            config.guardian.clone(),
            db.clone(),
            agent_client.clone(),
            agents.clone(),
        ));

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

        // Initialize authentication service
        let auth_service = Arc::new(AuthService::new(config.jwt.clone()));

        // Initialize audit service
        let audit = Arc::new(AuditService::new(db.clone()));

        info!("All services initialized successfully");

        let state = Arc::new(Self {
            config,
            db,
            agents,
            backup_service,
            cert_service,
            health_service,
            rate_limiter,
            operator_manager,
            build_queue,
            kms_client,
            auth_service,
            audit,
            scheduler,
            deploy_pipeline,
            guardian,
            agent_client,
        });

        // Spawn the guardian background task
        let _guardian_handle = state.guardian.spawn(state.guardian.clone());

        Ok(state)
    }

    /// Register a new agent connection
    pub fn register_agent(&self, node_id: Uuid, hostname: String, region: String) {
        let conn = AgentConnection::new(node_id, hostname.clone(), region);
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
