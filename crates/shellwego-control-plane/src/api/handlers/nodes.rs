//! Worker node management handlers

use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use shellwego_core::entities::node::{
    Node, RegisterNodeRequest, NodeJoinResponse,
};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListNodesQuery {
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

pub async fn list_nodes(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListNodesQuery>,
) -> Result<Json<Vec<Node>>, StatusCode> {
    // TODO: Filter by org, region, labels
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[utoipa::path(
    post,
    path = "/v1/nodes",
    request_body = RegisterNodeRequest,
    responses(
        (status = 201, description = "Node registered", body = NodeJoinResponse),
    ),
    tag = "Nodes"
)]
pub async fn register_node(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterNodeRequest>,
) -> Result<(StatusCode, Json<NodeJoinResponse>), StatusCode> {
    // TODO: Validate capabilities (KVM check)
    // TODO: Generate join token
    // TODO: Return install script with embedded token
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn get_node(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<uuid::Uuid>,
) -> Result<Json<Node>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn update_node(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<uuid::Uuid>,
    // TODO: Labels, status updates
) -> Result<Json<Node>, StatusCode> {
    // TODO: Handle draining state transition
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn delete_node(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<uuid::Uuid>,
    // TODO: migrate_apps flag
) -> Result<StatusCode, StatusCode> {
    // TODO: Verify no running apps or force migrate
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn drain_node(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<uuid::Uuid>,
    // TODO: Destination nodes, batch size
) -> Result<StatusCode, StatusCode> {
    // TODO: Set status to Draining
    // TODO: Trigger migration of all apps
    Err(StatusCode::NOT_IMPLEMENTED)
}