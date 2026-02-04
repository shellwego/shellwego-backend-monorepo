//! Migration: Create secrets table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000007_create_secrets_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create secrets table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add unique constraint on (organization_id, name)
        // TODO: Add foreign key constraint to organizations
        // TODO: Add foreign key constraint to apps (app_id)
        // TODO: Add index on organization_id
        // TODO: Add index on app_id
        // TODO: Add index on scope
        manager
            .create_table(
                Table::create()
                    .table(Secrets::Table)
                    .if_not_exists()
                    .col(pk_auto(Secrets::Id))
                    .col(string(Secrets::Name))
                    .col(string(Secrets::Scope))
                    .col(uuid_null(Secrets::AppId))
                    .col(integer(Secrets::CurrentVersion))
                    .col(json(Secrets::Versions))
                    .col(date_time_null(Secrets::LastUsedAt))
                    .col(date_time_null(Secrets::ExpiresAt))
                    .col(uuid(Secrets::OrganizationId))
                    .col(date_time(Secrets::CreatedAt))
                    .col(date_time(Secrets::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_secrets_organization")
                            .from(Secrets::Table, Secrets::OrganizationId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_secrets_app")
                            .from(Secrets::Table, Secrets::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop secrets table
        manager
            .drop_table(Table::drop().table(Secrets::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Secrets {
    Table,
    Id,
    Name,
    Scope,
    AppId,
    CurrentVersion,
    Versions,
    LastUsedAt,
    ExpiresAt,
    OrganizationId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}
