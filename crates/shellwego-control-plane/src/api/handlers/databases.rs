//! Managed database service handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use shellwego_core::entities::database::{Database, CreateDatabaseRequest, DatabaseBackup};
use crate::state::AppState;

pub async fn list_databases(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Database>>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn create_database(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDatabaseRequest>,
) -> Result<(StatusCode, Json<Database>), StatusCode> {
    // TODO: Provision on DB node or as sidecar
    // TODO: Generate credentials, store in secrets
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn get_database(
    State(state): State<Arc<AppState>>,
    Path(db_id): Path<uuid::Uuid>,
) -> Result<Json<Database>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn delete_database(
    State(state): State<Arc<AppState>>,
    Path(db_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Final snapshot option
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn get_connection_string(
    State(state): State<Arc<AppState>>,
    Path(db_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // TODO: Rotate credentials on each fetch or return cached
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn create_backup(
    State(state): State<Arc<AppState>>,
    Path(db_id): Path<uuid::Uuid>,
) -> Result<(StatusCode, Json<DatabaseBackup>), StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn restore_backup(
    State(state): State<Arc<AppState>>,
    Path(db_id): Path<uuid::Uuid>,
    // TODO: Backup ID or PIT timestamp
) -> Result<StatusCode, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}