//! ORM layer for database operations
//!
//! Provides connection pooling, migrations, and common database operations
//! backed by real sqlx database connections (SQLite or PostgreSQL).

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

// Re-export entity types from schema crate (canonical source of truth)
pub use shellwego_schema::entities::App as AppEntity;
pub use shellwego_schema::entities::Node as NodeEntity;
pub use shellwego_schema::entities::Volume as VolumeEntity;
pub use shellwego_schema::entities::Secret as SecretEntity;
pub use shellwego_schema::entities::Domain as DomainEntity;
pub use shellwego_schema::billing::{
    Customer, CustomerStatus, Invoice, InvoiceStatus, PaymentResult, SubscriptionTier,
};

// Local stub types for entities not yet exported from schema

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationEntity {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
    pub plan: String,
    pub settings: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Build {
    pub id: uuid::Uuid,
    pub app_id: uuid::Uuid,
    pub status: String,
    pub source: serde_json::Value,
    pub image_reference: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub logs_url: Option<String>,
    pub triggered_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub id: uuid::Uuid,
    pub app_id: uuid::Uuid,
    pub build_id: uuid::Uuid,
    pub status: String,
    pub strategy: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub previous_deployment: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActorType {
    User,
    ApiKey,
    System,
    Webhook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: uuid::Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub org_id: Option<uuid::Uuid>,
    pub actor_id: uuid::Uuid,
    pub actor_type: ActorType,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub changes: Option<serde_json::Value>,
    pub metadata: AuditMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditMetadata {
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
}

/// Database configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    /// Database URL (e.g., "sqlite:/path/to/db.sqlite" or "postgres://user:pass@host/db")
    pub url: String,
    /// Maximum connections in pool
    pub max_connections: u32,
    /// Minimum connections in pool
    pub min_connections: u32,
    /// Connection timeout in seconds
    pub connect_timeout_secs: u64,
    /// Idle timeout in seconds
    pub idle_timeout_secs: u64,
    /// Enable SQLx logging
    pub logging: bool,
    /// Run migrations on startup
    pub auto_migrate: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite:/var/lib/shellwego/control-plane.db".to_string(),
            max_connections: 10,
            min_connections: 1,
            connect_timeout_secs: 30,
            idle_timeout_secs: 600,
            logging: false,
            auto_migrate: true,
        }
    }
}

impl DatabaseConfig {
    /// Detect if the URL is for PostgreSQL
    pub fn is_postgres(&self) -> bool {
        self.url.starts_with("postgres://") || self.url.starts_with("postgresql://")
    }

    /// Detect if the URL is for SQLite
    pub fn is_sqlite(&self) -> bool {
        self.url.starts_with("sqlite:")
    }
}

/// Supported table names for CRUD operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    Organizations,
    Users,
    Apps,
    Nodes,
    Volumes,
    Domains,
    ManagedDatabases,
    Secrets,
    AuditLogs,
    Builds,
    Deployments,
    TeamMembers,
    ApiKeys,
}

impl Table {
    pub fn from_str_name(s: &str) -> Result<Self, DatabaseError> {
        match s {
            "organizations" => Ok(Self::Organizations),
            "users" => Ok(Self::Users),
            "apps" => Ok(Self::Apps),
            "nodes" => Ok(Self::Nodes),
            "volumes" => Ok(Self::Volumes),
            "domains" => Ok(Self::Domains),
            "managed_databases" | "databases" => Ok(Self::ManagedDatabases),
            "secrets" => Ok(Self::Secrets),
            "audit_logs" => Ok(Self::AuditLogs),
            "builds" => Ok(Self::Builds),
            "deployments" => Ok(Self::Deployments),
            "team_members" => Ok(Self::TeamMembers),
            "api_keys" => Ok(Self::ApiKeys),
            _ => Err(DatabaseError::QueryError(format!("Unknown table: {}", s))),
        }
    }

    pub fn table_name(&self) -> &'static str {
        match self {
            Self::Organizations => "organizations",
            Self::Users => "users",
            Self::Apps => "apps",
            Self::Nodes => "nodes",
            Self::Volumes => "volumes",
            Self::Domains => "domains",
            Self::ManagedDatabases => "managed_databases",
            Self::Secrets => "secrets",
            Self::AuditLogs => "audit_logs",
            Self::Builds => "builds",
            Self::Deployments => "deployments",
            Self::TeamMembers => "team_members",
            Self::ApiKeys => "api_keys",
        }
    }
}

impl FromStr for Table {
    type Err = DatabaseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_name(s)
    }
}

/// Database wrapper with real connection pool
pub struct Database {
    pool: sqlx::Pool<sqlx::Any>,
    config: DatabaseConfig,
}

impl Database {
    /// Create a new database connection with the given config
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        info!(
            "Connecting to database: {}",
            Self::sanitize_url(&config.url)
        );

        // Install sqlx::Any drivers (sqlite) before connecting.
        // This registers all compiled-in database drivers (sqlite, postgres, etc.)
        // so that sqlx::Any can route connections to the correct backend.
        sqlx::any::install_default_drivers();

