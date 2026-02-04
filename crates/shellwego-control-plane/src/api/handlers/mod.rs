//! HTTP request handlers organized by resource
//! 
//! Each module handles one domain. Keep 'em skinny - delegate to services.

pub mod apps;
pub mod auth;
pub mod databases;
pub mod domains;
pub mod health;
pub mod nodes;
pub mod secrets;
pub mod volumes;

// TODO: Add common response types (ApiResponse<T>, ErrorResponse)
// TODO: Add auth extractor (CurrentUser)
// TODO: Add pagination helper
// TODO: Add validation error formatter