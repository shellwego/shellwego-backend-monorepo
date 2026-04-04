//! API response types
//!
//! Re-exports types from shellwego-schema for consistency.
//! This module provides local extensions for backward compatibility.

use serde::{Deserialize, Serialize};

// Re-export from schema - single source of truth
pub use shellwego_schema::api::responses::ErrorResponse;

/// Generic list response with pagination (local extension for page-based pagination)
/// This complements the cursor-based PaginatedResponse from schema.
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

/// Pagination query parameters (local extension)
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }
