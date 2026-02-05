//! Secret management entity definitions.
//!
//! Encrypted key-value store for credentials and sensitive config.

use secrecy::SecretString;
#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;
#[cfg(feature = "orm")]
use sea_query::IdenStatic;
use crate::prelude::*;

pub type SecretId = Uuid;

/// Secret visibility scope
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::entity::prelude::DeriveActiveEnum, sea_query::IdenStatic))]
#[cfg_attr(feature = "orm", sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))"))]
#[serde(rename_all = "snake_case")]
pub enum SecretScope {
    Organization,  // Shared across org
    App,           // Specific to one app
    Node,          // Node-level secrets (rare)
}

/// Individual secret version
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(sea_orm::FromQueryResult))]
pub struct SecretVersion {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub created_by: Uuid,
    // Value is never returned in API responses
}

/// Secret entity (metadata only, never exposes value)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "secrets"))]
pub struct Secret {
    #[cfg_attr(feature = "orm", sea_orm(primary_key, auto_increment = false))]
    pub id: SecretId,
    pub name: String,
    pub scope: SecretScope,
    #[serde(default)]
    pub app_id: Option<Uuid>,
    pub current_version: u32,
    #[cfg_attr(feature = "orm", sea_orm(column_type = "JsonBinary"))]
    pub versions: Vec<SecretVersion>,
    #[serde(default)]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    pub organization_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(feature = "orm")]
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[cfg(feature = "orm")]
impl ActiveModelBehavior for ActiveModel {}

/// Create secret request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct CreateSecretRequest {
    pub name: String,
    #[cfg_attr(feature = "openapi", schemars(with = "String"))]
    pub value: SecretString,
    pub scope: SecretScope,
    #[serde(default)]
    pub app_id: Option<Uuid>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Rotate secret request (create new version)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct RotateSecretRequest {
    pub value: String,
}

/// Secret reference (how apps consume secrets)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema))]
pub struct SecretRef {
    pub secret_id: SecretId,
    #[serde(default)]
    pub version: Option<u32>, // None = latest
    pub env_name: String,     // Name to inject as
}
