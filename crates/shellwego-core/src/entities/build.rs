//! Build and deployment entities

use crate::prelude::*;

/// Build record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct Build {
    pub id: uuid::Uuid,
    pub app_id: uuid::Uuid,
    pub status: BuildStatus,
    pub source: BuildSource,
    pub image_reference: Option<String>,
    pub started_at: chrono::DateTime<Utc>,
    pub finished_at: Option<chrono::DateTime<Utc>>,
    pub logs_url: Option<String>,
    pub triggered_by: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum BuildStatus {
    Queued,
    Cloning,
    Building,
    Pushing,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub enum BuildSource {
    Git {
        repository: String,
        ref_name: String,
        commit_sha: String,
    },
    Dockerfile {
        content: String,
        context_url: Option<String>,
    },
    Buildpack {
        builder_image: String,
    },
}

/// Deployment record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct Deployment {
    pub id: uuid::Uuid,
    pub app_id: uuid::Uuid,
    pub build_id: uuid::Uuid,
    pub status: DeploymentStatus,
    pub strategy: DeploymentStrategy,
    pub started_at: chrono::DateTime<Utc>,
    pub finished_at: Option<chrono::DateTime<Utc>>,
    pub previous_deployment: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum DeploymentStatus {
    Pending,
    InProgress,
    Succeeded,
    Failed,
    RollingBack,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum DeploymentStrategy {
    Rolling,
    BlueGreen,
    Canary,
    Immediate,
}