//! Metrics query and export endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::state::AppState;

/// Query metrics for app
pub async fn get_app_metrics(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<uuid::Uuid>,
    Query(params): Query<MetricsQuery>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate time range
    // TODO: Query Prometheus or internal store
    // TODO: Return metrics series
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Query node metrics
pub async fn get_node_metrics(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Return node resource usage
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Receive metrics from agent
pub async fn ingest_agent_metrics(
    State(state): State<Arc<AppState>>,
    // TODO: Json body with ResourceSnapshot
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate agent authentication
    // TODO: Store in time-series DB
    // TODO: Check thresholds, alert if needed
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Prometheus remote write endpoint
pub async fn remote_write(
    State(state): State<Arc<AppState>>,
    // TODO: Protobuf body
) -> Result<StatusCode, StatusCode> {
    // TODO: Parse Prometheus remote write format
    // TODO: Store samples
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Metrics query parameters
#[derive(Debug, serde::Deserialize)]
pub struct MetricsQuery {
    // TODO: Add start, end, step, metric_name
}