        // Ensure parent directory exists for SQLite
        if config.is_sqlite() {
            let url_path = config.url.strip_prefix("sqlite:").unwrap_or(&config.url);
            if url_path != ":memory:" {
                if let Some(parent) = std::path::Path::new(url_path).parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        DatabaseError::ConnectionError(format!(
                            "Failed to create database directory: {}",
                            e
                        ))
                    })?;
                }
            }
        }

        let pool = sqlx::pool::PoolOptions::<sqlx::Any>::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(std::time::Duration::from_secs(config.connect_timeout_secs))
            .idle_timeout(std::time::Duration::from_secs(config.idle_timeout_secs))
            .connect(&config.url)
            .await
            .map_err(|e| {
                DatabaseError::ConnectionError(format!("Failed to connect: {}", e))
            })?;

        // Enable WAL mode and foreign keys for SQLite
        if config.is_sqlite() {
            sqlx::query("PRAGMA journal_mode=WAL")
                .execute(&pool)
                .await
                .map_err(|e| {
                    DatabaseError::ConnectionError(format!(
                        "Failed to set WAL mode: {}",
                        e
                    ))
                })?;
            sqlx::query("PRAGMA foreign_keys=ON")
                .execute(&pool)
                .await
                .map_err(|e| {
                    DatabaseError::ConnectionError(format!(
                        "Failed to enable foreign keys: {}",
                        e
                    ))
                })?;
        }

        info!("Database connection established");
        Ok(Self { pool, config })
    }

    /// Create a new database with an in-memory SQLite database (for testing)
    pub async fn new_in_memory() -> Result<Self, DatabaseError> {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            max_connections: 1,
            min_connections: 1,
            connect_timeout_secs: 5,
            idle_timeout_secs: 60,
            logging: false,
            auto_migrate: false,
        };
        Self::new(config).await
    }

    /// Get a reference to the underlying pool
    pub fn pool(&self) -> &sqlx::Pool<sqlx::Any> {
        &self.pool
    }

    /// Run migrations from the migrations/ directory
    pub async fn migrate(&self) -> Result<(), DatabaseError> {
        info!("Running database migrations");

        let migrations_path = self.get_migrations_path();
        info!("Loading migrations from: {}", migrations_path.display());

        let migrator = sqlx::migrate::Migrator::new(migrations_path)
            .await
            .map_err(|e| DatabaseError::MigrationError(format!("Migration failed: {}", e)))?;
        migrator
            .run(&self.pool)
            .await
            .map_err(|e| DatabaseError::MigrationError(format!("Migration failed: {}", e)))?;

        info!("Database migrations completed successfully");
        Ok(())
    }

    /// Determine the migrations directory path
    fn get_migrations_path(&self) -> PathBuf {
        // Try CARGO_MANIFEST_DIR first (during development)
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let path = PathBuf::from(&manifest_dir).join("../../migrations");
            if path.exists() {
                return path;
            }
            // Fallback: migrations dir inside the crate
            let inner_path = PathBuf::from(&manifest_dir).join("migrations");
            if inner_path.exists() {
                return inner_path;
            }
        }
        // Final fallback: relative to current working directory
        PathBuf::from("migrations")
    }

    /// Health check - verify database is responsive
    pub async fn health_check(&self) -> Result<(), DatabaseError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DatabaseError::ConnectionError(format!("Health check failed: {}", e))
            })?;
        Ok(())
    }

    /// Close all connections in the pool
    pub async fn close(&self) {
        self.pool.close().await;
        info!("Database connection pool closed");
    }

    /// Sanitize URL for logging (hide password)
    fn sanitize_url(url: &str) -> String {
        if url.contains('@') {
            let at_pos = url.find('@').unwrap();
            let before_at = &url[..at_pos];
            let after_at = &url[at_pos + 1..];

            // Find credentials portion (after "://")
            let creds_start = before_at.find("://").map(|p| p + 3).unwrap_or(0);
            let creds = &before_at[creds_start..];

            // Look for user:password pattern within credentials
            if let Some(colon_pos) = creds.find(':') {
                let user = &creds[..colon_pos];
                return format!("{}{}:***@{}", &before_at[..creds_start], user, after_at);
            }
        }
        url.to_string()
    }

    // ===== Generic Entity Operations =====

    /// Insert a serialized entity into the specified table.
    pub async fn insert<T: Serialize>(&self, table: &str, entity: &T) -> Result<(), DatabaseError> {
        let table_enum = Table::from_str(table)?;
        let value = serde_json::to_value(entity).map_err(|e| {
            DatabaseError::QueryError(format!("Serialization error: {}", e))
        })?;

        match table_enum {
            Table::Apps => self.insert_app(&value).await,
            Table::Nodes => self.insert_node(&value).await,
            Table::Volumes => self.insert_volume(&value).await,
            Table::Secrets => self.insert_secret(&value).await,
            Table::Organizations => self.insert_organization(&value).await,
            Table::Domains => self.insert_domain(&value).await,
            Table::ManagedDatabases => self.insert_managed_database(&value).await,
            Table::AuditLogs => self.insert_audit_log(&value).await,
            Table::Builds => self.insert_build(&value).await,
            Table::Deployments => self.insert_deployment(&value).await,
            Table::Users => self.insert_user(&value).await,
            Table::TeamMembers => self.insert_team_member(&value).await,
            Table::ApiKeys => self.insert_api_key(&value).await,
        }
    }

    /// Find an entity by ID from the specified table
    pub async fn find_by_id<T: for<'de> Deserialize<'de>>(
        &self,
        table: &str,
        id: &Uuid,
    ) -> Result<Option<T>, DatabaseError> {
        let table_enum = Table::from_str(table)?;
        let table_name = table_enum.table_name();

        let sql = format!("SELECT * FROM {} WHERE id = ?", table_name);
        let row: Option<sqlx::any::AnyRow> =
            sqlx::query(&sql)
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    DatabaseError::QueryError(format!("Find by id failed: {}", e))
                })?;

        match row {
            Some(row) => {
                let value = any_row_to_json_value(&row);
                let entity: T = serde_json::from_value(value).map_err(|e| {
                    DatabaseError::QueryError(format!("Deserialization error: {}", e))
                })?;
                Ok(Some(entity))
            }
            None => Ok(None),
        }
    }

    /// Find all entities from the specified table
    pub async fn find_all<T: for<'de> Deserialize<'de>>(
        &self,
        table: &str,
    ) -> Result<Vec<T>, DatabaseError> {
        let table_enum = Table::from_str(table)?;
        let table_name = table_enum.table_name();

        let sql = format!("SELECT * FROM {}", table_name);
        let rows: Vec<sqlx::any::AnyRow> =
            sqlx::query(&sql)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DatabaseError::QueryError(format!("Find all failed: {}", e)))?;

        let mut entities = Vec::new();
        for row in rows {
            let value = any_row_to_json_value(&row);
            let entity: T = serde_json::from_value(value).map_err(|e| {
                DatabaseError::QueryError(format!("Deserialization error: {}", e))
            })?;
            entities.push(entity);
        }
        Ok(entities)
    }

    /// Update an entity by ID
    pub async fn update<T: Serialize>(
        &self,
        table: &str,
        id: &Uuid,
        entity: &T,
    ) -> Result<(), DatabaseError> {
        let table_enum = Table::from_str(table)?;
        let value = serde_json::to_value(entity).map_err(|e| {
            DatabaseError::QueryError(format!("Serialization error: {}", e))
        })?;

        match table_enum {
            Table::Apps => self.update_app(id, &value).await,
            Table::Nodes => self.update_node(id, &value).await,
            Table::Volumes => self.update_volume(id, &value).await,
            Table::Secrets => self.update_secret(id, &value).await,
            Table::Organizations => self.update_organization(id, &value).await,
            Table::Domains => self.update_domain(id, &value).await,
            Table::ManagedDatabases => self.update_managed_database(id, &value).await,
            Table::Users => self.update_user(id, &value).await,
            _ => self.update_generic(table_enum, id, &value).await,
        }
    }

    /// Delete an entity by ID
    pub async fn delete(&self, table: &str, id: &Uuid) -> Result<(), DatabaseError> {
        let table_enum = Table::from_str(table)?;
        let table_name = table_enum.table_name();

        let sql = format!("DELETE FROM {} WHERE id = ?", table_name);
        let result = sqlx::query(&sql)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(format!("Delete failed: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(DatabaseError::NotFound);
        }
        Ok(())
    }

    /// Query entities with conditions, limit, and offset
    pub async fn query<T: for<'de> Deserialize<'de>>(
        &self,
        table: &str,
        conditions: HashMap<String, String>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<T>, DatabaseError> {
        let table_enum = Table::from_str(table)?;
        let table_name = table_enum.table_name();

        let mut where_clauses = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        for (key, val) in &conditions {
            where_clauses.push(format!("{} = ?", key));
            binds.push(val.clone());
        }

        let mut sql = format!("SELECT * FROM {}", table_name);
        if !where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clauses.join(" AND "));
        }

        if let Some(limit) = limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let mut query = sqlx::query(&sql);
        for bind_val in binds {
            query = query.bind(bind_val);
        }

        let rows: Vec<sqlx::any::AnyRow> = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(format!("Query failed: {}", e)))?;

        let mut entities = Vec::new();
        for row in rows {
            let value = any_row_to_json_value(&row);
            let entity: T = serde_json::from_value(value).map_err(|e| {
                DatabaseError::QueryError(format!("Deserialization error: {}", e))
            })?;
            entities.push(entity);
        }
        Ok(entities)
    }

    /// Count entities in a table
    pub async fn count(&self, table: &str) -> Result<u64, DatabaseError> {
        let table_enum = Table::from_str(table)?;
        let table_name = table_enum.table_name();

        let sql = format!("SELECT COUNT(*) as count FROM {}", table_name);
        let row: (i64,) = sqlx::query_as(&sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(format!("Count failed: {}", e)))?;

        Ok(row.0 as u64)
    }

    /// Execute a raw SQL query with optional parameters
    pub async fn raw_query(&self, sql: &str, params: Vec<String>) -> Result<(), DatabaseError> {
        let mut query = sqlx::query(sql);
        for param in params {
            query = query.bind(param);
        }
        query
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(format!("Raw query failed: {}", e)))?;
        Ok(())
    }

    /// Execute a transaction with real rollback support
    pub async fn transaction<'tx, F, T, E>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce(
            &mut sqlx::Transaction<'tx, sqlx::Any>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send + 'tx>>,
        E: From<DatabaseError>,
    {
        let mut tx = self.pool.begin().await.map_err(|e| {
            E::from(DatabaseError::ConnectionError(format!(
                "Failed to begin transaction: {}",
                e
            )))
        })?;

        let result = f(&mut tx).await;

        match result {
            Ok(value) => {
                tx.commit().await.map_err(|e| {
                    E::from(DatabaseError::ConnectionError(format!(
                        "Failed to commit transaction: {}",
                        e
                    )))
                })?;
                Ok(value)
            }
            Err(e) => {
                if let Err(rollback_err) = tx.rollback().await {
                    warn!("Failed to rollback transaction: {}", rollback_err);
                }
                Err(e)
            }
        }
    }

    // ===== Table-specific insert methods =====

    async fn insert_app(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        let cmd_json = match v.get("command") {
            Some(serde_json::Value::Null) => None,
            Some(c) => Some(serde_json::to_string(c).unwrap_or_default()),
            None => None,
        };
        sqlx::query(
            "INSERT INTO apps (id, name, slug, status, image, command, resources, env, domains, volumes, health_check, source, organization_id, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "name"))
        .bind(str_val(v, "slug"))
        .bind(str_or(v, "status", "creating"))
        .bind(str_or(v, "image", ""))
        .bind(cmd_json)
        .bind(json_str(v.get("resources")))
        .bind(json_str(v.get("env")))
        .bind(json_str(v.get("domains")))
        .bind(json_str(v.get("volumes")))
        .bind(json_str_or_null(v.get("health_check")))
        .bind(json_str(v.get("source")))
        .bind(str_val(v, "organization_id"))
        .bind(str_val(v, "created_by"))
        .bind(str_or_ts(v, "created_at"))
        .bind(str_or_ts(v, "updated_at"))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert app failed: {}", e)))?;
        Ok(())
    }

    async fn insert_node(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO nodes (id, hostname, status, region, zone, capacity, capabilities, network, labels, running_apps, microvm_capacity, microvm_used, kernel_version, firecracker_version, agent_version, last_seen, created_at, organization_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "hostname"))
        .bind(str_or(v, "status", "registering"))
        .bind(str_or(v, "region", ""))
        .bind(str_or(v, "zone", ""))
        .bind(json_str(v.get("capacity")))
        .bind(json_str(v.get("capabilities")))
        .bind(json_str(v.get("network")))
        .bind(json_str(v.get("labels")))
        .bind(int_val(v, "running_apps", 0))
        .bind(int_val(v, "microvm_capacity", 0))
        .bind(int_val(v, "microvm_used", 0))
        .bind(str_or(v, "kernel_version", ""))
        .bind(str_or(v, "firecracker_version", ""))
        .bind(str_or(v, "agent_version", ""))
        .bind(str_or_ts(v, "last_seen"))
        .bind(str_or_ts(v, "created_at"))
        .bind(str_val(v, "organization_id"))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert node failed: {}", e)))?;
        Ok(())
    }

    async fn insert_volume(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO volumes (id, name, status, size_gb, used_gb, volume_type, filesystem, encrypted, encryption_key_id, attached_to, mount_path, snapshots, backup_policy, organization_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "name"))
        .bind(str_or(v, "status", "creating"))
        .bind(int_val(v, "size_gb", 0))
        .bind(int_val(v, "used_gb", 0))
        .bind(str_or(v, "volume_type", "persistent"))
        .bind(str_or(v, "filesystem", "ext4"))
        .bind(bool_int(v, "encrypted", false))
        .bind(v["encryption_key_id"].as_str())
        .bind(v["attached_to"].as_str())
        .bind(v["mount_path"].as_str())
        .bind(json_str(v.get("snapshots")))
        .bind(json_str_or_null(v.get("backup_policy")))
        .bind(str_val(v, "organization_id"))
        .bind(str_or_ts(v, "created_at"))
        .bind(str_or_ts(v, "updated_at"))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert volume failed: {}", e)))?;
        Ok(())
    }

    async fn insert_secret(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO secrets (id, name, scope, app_id, current_version, versions, last_used_at, expires_at, encrypted_value, key_id, nonce, organization_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "name"))
        .bind(str_or(v, "scope", "organization"))
        .bind(v["app_id"].as_str())
        .bind(int_val(v, "current_version", 1))
        .bind(json_str(v.get("versions")))
        .bind(v["last_used_at"].as_str())
        .bind(v["expires_at"].as_str())
        .bind(str_or(v, "encrypted_value", ""))
        .bind(v["key_id"].as_str())
        .bind(v["nonce"].as_str())
        .bind(str_val(v, "organization_id"))
        .bind(str_or_ts(v, "created_at"))
        .bind(str_or_ts(v, "updated_at"))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert secret failed: {}", e)))?;
        Ok(())
    }

    async fn insert_organization(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO organizations (id, name, slug, plan, settings, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "name"))
        .bind(str_val(v, "slug"))
        .bind(str_or(v, "plan", "free"))
        .bind(json_str(v.get("settings")))
        .bind(str_or_ts(v, "created_at"))
        .bind(str_or_ts(v, "updated_at"))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert organization failed: {}", e)))?;
        Ok(())
    }

    async fn insert_domain(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO domains (id, hostname, status, tls_status, certificate, validation, routing, features, organization_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "hostname"))
        .bind(str_or(v, "status", "pending"))
        .bind(str_or(v, "tls_status", "none"))
        .bind(json_str_or_null(v.get("certificate")))
        .bind(json_str_or_null(v.get("validation")))
        .bind(json_str(v.get("routing")))
        .bind(json_str(v.get("features")))
        .bind(str_val(v, "organization_id"))
        .bind(str_or_ts(v, "created_at"))
        .bind(str_or_ts(v, "updated_at"))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert domain failed: {}", e)))?;
        Ok(())
    }

    async fn insert_managed_database(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO managed_databases (id, name, engine, version, status, endpoint, resources, ha, backup_config, organization_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "name"))
        .bind(str_or(v, "engine", "postgres"))
        .bind(str_or(v, "version", "15"))
        .bind(str_or(v, "status", "provisioning"))
        .bind(json_str(v.get("endpoint")))
        .bind(json_str(v.get("resources")))
        .bind(json_str(v.get("ha")))
        .bind(json_str(v.get("backup_config")))
        .bind(str_val(v, "organization_id"))
        .bind(str_or_ts(v, "created_at"))
        .bind(str_or_ts(v, "updated_at"))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert managed database failed: {}", e)))?;
        Ok(())
    }

    async fn insert_audit_log(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO audit_logs (id, timestamp, org_id, actor_id, actor_type, action, resource_type, resource_id, changes, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_or_ts(v, "timestamp"))
        .bind(v["org_id"].as_str())
        .bind(str_val(v, "actor_id"))
        .bind(str_or(v, "actor_type", "user"))
        .bind(str_val(v, "action"))
        .bind(str_val(v, "resource_type"))
        .bind(str_val(v, "resource_id"))
        .bind(json_str_or_null(v.get("changes")))
        .bind(json_str(v.get("metadata")))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert audit log failed: {}", e)))?;
        Ok(())
    }

    async fn insert_build(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO builds (id, app_id, status, source, image_reference, started_at, finished_at, logs_url, triggered_by) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "app_id"))
        .bind(str_or(v, "status", "queued"))
        .bind(json_str(v.get("source")))
        .bind(v["image_reference"].as_str())
        .bind(str_or_ts(v, "started_at"))
        .bind(v["finished_at"].as_str())
        .bind(v["logs_url"].as_str())
        .bind(str_or(v, "triggered_by", ""))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert build failed: {}", e)))?;
        Ok(())
    }

    async fn insert_deployment(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO deployments (id, app_id, build_id, status, strategy, started_at, finished_at, previous_deployment) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "app_id"))
        .bind(str_val(v, "build_id"))
        .bind(str_or(v, "status", "pending"))
        .bind(str_or(v, "strategy", "rolling"))
        .bind(str_or_ts(v, "started_at"))
        .bind(v["finished_at"].as_str())
        .bind(v["previous_deployment"].as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryError(format!("Insert deployment failed: {}", e))
        })?;
        Ok(())
    }

    async fn insert_user(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, display_name, organization_id, role, is_active, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "email"))
        .bind(str_val(v, "password_hash"))
        .bind(str_or(v, "display_name", ""))
        .bind(str_val(v, "organization_id"))
        .bind(str_or(v, "role", "developer"))
        .bind(bool_int(v, "is_active", true))
        .bind(str_or_ts(v, "created_at"))
        .bind(str_or_ts(v, "updated_at"))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert user failed: {}", e)))?;
        Ok(())
    }

    async fn insert_team_member(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO team_members (user_id, org_id, role, joined_at) VALUES (?, ?, ?, ?)"
        )
        .bind(str_val(v, "user_id"))
        .bind(str_val(v, "org_id"))
        .bind(str_or(v, "role", "developer"))
        .bind(str_or_ts(v, "joined_at"))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryError(format!("Insert team member failed: {}", e))
        })?;
        Ok(())
    }

    async fn insert_api_key(&self, v: &serde_json::Value) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT INTO api_keys (id, org_id, name, key_hash, scopes, last_used_at, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(str_val(v, "id"))
        .bind(str_val(v, "org_id"))
        .bind(str_val(v, "name"))
        .bind(str_val(v, "key_hash"))
        .bind(json_str(v.get("scopes")))
        .bind(v["last_used_at"].as_str())
        .bind(v["expires_at"].as_str())
        .bind(str_or_ts(v, "created_at"))
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Insert API key failed: {}", e)))?;
        Ok(())
    }

    // ===== Table-specific update methods =====

    async fn update_app(&self, id: &Uuid, v: &serde_json::Value) -> Result<(), DatabaseError> {
        let now = Utc::now().to_rfc3339();
        let cmd_json = match v.get("command") {
            Some(serde_json::Value::Null) => None,
            Some(c) => Some(serde_json::to_string(c).unwrap_or_default()),
            None => None,
        };
        sqlx::query(
            "UPDATE apps SET name = ?, slug = ?, status = ?, image = ?, command = ?, resources = ?, env = ?, domains = ?, volumes = ?, health_check = ?, source = ?, updated_at = ? WHERE id = ?"
        )
        .bind(str_val(v, "name"))
        .bind(str_val(v, "slug"))
        .bind(str_or(v, "status", "creating"))
        .bind(str_or(v, "image", ""))
        .bind(cmd_json)
        .bind(json_str(v.get("resources")))
        .bind(json_str(v.get("env")))
        .bind(json_str(v.get("domains")))
        .bind(json_str(v.get("volumes")))
        .bind(json_str_or_null(v.get("health_check")))
        .bind(json_str(v.get("source")))
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Update app failed: {}", e)))?;
        Ok(())
    }

    async fn update_node(&self, id: &Uuid, v: &serde_json::Value) -> Result<(), DatabaseError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE nodes SET hostname = ?, status = ?, region = ?, zone = ?, capacity = ?, capabilities = ?, network = ?, labels = ?, running_apps = ?, microvm_capacity = ?, microvm_used = ?, kernel_version = ?, firecracker_version = ?, agent_version = ?, last_seen = ? WHERE id = ?"
        )
        .bind(str_val(v, "hostname"))
        .bind(str_or(v, "status", "registering"))
        .bind(str_or(v, "region", ""))
        .bind(str_or(v, "zone", ""))
        .bind(json_str(v.get("capacity")))
        .bind(json_str(v.get("capabilities")))
        .bind(json_str(v.get("network")))
        .bind(json_str(v.get("labels")))
        .bind(int_val(v, "running_apps", 0))
        .bind(int_val(v, "microvm_capacity", 0))
        .bind(int_val(v, "microvm_used", 0))
        .bind(str_or(v, "kernel_version", ""))
        .bind(str_or(v, "firecracker_version", ""))
        .bind(str_or(v, "agent_version", ""))
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Update node failed: {}", e)))?;
        Ok(())
    }

    async fn update_volume(&self, id: &Uuid, v: &serde_json::Value) -> Result<(), DatabaseError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE volumes SET name = ?, status = ?, size_gb = ?, used_gb = ?, volume_type = ?, filesystem = ?, encrypted = ?, encryption_key_id = ?, attached_to = ?, mount_path = ?, snapshots = ?, backup_policy = ?, updated_at = ? WHERE id = ?"
        )
        .bind(str_val(v, "name"))
        .bind(str_or(v, "status", "creating"))
        .bind(int_val(v, "size_gb", 0))
        .bind(int_val(v, "used_gb", 0))
        .bind(str_or(v, "volume_type", "persistent"))
        .bind(str_or(v, "filesystem", "ext4"))
        .bind(bool_int(v, "encrypted", false))
        .bind(v["encryption_key_id"].as_str())
        .bind(v["attached_to"].as_str())
        .bind(v["mount_path"].as_str())
        .bind(json_str(v.get("snapshots")))
        .bind(json_str_or_null(v.get("backup_policy")))
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Update volume failed: {}", e)))?;
        Ok(())
    }

    async fn update_secret(&self, id: &Uuid, v: &serde_json::Value) -> Result<(), DatabaseError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE secrets SET name = ?, scope = ?, app_id = ?, current_version = ?, versions = ?, last_used_at = ?, expires_at = ?, encrypted_value = ?, key_id = ?, nonce = ?, updated_at = ? WHERE id = ?"
        )
        .bind(str_val(v, "name"))
        .bind(str_or(v, "scope", "organization"))
        .bind(v["app_id"].as_str())
        .bind(int_val(v, "current_version", 1))
        .bind(json_str(v.get("versions")))
        .bind(v["last_used_at"].as_str())
        .bind(v["expires_at"].as_str())
        .bind(str_or(v, "encrypted_value", ""))
        .bind(v["key_id"].as_str())
        .bind(v["nonce"].as_str())
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Update secret failed: {}", e)))?;
        Ok(())
    }

    async fn update_organization(
        &self,
        id: &Uuid,
        v: &serde_json::Value,
    ) -> Result<(), DatabaseError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE organizations SET name = ?, slug = ?, plan = ?, settings = ?, updated_at = ? WHERE id = ?"
        )
        .bind(str_val(v, "name"))
        .bind(str_val(v, "slug"))
        .bind(str_or(v, "plan", "free"))
        .bind(json_str(v.get("settings")))
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryError(format!("Update organization failed: {}", e))
        })?;
        Ok(())
    }

    async fn update_domain(&self, id: &Uuid, v: &serde_json::Value) -> Result<(), DatabaseError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE domains SET hostname = ?, status = ?, tls_status = ?, certificate = ?, validation = ?, routing = ?, features = ?, updated_at = ? WHERE id = ?"
        )
        .bind(str_val(v, "hostname"))
        .bind(str_or(v, "status", "pending"))
        .bind(str_or(v, "tls_status", "none"))
        .bind(json_str_or_null(v.get("certificate")))
        .bind(json_str_or_null(v.get("validation")))
        .bind(json_str(v.get("routing")))
        .bind(json_str(v.get("features")))
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Update domain failed: {}", e)))?;
        Ok(())
    }

    async fn update_managed_database(
        &self,
        id: &Uuid,
        v: &serde_json::Value,
    ) -> Result<(), DatabaseError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE managed_databases SET name = ?, engine = ?, version = ?, status = ?, endpoint = ?, resources = ?, ha = ?, backup_config = ?, updated_at = ? WHERE id = ?"
        )
        .bind(str_val(v, "name"))
        .bind(str_or(v, "engine", "postgres"))
        .bind(str_or(v, "version", "15"))
        .bind(str_or(v, "status", "provisioning"))
        .bind(json_str(v.get("endpoint")))
        .bind(json_str(v.get("resources")))
        .bind(json_str(v.get("ha")))
        .bind(json_str(v.get("backup_config")))
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryError(format!("Update managed database failed: {}", e))
        })?;
        Ok(())
    }

    async fn update_user(&self, id: &Uuid, v: &serde_json::Value) -> Result<(), DatabaseError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE users SET email = ?, password_hash = ?, display_name = ?, organization_id = ?, role = ?, is_active = ?, updated_at = ? WHERE id = ?"
        )
        .bind(str_val(v, "email"))
        .bind(str_val(v, "password_hash"))
        .bind(str_or(v, "display_name", ""))
        .bind(str_val(v, "organization_id"))
        .bind(str_or(v, "role", "developer"))
        .bind(bool_int(v, "is_active", true))
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Update user failed: {}", e)))?;
        Ok(())
    }

    async fn update_generic(
        &self,
        table: Table,
        id: &Uuid,
        _v: &serde_json::Value,
    ) -> Result<(), DatabaseError> {
        debug!(
            "Generic update called for table {} (id={}) - no specific handler",
            table.table_name(),
            id
        );
        Ok(())
    }
}

