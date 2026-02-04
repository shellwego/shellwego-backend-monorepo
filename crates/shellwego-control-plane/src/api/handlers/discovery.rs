//! Service discovery endpoints

use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use std::collections::HashMap;

use crate::state::AppState;

/// Register service instance
pub async fn register(
    State(state): State<Arc<AppState>>,
    // TODO: Json body with service details
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate instance belongs to authenticated app
    // TODO: Store in registry
    // TODO: Broadcast to watchers
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Deregister instance
pub async fn deregister(
    State(state): State<Arc<AppState>>,
    Path((service_name, instance_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Remove from registry
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Discover instances
pub async fn discover(
    State(state): State<Arc<AppState>>,
    Path(service_name): Path<String>,
    Query(params): Query<DiscoveryQuery>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Query registry for healthy instances
    // TODO: Filter by metadata if requested
    // TODO: Return weighted list
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Health check callback from instance
pub async fn health_check(
    State(state): State<Arc<AppState>>,
    Path((service_name, instance_id)): Path<(String, String)>,
    // TODO: Json body with health status
) -> Result<StatusCode, StatusCode> {
    // TODO: Update instance health timestamp
    // TODO: Mark unhealthy if missed checks
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Watch for changes (SSE)
pub async fn watch(
    State(state): State<Arc<AppState>>,
    Path(service_name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Create SSE stream
    // TODO: Send current state
    // TODO: Push updates as they happen
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Query parameters
#[derive(Debug, serde::Deserialize)]
pub struct DiscoveryQuery {
    // TODO: Add metadata filters, health_only
}