//! Managed Database entity definitions.
//!
//! DBaaS: Postgres, MySQL, Redis, etc.

use crate::prelude::*;

#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;

pub type DatabaseId = Uuid;

/// Supported database engines
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum DatabaseEngine {
    Postgres,
    Mysql,
    Redis,
    Mongodb,
    Clickhouse,
}

/// Database operational status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DatabaseStatus {
    Creating,
    Available,
    BackingUp,
    Restoring,
    Maintenance,
    Upgrading,
    Deleting,
    Error,
}

/// Connection endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct DatabaseEndpoint {
    pub host: String,
    pub port: u16,
    pub username: String,
    // TODO: This should reference a secret, not expose value
    pub password: String,
    pub database: String,
    pub ssl_mode: String,
}

/// Resource allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct DatabaseResources {
    pub storage_gb: u64,
    pub memory_gb: u64,
    pub cpu_cores: f64,
}

/// Current usage stats
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct DatabaseUsage {
    pub storage_used_gb: u64,
    pub connections_active: u32,
    pub connections_max: u32,
    pub transactions_per_sec: f64,
}

/// High availability config
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct HighAvailability {
    pub enabled: bool,
    pub mode: String, // "synchronous", "asynchronous"
    pub replica_regions: Vec<String>,
    pub failover_enabled: bool,
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct DatabaseBackupConfig {
    pub enabled: bool,
    pub frequency: String,
    pub retention_days: u32,
    pub window_start: String, // "02:00"
    pub window_duration_hours: u32,
}

/// Database entity
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    pub resources: DatabaseResources,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub usage: DatabaseUsage,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub ha: HighAvailability,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub backup_config: DatabaseBackupConfig,
    pub organization_id: Uuid,
    #[cfg_attr(feature = "orm", sea_orm(default_value = "sea_orm::prelude::DateTimeWithchrono::Utc::now()"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "orm", sea_orm(default_value = "sea_orm::prelude::DateTimeWithchrono::Utc::now()"))]
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
    #[serde(default = "default_version")]
    pub version: Option<String>,
    pub resources: DatabaseResources,
    #[serde(default)]
    pub ha: Option<HighAvailability>,
    #[serde(default)]
    pub backup_config: Option<DatabaseBackupConfig>,
}

fn default_version() -> Option<String> {
    Some("15".to_string()) // Default Postgres
}

/// Backup metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct DatabaseBackup {
    pub id: Uuid,
    pub database_id: DatabaseId,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub status: String, // completed, failed, in_progress
    #[serde(default)]
    pub wal_segment_start: Option<String>,
    #[serde(default)]
    pub wal_segment_end: Option<String>,
}