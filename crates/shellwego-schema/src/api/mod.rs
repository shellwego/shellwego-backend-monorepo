//! API types for ShellWeGo
//!
//! This module contains all types related to API request and response
//! handling, including query parameters, request bodies, and response structures.

pub mod apps;
pub mod pagination;
pub mod responses;

// Re-export commonly used types at module level
pub use apps::{DeployRequest, DeployStrategy, ListAppsQuery, ListNodesQuery, ScaleRequest};
pub use pagination::{Cursor, PaginatedResponse, PaginationParams};
pub use responses::{
    ApiResponse, ComponentHealth, ErrorResponse, HealthResponse, ServiceStatus,
};
