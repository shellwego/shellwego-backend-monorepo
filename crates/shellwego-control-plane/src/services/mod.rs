//! Services layer for control plane
//!
//! Provides backup orchestration, certificate management, health checking,
//! rate limiting, scheduling, deploy pipeline, and guardian services.

pub mod agent_client;
pub mod backup;
pub mod certificate;
pub mod health_check;
pub mod rate_limiter;
pub mod scheduler;
pub mod deploy_pipeline;
pub mod guardian;

pub use agent_client::AgentClient;
pub use backup::BackupService;
pub use certificate::CertificateService;
pub use health_check::HealthCheckService;
pub use rate_limiter::RateLimiter;
pub use scheduler::{Scheduler, SchedulerConfig};
pub use deploy_pipeline::{DeployPipeline, DeployPipelineConfig};
pub use guardian::{Guardian, GuardianConfig};

use std::sync::Arc;
use crate::config::Config;
use crate::orm::Database;

/// Service context for sharing dependencies
pub struct ServiceContext {
    pub config: Config,
    pub database: Arc<Database>,
}

impl ServiceContext {
    pub fn new(config: Config, database: Arc<Database>) -> Self {
        Self { config, database }
    }
}
