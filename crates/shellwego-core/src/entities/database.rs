//! Managed database entity definitions.
//!
//! Lifecycle management for Postgres, MySQL, Redis, etc.

use crate::prelude::*;
#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;
#[cfg(feature = "orm")]
use sea_query::IdenStatic;

pub type DatabaseId = Uuid;

/// Database engine types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum DatabaseEngine {
    #[strum(serialize = "postgres")]
    Postgres,
    #[strum(serialize = "mysql")]
    Mysql,
    #[strum(serialize = "redis")]
    Redis,
    #[strum(serialize = "mongodb")]
    Mongodb,
    #[strum(serialize = "clickhouse")]
    Clickhouse,
}

/// Database operational status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum DatabaseStatus {
    Provisioning,
    Ready,
    Scaling,
    BackingUp,
    Restoring,
    Deleting,
    Error,
}

/// Connection information (metadata only, secrets in separate vault)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromQueryResult))]
pub struct DatabaseEndpoint {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
}

/// High Availability configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromQueryResult))]
pub struct HighAvailability {
    pub enabled: bool,
    pub replicas: u32,
    pub synchronous: bool,
}

/// Backup configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromQueryResult))]
pub struct DatabaseBackupConfig {
    pub enabled: bool,
    pub retention_days: u32,
    pub schedule: String, // Cron expression
}

/// Database entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "databases"))]
pub struct Database {
    #[cfg_attr(feature = "orm", sea_orm(primary_key, auto_increment = false))]
    pub id: DatabaseId,
    pub name: String,
    pub engine: DatabaseEngine,
    pub version: String,
    pub status: DatabaseStatus,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub endpoint: DatabaseEndpoint,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub resources: super::app::ResourceSpec,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub ha: HighAvailability,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub backup_config: DatabaseBackupConfig,
    pub organization_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(feature = "orm")]
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[cfg(feature = "orm")]
impl ActiveModelBehavior for ActiveModel {}

/// Create database request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct CreateDatabaseRequest {
    pub name: String,
    pub engine: DatabaseEngine,
    pub version: String,
    pub resources: super::app::ResourceSpec,
    #[serde(default)]
    pub ha_enabled: bool,
}

/// Database backup metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct DatabaseBackup {
    pub id: Uuid,
    pub database_id: DatabaseId,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub status: String,
}
