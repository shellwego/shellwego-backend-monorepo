//! Business logic services
//! 
//! The actual work happens here. Handlers are just HTTP glue;
//! services contain the orchestration logic, state machines,
//! and external integrations.

pub mod deployment;
pub mod scheduler;

// TODO: Add app_service for CRUD operations
// TODO: Add node_service for capacity tracking
// TODO: Add volume_service for ZFS operations
// TODO: Add auth_service for identity management
// TODO: Add billing_service for metering (commercial)

use std::sync::Arc;
use crate::state::AppState;

/// Service context passed to all business logic
#[derive(Clone)]
pub struct ServiceContext {
    pub state: Arc<AppState>,
    // TODO: Add metrics client
    // TODO: Add tracer/span context
}

impl ServiceContext {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}