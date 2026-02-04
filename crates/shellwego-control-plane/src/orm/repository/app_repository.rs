use sea_orm::{DatabaseConnection, DbErr, EntityTrait, QueryFilter, ColumnTrait, PaginatorTrait};
use crate::orm::entities::{Apps, AppModel, AppActiveModel, prelude::*};
use uuid::Uuid;
use shellwego_core::entities::app::AppStatus;

pub struct AppRepository {
    db: DatabaseConnection,
}

impl AppRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn find_by_slug(&self, org_id: Uuid, slug: &str) -> Result<Option<AppModel>, DbErr> {
        Apps::find()
            .filter(Apps::Column::OrganizationId.eq(org_id))
            .filter(Apps::Column::Slug.eq(slug))
            .one(&self.db)
            .await
    }

    pub async fn list_for_org(&self, org_id: Uuid, page: u64, limit: u64) -> Result<Vec<AppModel>, DbErr> {
        Apps::find()
            .filter(Apps::Column::OrganizationId.eq(org_id))
            .paginate(&self.db, limit)
            .fetch_page(page)
            .await
    }

    pub async fn update_status(&self, id: Uuid, status: AppStatus) -> Result<(), DbErr> {
        let app: AppActiveModel = Apps::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(DbErr::RecordNotFound("App not found".into()))?
            .into();

        app.status = sea_orm::ActiveValue::Set(status);
        app.update(&self.db).await?;
        Ok(())
    }
}
