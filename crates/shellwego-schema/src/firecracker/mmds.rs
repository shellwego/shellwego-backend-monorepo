//! MMDS (Microvm Metadata Service) types

use serde::{Serialize, Deserialize};

/// MMDS configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MmdsConfig {
    /// List of network interface IDs for MMDS.
    pub network_interfaces: Vec<String>,
    /// MMDS version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<MmdsVersion>,
    /// IPv4 address for MMDS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4_address: Option<String>,
    /// IMDS compatibility mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imds_compat: Option<bool>,
}

/// MMDS version enumeration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum MmdsVersion {
    #[default]
    V1,
    V2,
}

/// MMDS contents (JSON object).
pub type MmdsContentsObject = serde_json::Value;
