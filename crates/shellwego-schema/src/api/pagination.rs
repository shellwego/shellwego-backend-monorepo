//! Pagination types for API responses
//!
//! Standard pagination structures for list endpoints.

use serde::{Deserialize, Serialize};

/// Paginated response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct PaginatedResponse<T> {
    /// List of items
    pub items: Vec<T>,
    /// Cursor for the next page (if any)
    pub next_cursor: Option<String>,
    /// Whether there are more items
    pub has_more: bool,
    /// Total count (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_count: Option<u64>,
}

impl<T> PaginatedResponse<T> {
    /// Create a new paginated response
    pub fn new(items: Vec<T>, next_cursor: Option<String>, has_more: bool) -> Self {
        Self {
            items,
            next_cursor,
            has_more,
            total_count: None,
        }
    }

    /// Create an empty response
    pub fn empty() -> Self {
        Self {
            items: vec![],
            next_cursor: None,
            has_more: false,
            total_count: Some(0),
        }
    }

    /// Set the total count
    pub fn with_total_count(mut self, count: u64) -> Self {
        self.total_count = Some(count);
        self
    }

    /// Get the number of items
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if there are no items
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T: Clone> PaginatedResponse<T> {
    /// Map the items to a different type
    pub fn map<U, F: Fn(T) -> U>(self, f: F) -> PaginatedResponse<U> {
        PaginatedResponse {
            items: self.items.into_iter().map(f).collect(),
            next_cursor: self.next_cursor,
            has_more: self.has_more,
            total_count: self.total_count,
        }
    }
}

/// Pagination query parameters
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct PaginationParams {
    /// Maximum number of results to return
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Pagination cursor
    #[serde(default)]
    pub cursor: Option<String>,
}

fn default_limit() -> u32 {
    50
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            limit: default_limit(),
            cursor: None,
        }
    }
}

impl PaginationParams {
    /// Create pagination params with a specific limit
    pub fn with_limit(limit: u32) -> Self {
        Self {
            limit,
            cursor: None,
        }
    }

    /// Create pagination params with a cursor
    pub fn with_cursor(cursor: String) -> Self {
        Self {
            limit: default_limit(),
            cursor: Some(cursor),
        }
    }

    /// Set the cursor
    pub fn cursor(mut self, cursor: String) -> Self {
        self.cursor = Some(cursor);
        self
    }
}

/// Cursor for pagination
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Cursor {
    /// Offset in the result set
    pub offset: u64,
    /// Optional timestamp for time-based pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

impl Cursor {
    /// Create a new cursor with an offset
    pub fn new(offset: u64) -> Self {
        Self {
            offset,
            timestamp: None,
        }
    }

    /// Create a cursor with timestamp
    pub fn with_timestamp(offset: u64, timestamp: i64) -> Self {
        Self {
            offset,
            timestamp: Some(timestamp),
        }
    }

    /// Encode cursor to base64 string
    pub fn encode(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        base64_encode(&json)
    }

    /// Decode cursor from base64 string
    pub fn decode(s: &str) -> Result<Self, String> {
        let json = base64_decode(s)?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }
}

/// Simple base64 encoding (uses standard library)
fn base64_encode(input: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.encode(input.as_bytes())
}

/// Simple base64 decoding
fn base64_decode(input: &str) -> Result<String, String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD
        .decode(input.as_bytes())
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paginated_response_empty() {
        let response: PaginatedResponse<i32> = PaginatedResponse::empty();
        assert!(response.is_empty());
        assert!(!response.has_more);
        assert!(response.next_cursor.is_none());
    }

    #[test]
    fn test_paginated_response_new() {
        let response = PaginatedResponse::new(vec![1, 2, 3], Some("cursor".to_string()), true);
        assert_eq!(response.len(), 3);
        assert!(response.has_more);
        assert_eq!(response.next_cursor, Some("cursor".to_string()));
    }

    #[test]
    fn test_paginated_response_with_total_count() {
        let response = PaginatedResponse::new(vec![1, 2], None, false)
            .with_total_count(100);
        assert_eq!(response.total_count, Some(100));
    }

    #[test]
    fn test_pagination_params_default() {
        let params = PaginationParams::default();
        assert_eq!(params.limit, 50);
        assert!(params.cursor.is_none());
    }

    #[test]
    fn test_pagination_params_with_limit() {
        let params = PaginationParams::with_limit(100);
        assert_eq!(params.limit, 100);
        assert!(params.cursor.is_none());
    }

    #[test]
    fn test_cursor_encode_decode() {
        let cursor = Cursor::new(42);
        let encoded = cursor.encode();
        let decoded = Cursor::decode(&encoded).unwrap();
        assert_eq!(decoded.offset, 42);
    }

    #[test]
    fn test_cursor_with_timestamp() {
        let cursor = Cursor::with_timestamp(10, 1234567890);
        assert_eq!(cursor.offset, 10);
        assert_eq!(cursor.timestamp, Some(1234567890));
    }
}
