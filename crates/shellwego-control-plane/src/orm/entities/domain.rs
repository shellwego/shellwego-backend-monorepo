//! Domain entity using Sea-ORM
//!
//! Represents custom domains with TLS certificates for edge routing.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "domains")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub hostname: String,
    pub status: String, // TODO: Use DomainStatus enum with custom type
    pub tls_status: String, // TODO: Use TlsStatus enum with custom type
    pub certificate: Option<Json>, // TODO: Use TlsCertificate with custom type
    pub validation: Option<Json>, // TODO: Use DnsValidation with custom type
    pub routing: Json, // TODO: Use RoutingConfig with custom type
    pub features: Json, // TODO: Use EdgeFeatures with custom type
    pub organization_id: Uuid,
    pub created_at: DateTime,
    pub updated_at: DateTime,
    // TODO: Add acme_account_id field
    // TODO: Add certificate_expires_at field
    // TODO: Add last_renewal_attempt field
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // TODO: Define relation to Organization
    // TODO: Define relation to App (via routing.app_id)
    // TODO: Define relation to AcmeAccount (many-to-one)
}

impl ActiveModelBehavior for ActiveModel {
    // TODO: Implement before_save hook for hostname validation
    // TODO: Implement after_save hook for ACME challenge initiation
    // TODO: Implement before_update hook for certificate renewal check
}

// TODO: Implement conversion methods between ORM Model and core entity Domain
// impl From<Model> for shellwego_core::entities::domain::Domain { ... }
// impl From<shellwego_core::entities::domain::Domain> for ActiveModel { ... }

// TODO: Implement custom query methods
// impl Model {
//     pub async fn find_by_hostname(db: &DatabaseConnection, hostname: &str) -> Result<Option<Self>, DbErr> { ... }
//     pub async fn find_expiring_soon(db: &DatabaseConnection, days: i64) -> Result<Vec<Self>, DbErr> { ... }
//     pub async fn find_by_app(db: &DatabaseConnection, app_id: Uuid) -> Result<Vec<Self>, DbErr> { ... }
// }
