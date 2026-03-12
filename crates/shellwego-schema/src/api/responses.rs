//! API response types
//!
//! Standard response structures for API endpoints.

use serde::{Deserialize, Serialize};

/// Standard API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct ApiResponse<T> {
    /// Response data
    pub data: T,
    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    /// Create a new API response
    pub fn new(data: T) -> Self {
        Self {
            data,
            message: None,
        }
    }

    /// Create an API response with a message
    pub fn with_message(data: T, message: &str) -> Self {
        Self {
            data,
            message: Some(message.to_string()),
        }
    }
}

impl<T: serde::Serialize> ApiResponse<T> {
    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Error response for API failures
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct ErrorResponse {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Optional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    /// Create a new error response
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
        }
    }

    /// Create a not found error
    pub fn not_found(resource: &str) -> Self {
        Self::new("NOT_FOUND", &format!("{} not found", resource))
    }

    /// Create a validation error
    pub fn validation(message: &str) -> Self {
        Self::new("VALIDATION_ERROR", message)
    }

    /// Create an unauthorized error
    pub fn unauthorized() -> Self {
        Self::new("UNAUTHORIZED", "Authentication required")
    }

    /// Create a forbidden error
    pub fn forbidden() -> Self {
        Self::new("FORBIDDEN", "Access denied")
    }

    /// Create an internal server error
    pub fn internal() -> Self {
        Self::new("INTERNAL_ERROR", "An internal error occurred")
    }

    /// Add details to the error
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl std::fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ErrorResponse {}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct HealthResponse {
    /// Service status
    pub status: ServiceStatus,
    /// Service version
    pub version: String,
    /// Optional components status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<ComponentHealth>>,
}

impl HealthResponse {
    /// Create a healthy response
    pub fn healthy(version: &str) -> Self {
        Self {
            status: ServiceStatus::Healthy,
            version: version.to_string(),
            components: None,
        }
    }

    /// Create an unhealthy response
    pub fn unhealthy(version: &str) -> Self {
        Self {
            status: ServiceStatus::Unhealthy,
            version: version.to_string(),
            components: None,
        }
    }

    /// Add component health
    pub fn with_component(mut self, name: &str, status: ServiceStatus, message: Option<&str>) -> Self {
        let component = ComponentHealth {
            name: name.to_string(),
            status,
            message: message.map(|s| s.to_string()),
        };
        self.components.get_or_insert_with(Vec::new).push(component);
        self
    }
}

/// Service health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum ServiceStatus {
    /// Service is healthy
    Healthy,
    /// Service is unhealthy
    Unhealthy,
    /// Service is degraded but functional
    Degraded,
}

impl Default for ServiceStatus {
    fn default() -> Self {
        Self::Healthy
    }
}

/// Component health status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct ComponentHealth {
    /// Component name
    pub name: String,
    /// Component status
    pub status: ServiceStatus,
    /// Optional status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_new() {
        let response = ApiResponse::new(42);
        assert_eq!(response.data, 42);
        assert!(response.message.is_none());
    }

    #[test]
    fn test_api_response_with_message() {
        let response = ApiResponse::with_message(42, "Success");
        assert_eq!(response.data, 42);
        assert_eq!(response.message, Some("Success".to_string()));
    }

    #[test]
    fn test_error_response_new() {
        let error = ErrorResponse::new("TEST_ERROR", "Test error message");
        assert_eq!(error.code, "TEST_ERROR");
        assert_eq!(error.message, "Test error message");
    }

    #[test]
    fn test_error_response_helpers() {
        let not_found = ErrorResponse::not_found("App");
        assert_eq!(not_found.code, "NOT_FOUND");

        let validation = ErrorResponse::validation("Invalid input");
        assert_eq!(validation.code, "VALIDATION_ERROR");

        let unauthorized = ErrorResponse::unauthorized();
        assert_eq!(unauthorized.code, "UNAUTHORIZED");
    }

    #[test]
    fn test_error_response_display() {
        let error = ErrorResponse::new("ERR", "Something went wrong");
        assert_eq!(format!("{}", error), "[ERR] Something went wrong");
    }

    #[test]
    fn test_health_response_healthy() {
        let health = HealthResponse::healthy("1.0.0");
        assert_eq!(health.status, ServiceStatus::Healthy);
        assert_eq!(health.version, "1.0.0");
    }

    #[test]
    fn test_health_response_with_component() {
        let health = HealthResponse::healthy("1.0.0")
            .with_component("database", ServiceStatus::Healthy, None)
            .with_component("cache", ServiceStatus::Degraded, Some("High latency"));

        assert!(health.components.is_some());
        let components = health.components.unwrap();
        assert_eq!(components.len(), 2);
    }
}
