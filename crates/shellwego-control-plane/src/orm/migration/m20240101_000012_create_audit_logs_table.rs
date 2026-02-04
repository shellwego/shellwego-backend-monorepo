//! Migration: Create audit_logs table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000012_create_audit_logs_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create audit_logs table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add foreign key constraint to users
        // TODO: Add foreign key constraint to organizations
        // TODO: Add index on user_id
        // TODO: Add index on organization_id
        // TODO: Add index on action
        // TODO: Add index on resource_type
        // TODO: Add index on created_at
        manager
            .create_table(
                Table::create()
                    .table(AuditLogs::Table)
                    .if_not_exists()
                    .col(pk_auto(AuditLogs::Id))
                    .col(uuid_null(AuditLogs::UserId))
                    .col(uuid_null(AuditLogs::OrganizationId))
                    .col(string(AuditLogs::Action))
                    .col(string(AuditLogs::ResourceType))
                    .col(uuid_null(AuditLogs::ResourceId))
                    .col(json(AuditLogs::Details))
                    .col(string_null(AuditLogs::IpAddress))
                    .col(string_null(AuditLogs::UserAgent))
                    .col(boolean(AuditLogs::Success))
                    .col(string_null(AuditLogs::ErrorMessage))
                    .col(date_time(AuditLogs::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_audit_logs_user")
                            .from(AuditLogs::Table, AuditLogs::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_audit_logs_organization")
                            .from(AuditLogs::Table, AuditLogs::OrganizationId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop audit_logs table
        manager
            .drop_table(Table::drop().table(AuditLogs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum AuditLogs {
    Table,
    Id,
    UserId,
    OrganizationId,
    Action,
    ResourceType,
    ResourceId,
    Details,
    IpAddress,
    UserAgent,
    Success,
    ErrorMessage,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}
