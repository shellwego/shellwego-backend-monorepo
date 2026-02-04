//! Node entity using Sea-ORM
//!
//! Represents worker nodes that run Firecracker microVMs.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use shellwego_core::entities::node::{NodeStatus, NodeCapacity, NodeCapabilities, NodeNetwork};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "nodes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub hostname: String,
    pub status: NodeStatus,
    pub region: String,
    pub zone: String,
    pub capacity: NodeCapacity,
    pub capabilities: NodeCapabilities,
    pub network: NodeNetwork,
    pub labels: std::collections::HashMap<String, String>,
    pub running_apps: u32,
    pub microvm_capacity: u32,
    pub microvm_used: u32,
    pub kernel_version: String,
    pub firecracker_version: String,
    pub agent_version: String,
    pub last_seen: DateTime,
    pub created_at: DateTime,
    pub organization_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    BelongsTo,
    HasMany,
}

impl ActiveModelBehavior for ActiveModel {}
