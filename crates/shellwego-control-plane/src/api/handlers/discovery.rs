//! DNS-based service discovery API endpoints
//!
//! Provides HTTP API for DNS-based service registry using hickory-dns.

use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use std::collections::HashMap;

use crate::state::AppState;
use crate::services::discovery::ServiceInstance;

#[derive(Debug, serde::Deserialize)]
pub struct RegisterRequest {
    // TODO: Add service_name String
    // TODO: Add instance_id String
    // TODO: Add address String
    // TODO: Add port u16
    // TODO: Add metadata HashMap<String, String>
}

#[derive(Debug, serde::Deserialize)]
pub struct HealthUpdateRequest {
    // TODO: Add healthy bool
}

#[derive(Debug, serde::Deserialize)]
pub struct DiscoveryQuery {
    // TODO: Add healthy_only bool
    // TODO: Add metadata_filters HashMap<String, String>
    // TODO: Add zone String
}

/// Register service instance
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterRequest>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate request body
    // TODO: Parse address to SocketAddr
    // TODO: Call registry.register()
    // TODO: Publish DNS SRV record
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Deregister instance
pub async fn deregister(
    State(state): State<Arc<AppState>>,
    Path((service_name, instance_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Call registry.deregister()
    // TODO: Remove DNS SRV record
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Discover instances via DNS
pub async fn discover(
    State(state): State<Arc<AppState>>,
    Path(service_name): Path<String>,
    Query(params): Query<DiscoveryQuery>,
) -> Result<Json<Vec<ServiceInstance>>, StatusCode> {
    // TODO: Call registry.get_healthy() or get_all()
    // TODO: Filter by metadata if params.metadata_filters set
    // TODO: Filter by zone if params.zone set
    // TODO: Return as JSON
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// DNS SRV record lookup endpoint
pub async fn discover_dns(
    Path(service_name): Path<String>,
) -> Result<Json<Vec<DnsServiceInstance>>, StatusCode> {
    // TODO: Perform direct DNS SRV lookup
    // TODO: Parse SRV records to DnsServiceInstance
    // TODO: Return as JSON
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Health check callback from instance
pub async fn health_check(
    State(state): State<Arc<AppState>>,
    Path((service_name, instance_id)): Path<(String, String)>,
    Json(body): Json<HealthUpdateRequest>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Call registry.update_health()
    // TODO: Update DNS record if health changed
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Heartbeat from instance
pub async fn heartbeat(
    State(state): State<Arc<AppState>>,
    Path((service_name, instance_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Call registry.update_heartbeat()
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// List all registered services
pub async fn list_services(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    // TODO: Call registry.list_services()
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// DNS service instance response
#[derive(Debug, Clone, serde::Serialize)]
pub struct DnsServiceInstance {
    // TODO: Add host String
    // TODO: Add port u16
    // TODO: Add priority u16
    // TODO: Add weight u16
    // TODO: Add target String
}
