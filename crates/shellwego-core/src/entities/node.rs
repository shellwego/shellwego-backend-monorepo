//! Worker Node entity definitions.
//!
//! Infrastructure that runs the actual Firecracker microVMs.

use crate::prelude::*;
#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;
#[cfg(feature = "orm")]
use sea_query::IdenStatic;

pub type NodeId = Uuid;

/// Node operational status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Registering,
    Ready,
    Draining,
    Maintenance,
    Offline,
    Decommissioned,
}

/// Hardware/OS capabilities
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromQueryResult))]
pub struct NodeCapabilities {
    pub kvm: bool,
    pub nested_virtualization: bool,
    pub cpu_features: Vec<String>,
    #[serde(default)]
    pub gpu: bool,
}

/// Resource capacity and current usage
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromQueryResult))]
pub struct NodeCapacity {
    pub cpu_cores: u32,
    pub memory_total_gb: u64,
    pub disk_total_gb: u64,
    pub memory_available_gb: u64,
    pub cpu_available: f64,
}

/// Node networking configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromQueryResult))]
pub struct NodeNetwork {
    pub internal_ip: String,
    #[serde(default)]
    pub public_ip: Option<String>,
    pub wireguard_pubkey: String,
    #[serde(default)]
    pub pod_cidr: Option<String>,
}

/// Worker Node entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "nodes"))]
pub struct Node {
    #[cfg_attr(feature = "orm", sea_orm(primary_key, auto_increment = false))]
    pub id: NodeId,
    pub hostname: String,
    pub status: NodeStatus,
    pub region: String,
    pub zone: String,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub capacity: NodeCapacity,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub capabilities: NodeCapabilities,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub network: NodeNetwork,
    #[serde(default)]
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub labels: std::collections::HashMap<String, String>,
    pub running_apps: u32,
    pub microvm_capacity: u32,
    pub microvm_used: u32,
    pub kernel_version: String,
    pub firecracker_version: String,
    pub agent_version: String,
    pub last_seen: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub organization_id: Uuid,
}

#[cfg(feature = "orm")]
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[cfg(feature = "orm")]
impl ActiveModelBehavior for ActiveModel {}

/// Request to register a new node
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct RegisterNodeRequest {
    pub hostname: String,
    pub region: String,
    pub zone: String,
    #[serde(default)]
    pub labels: std::collections::HashMap<String, String>,
    pub capabilities: NodeCapabilities,
}

/// Node join response with token
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct NodeJoinResponse {
    pub node_id: NodeId,
    pub join_token: String,
    pub install_script: String,
}