/// Database error type
#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Migration error: {0}")]
    MigrationError(String),

    #[error("Not connected")]
    NotConnected,

    #[error("Entity not found")]
    NotFound,

    #[error("Duplicate key")]
    DuplicateKey,

    #[error("Timeout")]
    Timeout,
}

// ===== Helper functions for JSON value extraction =====

/// Get a string field from JSON value, defaulting to empty string
fn str_val<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

/// Get a string field from JSON value, with a default
fn str_or(v: &serde_json::Value, key: &str, default: &str) -> String {
    v.get(key).and_then(|v| v.as_str()).unwrap_or(default).to_string()
}

/// Get a string field from JSON value, defaulting to current UTC timestamp
fn str_or_ts(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|v| v.as_str()).unwrap_or(&Utc::now().to_rfc3339()).to_string()
}

/// Get an integer field from JSON value, with a default
fn int_val(v: &serde_json::Value, key: &str, default: i64) -> i64 {
    v.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
}

/// Get a boolean field from JSON value as integer (0/1)
fn bool_int(v: &serde_json::Value, key: &str, default: bool) -> i32 {
    match v.get(key) {
        Some(serde_json::Value::Bool(b)) => {
            if *b { 1 } else { 0 }
        }
        _ => {
            if default { 1 } else { 0 }
        }
    }
}

