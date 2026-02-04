//! Migration: Create databases table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000005_create_databases_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create databases table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add unique constraint on (organization_id, name)
        // TODO: Add foreign key constraint to organizations
        // TODO: Add index on organization_id
        // TODO: Add index on status
        // TODO: Add index on engine
        manager
            .create_table(
                Table::create()
                    .table(Databases::Table)
                    .if_not_exists()
                    .col(pk_auto(Databases::Id))
                    .col(string(Databases::Name))
                    .col(string(Databases::Engine))
                    .col(string(Databases::Version))
                    .col(string(Databases::Status))
                    .col(json(Databases::Endpoint))
                    .col(json(Databases::Resources))
                    .col(json(Databases::Usage))
                    .col(json(Databases::Ha))
                    .col(json(Databases::BackupConfig))
                    .col(uuid(Databases::OrganizationId))
                    .col(date_time(Databases::CreatedAt))
                    .col(date_time(Databases::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_databases_organization")
                            .from(Databases::Table, Databases::OrganizationId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop databases table
        manager
            .drop_table(Table::drop().table(Databases::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Databases {
    Table,
    Id,
    Name,
    Engine,
    Version,
    Status,
    Endpoint,
    Resources,
    Usage,
    Ha,
    BackupConfig,
    OrganizationId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}
