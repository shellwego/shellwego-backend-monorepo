//! Domain and TLS management handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use shellwego_core::entities::domain::{Domain, CreateDomainRequest, UploadCertificateRequest};
use crate::state::AppState;

pub async fn list_domains(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Domain>>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn create_domain(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDomainRequest>,
) -> Result<(StatusCode, Json<Domain>), StatusCode> {
    // TODO: Validate hostname DNS points to us
    // TODO: Queue ACME challenge if TLS enabled
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn get_domain(
    State(state): State<Arc<AppState>>,
    Path(domain_id): Path<uuid::Uuid>,
) -> Result<Json<Domain>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn delete_domain(
    State(state): State<Arc<AppState>>,
    Path(domain_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Cleanup certificates
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn upload_certificate(
    State(state): State<Arc<AppState>>,
    Path(domain_id): Path<uuid::Uuid>,
    Json(req): Json<UploadCertificateRequest>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Validate cert chain
    // TODO: Store encrypted
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn validate_dns(
    State(state): State<Arc<AppState>>,
    Path(domain_id): Path<uuid::Uuid>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Check DNS records match expected
    Err(StatusCode::NOT_IMPLEMENTED)
}