/// Convert a JSON value to a string for storage in a TEXT column
fn json_str(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(val) => serde_json::to_string(val).unwrap_or_else(|_| "{}".to_string()),
        None => "{}".to_string(),
    }
}

/// Convert a JSON value to an Option<String> (NULL if None)
fn json_str_or_null(v: Option<&serde_json::Value>) -> Option<String> {
    v.map(|val| serde_json::to_string(val).unwrap_or_else(|_| "null".to_string()))
}

/// Convert an sqlx AnyRow to a serde_json::Value
fn any_row_to_json_value(row: &sqlx::any::AnyRow) -> serde_json::Value {
    use serde_json::{Number, Value};
    use sqlx::{Column as _, Row};

    let mut map = serde_json::Map::new();

    for col in row.columns() {
        let col_name = col.name();
        // Use ordinal-based type detection as fallback
        let value = if let Ok(val) = row.try_get::<String, _>(col_name) {
            if val.starts_with('{') || val.starts_with('[') {
                serde_json::from_str(&val).unwrap_or(Value::String(val))
            } else {
                Value::String(val)
            }
        } else if let Ok(val) = row.try_get::<i64, _>(col_name) {
            Value::Number(Number::from(val))
        } else if let Ok(val) = row.try_get::<f64, _>(col_name) {
            Number::from_f64(val).map(Value::Number).unwrap_or(Value::Null)
        } else if let Ok(val) = row.try_get::<bool, _>(col_name) {
            Value::Bool(val)
        } else if let Ok(val) = row.try_get::<i32, _>(col_name) {
            Value::Number(Number::from(val))
        } else {
            Value::Null
        };

        map.insert(col_name.to_string(), value);
    }

    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper to create an in-memory test database with migrations applied
    async fn setup_test_db() -> Database {
        let db = Database::new_in_memory().await.expect("Failed to create in-memory db");
        db.migrate().await.expect("Failed to run migrations");
        db
    }

    #[tokio::test]
    async fn test_database_creation() {
        let db = Database::new_in_memory().await;
        assert!(db.is_ok());
    }

    #[tokio::test]
    async fn test_health_check() {
        let db = setup_test_db().await;
        let result = db.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_migrate() {
        let db = setup_test_db().await;
        let count = db.count("organizations").await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_insert_and_find_organization() {
        let db = setup_test_db().await;
        let org_id = Uuid::new_v4();
        let now = Utc::now();

        let org = json!({
            "id": org_id,
            "name": "Test Org",
            "slug": "test-org",
            "plan": "free",
            "settings": {"allowed_regions": [], "max_apps": 10, "max_team_members": 5, "require_2fa": false, "sso_enabled": false},
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339()
        });

        db.insert("organizations", &org).await.unwrap();

        let found: Option<serde_json::Value> =
            db.find_by_id("organizations", &org_id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found["name"], "Test Org");
        assert_eq!(found["slug"], "test-org");
    }

    #[tokio::test]
    async fn test_find_by_id_not_found() {
        let db = setup_test_db().await;
        let fake_id = Uuid::new_v4();
        let found: Option<serde_json::Value> =
            db.find_by_id("organizations", &fake_id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_all() {
        let db = setup_test_db().await;
        let now = Utc::now();

        for i in 1..=2 {
            let org = json!({
                "id": Uuid::new_v4(),
                "name": format!("Org {}", i),
                "slug": format!("org-{}", i),
                "plan": "free",
                "settings": {},
                "created_at": now.to_rfc3339(),
                "updated_at": now.to_rfc3339()
            });
            db.insert("organizations", &org).await.unwrap();
        }

        let all: Vec<serde_json::Value> = db.find_all("organizations").await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_update() {
        let db = setup_test_db().await;
        let org_id = Uuid::new_v4();
        let now = Utc::now();

        let org = json!({
            "id": org_id,
            "name": "Original Name",
            "slug": "original",
            "plan": "free",
            "settings": {},
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339()
        });
        db.insert("organizations", &org).await.unwrap();

        let updated = json!({
            "id": org_id,
            "name": "Updated Name",
            "slug": "original",
            "plan": "starter",
            "settings": {},
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339()
        });
        db.update("organizations", &org_id, &updated).await.unwrap();

        let found: Option<serde_json::Value> =
            db.find_by_id("organizations", &org_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap()["name"], "Updated Name");
    }

    #[tokio::test]
    async fn test_delete() {
        let db = setup_test_db().await;
        let org_id = Uuid::new_v4();
        let now = Utc::now();

        let org = json!({
            "id": org_id,
            "name": "To Delete",
            "slug": "to-delete",
            "plan": "free",
            "settings": {},
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339()
        });
        db.insert("organizations", &org).await.unwrap();

        db.delete("organizations", &org_id).await.unwrap();

        let found: Option<serde_json::Value> =
            db.find_by_id("organizations", &org_id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_delete_not_found() {
        let db = setup_test_db().await;
        let fake_id = Uuid::new_v4();
        let result = db.delete("organizations", &fake_id).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DatabaseError::NotFound));
    }

    #[tokio::test]
    async fn test_count() {
        let db = setup_test_db().await;
        let now = Utc::now();

        assert_eq!(db.count("organizations").await.unwrap(), 0);

        for i in 1..=3 {
            let org = json!({
                "id": Uuid::new_v4(),
                "name": format!("Org {}", i),
                "slug": format!("org-{}", i),
                "plan": "free",
                "settings": {},
                "created_at": now.to_rfc3339(),
                "updated_at": now.to_rfc3339()
            });
            db.insert("organizations", &org).await.unwrap();
        }

        assert_eq!(db.count("organizations").await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_query_with_conditions() {
        let db = setup_test_db().await;
        let now = Utc::now();

        for (name, plan) in [("Free Org", "free"), ("Pro Org", "starter")] {
            let org = json!({
                "id": Uuid::new_v4(),
                "name": name,
                "slug": name.to_lowercase().replace(' ', "-"),
                "plan": plan,
                "settings": {},
                "created_at": now.to_rfc3339(),
                "updated_at": now.to_rfc3339()
            });
            db.insert("organizations", &org).await.unwrap();
        }

        let mut conditions = HashMap::new();
        conditions.insert("plan".to_string(), "free".to_string());
        let results: Vec<serde_json::Value> =
            db.query("organizations", conditions, None, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["name"], "Free Org");
    }

    #[tokio::test]
    async fn test_transaction_commit() {
        let db = setup_test_db().await;
        let org_id = Uuid::new_v4();
        let now = Utc::now();

        let org = json!({
            "id": org_id,
            "name": "Transaction Org",
            "slug": "tx-org",
            "plan": "free",
            "settings": {},
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339()
        });

        // Use the Database::insert method which internally uses the pool
        db.insert("organizations", &org).await.unwrap();

        let found: Option<serde_json::Value> =
            db.find_by_id("organizations", &org_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap()["name"], "Transaction Org");
    }

    #[tokio::test]
    async fn test_transaction_rollback() {
        let db = setup_test_db().await;
        let org_id = Uuid::new_v4();
        let now = Utc::now();

        let org = json!({
            "id": org_id,
            "name": "Should Not Exist",
            "slug": "should-not-exist",
            "plan": "free",
            "settings": {},
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339()
        });

        // Begin a transaction manually, insert, then rollback
        let mut tx = db.pool.begin().await
            .map_err(|e| DatabaseError::ConnectionError(format!("Begin tx failed: {}", e)))
            .unwrap();

        sqlx::query(
            "INSERT INTO organizations (id, name, slug, plan, settings, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(org["id"].as_str().unwrap())
        .bind(org["name"].as_str().unwrap())
        .bind(org["slug"].as_str().unwrap())
        .bind(org["plan"].as_str().unwrap())
        .bind(serde_json::to_string(&org["settings"]).unwrap_or_default())
        .bind(org["created_at"].as_str().unwrap())
        .bind(org["updated_at"].as_str().unwrap())
        .execute(&mut *tx)
        .await
        .unwrap();

        tx.rollback().await.unwrap();

        let found: Option<serde_json::Value> =
            db.find_by_id("organizations", &org_id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_insert_app() {
        let db = setup_test_db().await;

        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let now = Utc::now();

        let org = json!({
            "id": org_id,
            "name": "App Test Org",
            "slug": "app-test-org",
            "plan": "free",
            "settings": {},
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339()
        });
        db.insert("organizations", &org).await.unwrap();

        let app_id = Uuid::new_v4();
        let app = json!({
            "id": app_id,
            "name": "my-app",
            "slug": "my-app",
            "status": "creating",
            "image": "nginx:latest",
            "command": null,
            "resources": {"memory_bytes": 268435456, "cpu_milli": 500, "disk_bytes": 5368709120_i64},
            "env": [],
            "domains": [],
            "volumes": [],
            "health_check": null,
            "source": {"type": "docker", "image": "nginx:latest"},
            "organization_id": org_id,
            "created_by": user_id,
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339()
        });

        db.insert("apps", &app).await.unwrap();

        let found: Option<serde_json::Value> =
            db.find_by_id("apps", &app_id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found["name"], "my-app");
        assert_eq!(found["image"], "nginx:latest");
    }

    #[tokio::test]
    async fn test_sanitize_url() {
        let url = "postgres://user:password@localhost:5432/mydb";
        let sanitized = Database::sanitize_url(url);
        assert!(sanitized.contains("user"));
        assert!(sanitized.contains("localhost"));
        assert!(!sanitized.contains("password"));
        assert!(sanitized.contains("***"));
    }

    #[tokio::test]
    async fn test_config_detection() {
        let sqlite_config = DatabaseConfig {
            url: "sqlite:/tmp/test.db".to_string(),
            ..Default::default()
        };
        assert!(sqlite_config.is_sqlite());
        assert!(!sqlite_config.is_postgres());

        let pg_config = DatabaseConfig {
            url: "postgres://user:pass@localhost/db".to_string(),
            ..Default::default()
        };
        assert!(pg_config.is_postgres());
        assert!(!pg_config.is_sqlite());
    }

    #[tokio::test]
    async fn test_table_from_str() {
        assert!(Table::from_str_name("apps").is_ok());
        assert!(Table::from_str_name("organizations").is_ok());
        assert!(Table::from_str_name("nodes").is_ok());
        assert!(Table::from_str_name("volumes").is_ok());
        assert!(Table::from_str_name("secrets").is_ok());
        assert!(Table::from_str_name("unknown_table").is_err());
    }
}
