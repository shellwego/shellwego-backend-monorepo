//! Migration: Create users table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000002_create_users_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create users table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add unique constraint on email
        // TODO: Add foreign key constraint to organizations
        // TODO: Add index on organization_id
        // TODO: Add index on email
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(pk_auto(Users::Id))
                    .col(string(Users::Email))
                    .col(string(Users::PasswordHash))
                    .col(string(Users::Name))
                    .col(string(Users::Role))
                    .col(uuid(Users::OrganizationId))
                    .col(date_time(Users::CreatedAt))
                    .col(date_time(Users::UpdatedAt))
                    .col(date_time_null(Users::LastLoginAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_users_organization")
                            .from(Users::Table, Users::OrganizationId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop users table
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Email,
    PasswordHash,
    Name,
    Role,
    OrganizationId,
    CreatedAt,
    UpdatedAt,
    LastLoginAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}
