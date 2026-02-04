//! Migration: Create webhooks table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000011_create_webhooks_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create webhooks table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add unique constraint on (organization_id, name)
        // TODO: Add foreign key constraint to organizations
        // TODO: Add index on organization_id
        // TODO: Add index on active
        manager
            .create_table(
                Table::create()
                    .table(Webhooks::Table)
                    .if_not_exists()
                    .col(pk_auto(Webhooks::Id))
                    .col(string(Webhooks::Name))
                    .col(string(Webhooks::Url))
                    .col(json(Webhooks::Events))
                    .col(string_null(Webhooks::Secret))
                    .col(boolean(Webhooks::Active))
                    .col(uuid(Webhooks::OrganizationId))
                    .col(date_time(Webhooks::CreatedAt))
                    .col(date_time(Webhooks::UpdatedAt))
                    .col(date_time_null(Webhooks::LastTriggeredAt))
                    .col(date_time_null(Webhooks::LastSuccessAt))
                    .col(date_time_null(Webhooks::LastFailureAt))
                    .col(integer(Webhooks::FailureCount))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_webhooks_organization")
                            .from(Webhooks::Table, Webhooks::OrganizationId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop webhooks table
        manager
            .drop_table(Table::drop().table(Webhooks::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Webhooks {
    Table,
    Id,
    Name,
    Url,
    Events,
    Secret,
    Active,
    OrganizationId,
    CreatedAt,
    UpdatedAt,
    LastTriggeredAt,
    LastSuccessAt,
    LastFailureAt,
    FailureCount,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}
