//! Migration: Create apps table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000003_create_apps_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create apps table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add unique constraint on (organization_id, slug)
        // TODO: Add foreign key constraint to organizations
        // TODO: Add foreign key constraint to users (created_by)
        // TODO: Add index on organization_id
        // TODO: Add index on status
        // TODO: Add index on created_by
        manager
            .create_table(
                Table::create()
                    .table(Apps::Table)
                    .if_not_exists()
                    .col(pk_auto(Apps::Id))
                    .col(string(Apps::Name))
                    .col(string(Apps::Slug))
                    .col(string(Apps::Status))
                    .col(string(Apps::Image))
                    .col(json_null(Apps::Command))
                    .col(json(Apps::Resources))
                    .col(json(Apps::Env))
                    .col(json(Apps::Domains))
                    .col(json(Apps::Volumes))
                    .col(json_null(Apps::HealthCheck))
                    .col(json(Apps::Source))
                    .col(uuid(Apps::OrganizationId))
                    .col(uuid(Apps::CreatedBy))
                    .col(date_time(Apps::CreatedAt))
                    .col(date_time(Apps::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_apps_organization")
                            .from(Apps::Table, Apps::OrganizationId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_apps_created_by")
                            .from(Apps::Table, Apps::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop apps table
        manager
            .drop_table(Table::drop().table(Apps::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
    Name,
    Slug,
    Status,
    Image,
    Command,
    Resources,
    Env,
    Domains,
    Volumes,
    HealthCheck,
    Source,
    OrganizationId,
    CreatedBy,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
