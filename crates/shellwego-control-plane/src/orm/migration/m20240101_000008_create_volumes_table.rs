//! Migration: Create volumes table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000008_create_volumes_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create volumes table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add unique constraint on (organization_id, name)
        // TODO: Add foreign key constraint to organizations
        // TODO: Add index on organization_id
        // TODO: Add index on status
        // TODO: Add index on attached_to
        manager
            .create_table(
                Table::create()
                    .table(Volumes::Table)
                    .if_not_exists()
                    .col(pk_auto(Volumes::Id))
                    .col(string(Volumes::Name))
                    .col(string(Volumes::Status))
                    .col(integer(Volumes::SizeGb))
                    .col(integer(Volumes::UsedGb))
                    .col(string(Volumes::VolumeType))
                    .col(string(Volumes::Filesystem))
                    .col(boolean(Volumes::Encrypted))
                    .col(string_null(Volumes::EncryptionKeyId))
                    .col(uuid_null(Volumes::AttachedTo))
                    .col(string_null(Volumes::MountPath))
                    .col(json(Volumes::Snapshots))
                    .col(json(Volumes::BackupPolicy))
                    .col(uuid(Volumes::OrganizationId))
                    .col(date_time(Volumes::CreatedAt))
                    .col(date_time(Volumes::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_volumes_organization")
                            .from(Volumes::Table, Volumes::OrganizationId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop volumes table
        manager
            .drop_table(Table::drop().table(Volumes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Volumes {
    Table,
    Id,
    Name,
    Status,
    SizeGb,
    UsedGb,
    VolumeType,
    Filesystem,
    Encrypted,
    EncryptionKeyId,
    AttachedTo,
    MountPath,
    Snapshots,
    BackupPolicy,
    OrganizationId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}
