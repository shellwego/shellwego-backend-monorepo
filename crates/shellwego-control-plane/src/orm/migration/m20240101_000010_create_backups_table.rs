//! Migration: Create backups table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000010_create_backups_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create backups table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add index on resource_type
        // TODO: Add index on resource_id
        // TODO: Add index on status
        // TODO: Add index on created_at
        manager
            .create_table(
                Table::create()
                    .table(Backups::Table)
                    .if_not_exists()
                    .col(pk_auto(Backups::Id))
                    .col(string(Backups::ResourceType))
                    .col(uuid(Backups::ResourceId))
                    .col(string(Backups::Name))
                    .col(string(Backups::Status))
                    .col(big_integer(Backups::SizeBytes))
                    .col(json(Backups::StorageLocation))
                    .col(string(Backups::Checksum))
                    .col(date_time(Backups::CreatedAt))
                    .col(date_time_null(Backups::CompletedAt))
                    .col(date_time_null(Backups::ExpiresAt))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop backups table
        manager
            .drop_table(Table::drop().table(Backups::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Backups {
    Table,
    Id,
    ResourceType,
    ResourceId,
    Name,
    Status,
    SizeBytes,
    StorageLocation,
    Checksum,
    CreatedAt,
    CompletedAt,
    ExpiresAt,
}
