//! Instance action and VM state types

use serde::{Serialize, Deserialize};

/// Instance action information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct InstanceActionInfo {
    /// Action type.
    pub action_type: ActionType,
}

/// Action type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum ActionType {
    FlushMetrics,
    InstanceStart,
    SendCtrlAltDel,
}

/// VM state descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Vm {
    /// VM state.
    pub state: VmState,
}

/// VM state enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum VmState {
    Paused,
    Resumed,
}
