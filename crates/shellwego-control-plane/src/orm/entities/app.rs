//! App entity using Sea-ORM
//!
//! Represents deployable applications running in Firecracker microVMs.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use shellwego_core::entities::app::{AppStatus, ResourceSpec, EnvVar, DomainConfig, VolumeMount, HealthCheck, SourceSpec};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "apps")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: AppStatus,
    pub image: String,
    pub command: Option<Vec<String>>,
    pub resources: ResourceSpec,
    pub env: Vec<EnvVar>,
    pub domains: Vec<DomainConfig>,
    pub volumes: Vec<VolumeMount>,
    pub health_check: Option<HealthCheck>,
    pub source: SourceSpec,
    pub organization_id: Uuid,
    pub created_by: Uuid,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    BelongsTo,
    HasMany,
}

impl ActiveModelBehavior for ActiveModel {}
