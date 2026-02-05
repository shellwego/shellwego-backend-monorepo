//! Secret management entity definitions.
//!
//! Encrypted key-value store for credentials and sensitive config.

#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;
#[cfg(feature = "orm")]
use sea_query::IdenStatic;
use crate::prelude::*;

pub type SecretId = Uuid;

/// Secret visibility scope
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_orm::EnumIter, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum SecretScope {
    #[cfg_attr(feature = "orm", sea_orm(string_value = "organization"))]
    Organization,  // Shared across org
    #[cfg_attr(feature = "orm", sea_orm(string_value = "app"))]
    App,           // Specific to one app
    #[cfg_attr(feature = "orm", sea_orm(string_value = "node"))]
    Node,          // Node-level secrets (rare)
}

/// Individual secret version
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromJsonQueryResult))]
pub struct SecretVersion {
    pub version: u32,
    pub created_at: chrono::DateTime<Utc>,
    pub created_by: Uuid,
    // Value is never returned in API responses
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromJsonQueryResult))]
#[serde(transparent)]
pub struct SecretVersions(pub Vec<SecretVersion>);

/// Secret entity (metadata only, never exposes value)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "secrets"))]
pub struct Model {
    #[cfg_attr(feature = "orm", sea_orm(primary_key, auto_increment = false))]
    pub id: SecretId,
    pub name: String,
    pub scope: SecretScope,
    #[serde(default)]
    pub app_id: Option<Uuid>,
    pub current_version: u32,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub versions: SecretVersions,
    #[serde(default)]
    pub last_used_at: Option<chrono::DateTime<Utc>>,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub organization_id: Uuid,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[cfg(feature = "orm")]
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[cfg(feature = "orm")]
impl ActiveModelBehavior for ActiveModel {}

/// Create secret request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
pub struct CreateSecretRequest {
    pub name: String,
    pub value: String,
    pub scope: SecretScope,
    #[serde(default)]
    pub app_id: Option<Uuid>,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<Utc>>,
}

/// Rotate secret request (create new version)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
pub struct RotateSecretRequest {
    pub value: String,
}

/// Secret reference (how apps consume secrets)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
pub struct SecretRef {
    pub secret_id: SecretId,
    #[serde(default)]
    pub version: Option<u32>, // None = latest
    pub env_name: String,     // Name to inject as
}
