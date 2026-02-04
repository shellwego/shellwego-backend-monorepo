//! Migration: Create deployments table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000009_create_deployments_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create deployments table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add foreign key constraint to apps
        // TODO: Add foreign key constraint to users (created_by)
        // TODO: Add foreign key constraint to deployments (previous_deployment_id)
        // TODO: Add index on app_id
        // TODO: Add index on status
        // TODO: Add index on created_by
        manager
            .create_table(
                Table::create()
                    .table(Deployments::Table)
                    .if_not_exists()
                    .col(pk_auto(Deployments::Id))
                    .col(uuid(Deployments::AppId))
                    .col(string(Deployments::Version))
                    .col(json(Deployments::Spec))
                    .col(string(Deployments::State))
                    .col(string_null(Deployments::Message))
                    .col(uuid_null(Deployments::PreviousDeploymentId))
                    .col(uuid(Deployments::CreatedBy))
                    .col(date_time(Deployments::CreatedAt))
                    .col(date_time(Deployments::UpdatedAt))
                    .col(date_time_null(Deployments::CompletedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_deployments_app")
                            .from(Deployments::Table, Deployments::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_deployments_created_by")
                            .from(Deployments::Table, Deployments::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_deployments_previous")
                            .from(Deployments::Table, Deployments::PreviousDeploymentId)
                            .to(Deployments::Table, Deployments::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop deployments table
        manager
            .drop_table(Table::drop().table(Deployments::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Deployments {
    Table,
    Id,
    AppId,
    Version,
    Spec,
    State,
    Message,
    PreviousDeploymentId,
    CreatedBy,
    CreatedAt,
    UpdatedAt,
    CompletedAt,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
