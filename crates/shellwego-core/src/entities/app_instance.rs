//! App Instance entity definitions.

#[cfg(feature = "orm")]
use sea_orm::entity::prelude::*;

use crate::prelude::*;
use super::app::{AppId, InstanceStatus};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(schemars::JsonSchema, utoipa::ToSchema))]
#[cfg_attr(feature = "orm", derive(DeriveEntityModel))]
#[cfg_attr(feature = "orm", sea_orm(table_name = "app_instances"))]
pub struct Model {
    #[cfg_attr(feature = "orm", sea_orm(primary_key, auto_increment = false))]
    pub id: Uuid,
    pub app_id: AppId,
    pub node_id: Uuid,
    pub status: InstanceStatus,
    pub internal_ip: String,
    #[cfg_attr(feature = "orm", sea_orm(default_value = "sea_orm::prelude::DateTimeWithchrono::Utc::now()"))]
    pub started_at: chrono::DateTime<Utc>,
    pub health_checks_passed: u64,
    pub health_checks_failed: u64,
}

#[cfg(feature = "orm")]
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[cfg(feature = "orm")]
impl ActiveModelBehavior for ActiveModel {}
