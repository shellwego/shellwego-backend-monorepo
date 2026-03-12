//! API middleware

use axum::{
    body::Body,
    http::{Request, Response},
    middleware::Next,
};
use tracing::info;

/// Request logging middleware
pub async fn log_request(
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    
    info!("{} {}", method, path);
    
    next.run(req).await
}
