//! API response types

use serde::{Deserialize, Serialize};

/// Generic API response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ErrorResponse>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
    
    pub fn error(error: ErrorResponse) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

/// Generic error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>, code: u16) -> Self {
        Self {
            error: error.into(),
            code,
            details: None,
        }
    }
    
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
    
    pub fn not_found(resource: &str) -> Self {
        Self::new(format!("{} not found", resource), 404)
    }
    
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(message, 400)
    }
    
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(message, 500)
    }
    
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(message, 401)
    }
    
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(message, 403)
    }
    
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(message, 409)
    }
}

/// Generic list response with pagination
#[derive(Debug, Serialize, Deserialize)]
pub struct ListResponse<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
}

impl<T> ListResponse<T> {
    pub fn new(items: Vec<T>, total: u64, page: u32, per_page: u32) -> Self {
        Self { items, total, page, per_page }
    }
    
    pub fn empty() -> Self {
        Self {
            items: vec![],
            total: 0,
            page: 1,
            per_page: 20,
        }
    }
    
    pub fn from_items(items: Vec<T>) -> Self {
        let total = items.len() as u64;
        Self {
            items,
            total,
            page: 1,
            per_page: 20,
        }
    }
}

/// Pagination query parameters
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }
