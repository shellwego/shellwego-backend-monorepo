//! API middleware
//!
//! Provides authentication, rate limiting, and request logging middleware.

use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    middleware::Next,
};
use tracing::{info, warn};
use std::sync::Arc;

use crate::auth::CurrentUser;
use crate::state::AppState;
use super::handlers::ErrorResponse;

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

/// Authentication middleware
///
/// Extracts Bearer token from Authorization header, validates it
/// using the AuthService, and injects `CurrentUser` into request extensions.
/// Returns 401 if no token, invalid token, expired token, or revoked token.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response<Body>, (StatusCode, Response<Body>)> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(header) => match header.strip_prefix("Bearer ") {
            Some(token) if !token.is_empty() => token,
            _ => {
                warn!("Missing or invalid Authorization header");
                return Err((
                    StatusCode::UNAUTHORIZED,
                    unauthorized_response("Missing or invalid Bearer token"),
                ));
            }
        },
        None => {
            warn!("No Authorization header");
            return Err((
                StatusCode::UNAUTHORIZED,
                unauthorized_response("Authentication required"),
            ));
        }
    };

    match state.auth_service.validate_access_token(token).await {
        Ok(current_user) => {
            info!("Authenticated user: {} (id: {})", current_user.username, current_user.user_id);
            req.extensions_mut().insert(current_user);
            Ok(next.run(req).await)
        }
        Err(e) => {
            warn!("Token validation failed: {}", e);
            let (status, message) = match e {
                crate::auth::AuthError::TokenExpired => {
                    (StatusCode::UNAUTHORIZED, "Token has expired".to_string())
                }
                crate::auth::AuthError::TokenRevoked => {
                    (StatusCode::UNAUTHORIZED, "Token has been revoked".to_string())
                }
                crate::auth::AuthError::InvalidToken(msg) => {
                    (StatusCode::UNAUTHORIZED, format!("Invalid token: {}", msg))
                }
                crate::auth::AuthError::UserNotFound => {
                    (StatusCode::UNAUTHORIZED, "User not found".to_string())
                }
                _ => (StatusCode::UNAUTHORIZED, "Authentication failed".to_string()),
            };
            Err((status, unauthorized_response(&message)))
        }
    }
}

/// Rate limiting middleware
///
/// Uses the existing RateLimiter service to enforce per-IP rate limits.
/// Returns 429 Too Many Requests when the limit is exceeded.
pub async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let key = req
        .headers()
        .get("X-Real-IP")
        .or_else(|| req.headers().get("X-Forwarded-For"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous")
        .to_string();

    let result = state.rate_limiter.check(&key).await;

    if !result.allowed {
        let retry_after = result.retry_after_ms.unwrap_or(1000);
        let body = serde_json::json!({
            "code": "RATE_LIMITED",
            "message": "Too many requests",
            "retry_after_ms": retry_after,
        })
        .to_string();

        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("Content-Type", "application/json")
            .header("Retry-After", (retry_after / 1000 + 1).to_string())
            .header("X-RateLimit-Remaining", "0")
            .body(Body::from(body))
            .unwrap();
    }

    next.run(req).await
}

/// Simple RBAC check function for use in handlers
///
/// Call this from any protected handler to verify the user has a specific permission.
///
/// # Example
/// ```ignore
/// pub async fn delete_app(
///     State(state): State<Arc<AppState>>,
///     Extension(current_user): Extension<CurrentUser>,
///     Path(app_id): Path<Uuid>,
/// ) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
///     check_permission(&current_user, "apps:delete")?;
///     // ... proceed with deletion
/// }
/// ```
pub fn check_permission(
    current_user: &CurrentUser,
    required: &str,
) -> Result<(), (StatusCode, ErrorResponse)> {
    if crate::auth::has_permission(&current_user.permissions, required) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            ErrorResponse::new(
                "FORBIDDEN",
                &format!("Insufficient permissions: '{}' required", required),
            ),
        ))
    }
}

/// Create an unauthorized JSON response body
fn unauthorized_response(message: &str) -> Response<Body> {
    let body = serde_json::json!({
        "code": "UNAUTHORIZED",
        "message": message,
    })
    .to_string();

    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap()
}
