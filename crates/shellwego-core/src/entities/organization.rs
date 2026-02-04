//! Organization and team management

use crate::prelude::*;

/// Organization entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
    pub plan: PlanTier,
    pub settings: OrgSettings,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanTier {
    Free,
    Starter,
    Growth,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgSettings {
    pub allowed_regions: Vec<String>,
    pub max_apps: u32,
    pub max_team_members: u32,
    pub require_2fa: bool,
    pub sso_enabled: bool,
}

/// Team member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub user_id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub role: TeamRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeamRole {
    Owner,
    Admin,
    Developer,
    Viewer,
}

/// API key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: uuid::Uuid,
    pub org_id: uuid::Uuid,
    pub name: String,
    pub scopes: Vec<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}