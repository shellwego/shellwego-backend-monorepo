//! Migration: Create domains table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000006_create_domains_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create domains table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add unique constraint on hostname
        // TODO: Add foreign key constraint to organizations
        // TODO: Add index on organization_id
        // TODO: Add index on status
        manager
            .create_table(
                Table::create()
                    .table(Domains::Table)
                    .if_not_exists()
                    .col(pk_auto(Domains::Id))
                    .col(string(Domains::Hostname))
                    .col(string(Domains::Status))
                    .col(string(Domains::TlsStatus))
                    .col(json(Domains::Certificate))
                    .col(json(Domains::Validation))
                    .col(json(Domains::Routing))
                    .col(json(Domains::Features))
                    .col(uuid(Domains::OrganizationId))
                    .col(date_time(Domains::CreatedAt))
                    .col(date_time(Domains::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_domains_organization")
                            .from(Domains::Table, Domains::OrganizationId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop domains table
        manager
            .drop_table(Table::drop().table(Domains::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Domains {
    Table,
    Id,
    Hostname,
    Status,
    TlsStatus,
    Certificate,
    Validation,
    Routing,
    Features,
    OrganizationId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}
