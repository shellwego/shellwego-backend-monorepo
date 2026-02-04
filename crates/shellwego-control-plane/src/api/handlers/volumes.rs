//! Persistent volume handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use shellwego_core::entities::volume::{Volume, CreateVolumeRequest};
use crate::state::AppState;

pub async fn list_volumes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Volume>>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn create_volume(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateVolumeRequest>,
) -> Result<(StatusCode, Json<Volume>), StatusCode> {
    // TODO: Create ZFS dataset
    // TODO: Schedule on node with capacity
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn get_volume(
    State(state): State<Arc<AppState>>,
    Path(volume_id): Path<uuid::Uuid>,
) -> Result<Json<Volume>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn delete_volume(
    State(state): State<Arc<AppState>>,
    Path(volume_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Verify detached
    // TODO: Destroy ZFS dataset
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn attach_volume(
    State(state): State<Arc<AppState>>,
    Path(volume_id): Path<uuid::Uuid>,
    // TODO: App ID, mount path
) -> Result<StatusCode, StatusCode> {
    // TODO: Update volume record
    // TODO: Notify agent to mount
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn detach_volume(
    State(state): State<Arc<AppState>>,
    Path(volume_id): Path<uuid::Uuid>,
    // TODO: Force flag
) -> Result<StatusCode, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn create_snapshot(
    State(state): State<Arc<AppState>>,
    Path(volume_id): Path<uuid::Uuid>,
    // TODO: Name, description
) -> Result<StatusCode, StatusCode> {
    // TODO: ZFS snapshot
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn restore_snapshot(
    State(state): State<Arc<AppState>>,
    Path(volume_id): Path<uuid::Uuid>,
    // TODO: Snapshot ID, create_new_volume flag
) -> Result<StatusCode, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}