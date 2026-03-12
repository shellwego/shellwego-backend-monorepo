//! Services layer for control plane
//!
//! Provides backup orchestration, certificate management, health checking,
//! and rate limiting services.

pub mod backup;
pub mod certificate;
pub mod health_check;
pub mod rate_limiter;

pub use backup::BackupService;
pub use certificate::CertificateService;
pub use health_check::HealthCheckService;
pub use rate_limiter::RateLimiter;

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
