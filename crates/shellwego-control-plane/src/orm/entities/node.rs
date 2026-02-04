//! Node entity using Sea-ORM
//!
//! Represents worker nodes that run Firecracker microVMs.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "nodes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub hostname: String,
    pub status: String, // TODO: Use NodeStatus enum with custom type
    pub region: String,
    pub zone: String,
    pub capacity: Json, // TODO: Use NodeCapacity with custom type
    pub capabilities: Json, // TODO: Use NodeCapabilities with custom type
    pub network: Json, // TODO: Use NodeNetwork with custom type
    pub labels: Json, // TODO: Use HashMap<String, String> with custom type
    pub running_apps: u32,
    pub microvm_capacity: u32,
    pub microvm_used: u32,
    pub kernel_version: String,
    pub firecracker_version: String,
    pub agent_version: String,
    pub last_seen: DateTime,
    pub created_at: DateTime,
    pub organization_id: Uuid,
    // TODO: Add join_token field (encrypted)
    // TODO: Add drain_started_at field
    // TODO: Add maintenance_reason field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to Organization
    // TODO: Define relation to AppInstance (has many)
    // TODO: Define relation to Volume (has many)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for status validation
    // TODO: Implement after_save hook for node registration event
    // TODO: Implement before_update hook for heartbeat tracking
}

// TODO: Implement conversion methods between ORM Model and core entity Node
// impl From<Model> for shellwego_core::entities::node::Node { ... }
// impl From<shellwego_core::entities::node::Node> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_ready(db: &DatabaseConnection) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_by_region(db: &DatabaseConnection, region: &str) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_stale_nodes(db: &DatabaseConnection, timeout_secs: i64) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn update_heartbeat(db: &DatabaseConnection, node_id: Uuid, capacity: NodeCapacity) -> Result<Self, DbErr> { ... }
// }
