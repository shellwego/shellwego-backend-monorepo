//! Firecracker API error type

use serde::{Serialize, Deserialize};

/// Firecracker API error.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Error {
    /// Error description.
    pub fault_message: String,
}
