//! Migration: Create organizations table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000001_create_organizations_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create organizations table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add unique constraint on slug
        // TODO: Add unique constraint on billing_email
        // TODO: Add index on plan
        // TODO: Add index on created_at
        // TODO: Add foreign key constraints (if needed)
        manager
            .create_table(
                Table::create()
                    .table(Organizations::Table)
                    .if_not_exists()
                    .col(pk_auto(Organizations::Id))
                    .col(string(Organizations::Name))
                    .col(string(Organizations::Slug))
                    .col(string(Organizations::Plan))
                    .col(json(Organizations::Settings))
                    .col(string(Organizations::BillingEmail))
                    .col(date_time(Organizations::CreatedAt))
                    .col(date_time(Organizations::UpdatedAt))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop organizations table
        // TODO: Cascade delete related records (or handle manually)
        manager
            .drop_table(Table::drop().table(Organizations::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
    Name,
    Slug,
    Plan,
    Settings,
    BillingEmail,
    CreatedAt,
    UpdatedAt,
}
