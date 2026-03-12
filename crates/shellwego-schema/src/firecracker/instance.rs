//! Instance information and state types

use serde::{Serialize, Deserialize};

/// Describes MicroVM instance information.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct InstanceInfo {
    /// Application name.
    pub app_name: String,
    /// MicroVM / instance ID.
    pub id: String,
    /// The current detailed state of the Firecracker instance.
    pub state: InstanceState,
    /// MicroVM hypervisor build version.
    pub vmm_version: String,
}

/// Instance state enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum InstanceState {
    NotStarted,
    Running,
    Paused,
}

impl Default for InstanceState {
    fn default() -> Self {
        Self::NotStarted
    }
}

/// Firecracker version information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct FirecrackerVersion {
    /// Firecracker build version.
    pub firecracker_version: String,
}
