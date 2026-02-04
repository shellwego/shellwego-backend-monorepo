//! Server-sent events and WebSocket real-time updates

use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
};
use std::sync::Arc;

use crate::state::AppState;

/// SSE stream for app events
pub async fn app_events_sse(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<uuid::Uuid>,
) -> Result<Response, StatusCode> {
    // TODO: Validate access to app
    // TODO: Create SSE stream
    // TODO: Subscribe to app events via QUIC
    // TODO: Stream events as they arrive
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// WebSocket for interactive exec
pub async fn exec_websocket(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<uuid::Uuid>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    // TODO: Upgrade to WebSocket
    // TODO: Proxy to agent via QUIC
    // TODO: Handle stdin/stdout/stderr
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// WebSocket for log streaming
pub async fn logs_websocket(
    State(state): State<Arc<AppState>>,
    Path(app_id): Path<uuid::Uuid>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    // TODO: Upgrade to WebSocket
    // TODO: Stream historical logs first
    // TODO: Subscribe to new logs
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// WebSocket for build logs
pub async fn build_logs_websocket(
    State(state): State<Arc<AppState>>,
    Path(build_id): Path<uuid::Uuid>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    // TODO: Stream build log output
    Err(StatusCode::NOT_IMPLEMENTED)
}