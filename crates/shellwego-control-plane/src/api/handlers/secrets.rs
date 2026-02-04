//! Secret management handlers (metadata only, no values exposed)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use shellwego_core::entities::secret::{Secret, CreateSecretRequest, RotateSecretRequest};
use crate::state::AppState;

pub async fn list_secrets(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Secret>>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn create_secret(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSecretRequest>,
) -> Result<(StatusCode, Json<Secret>), StatusCode> {
    // TODO: Encrypt with KMS
    // TODO: Store ciphertext, return metadata only
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn get_secret(
    State(state): State<Arc<AppState>>,
    Path(secret_id): Path<uuid::Uuid>,
) -> Result<Json<Secret>, StatusCode> {
    // TODO: Never return value here
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn delete_secret(
    State(state): State<Arc<AppState>>,
    Path(secret_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn rotate_secret(
    State(state): State<Arc<AppState>>,
    Path(secret_id): Path<uuid::Uuid>,
    Json(req): Json<RotateSecretRequest>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Create new version, mark old for deletion
    Err(StatusCode::NOT_IMPLEMENTED)
}