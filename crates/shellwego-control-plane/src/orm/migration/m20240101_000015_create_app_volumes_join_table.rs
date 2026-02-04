//! Migration: Create app_volumes join table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000015_create_app_volumes_join_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create app_volumes join table
        // TODO: Add primary key constraint on (app_id, volume_id)
        // TODO: Add foreign key constraint to apps
        // TODO: Add foreign key constraint to volumes
        // TODO: Add index on app_id
        // TODO: Add index on volume_id
        manager
            .create_table(
                Table::create()
                    .table(AppVolumes::Table)
                    .if_not_exists()
                    .col(uuid(AppVolumes::AppId))
                    .col(uuid(AppVolumes::VolumeId))
                    .col(string(AppVolumes::MountPath))
                    .col(boolean(AppVolumes::ReadOnly))
                    .col(date_time(AppVolumes::CreatedAt))
                    .primary_key(
                        PrimaryKey::new()
                            .col(AppVolumes::AppId)
                            .col(AppVolumes::VolumeId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_volumes_app")
                            .from(AppVolumes::Table, AppVolumes::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_volumes_volume")
                            .from(AppVolumes::Table, AppVolumes::VolumeId)
                            .to(Volumes::Table, Volumes::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop app_volumes join table
        manager
            .drop_table(Table::drop().table(AppVolumes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum AppVolumes {
    Table,
    AppId,
    VolumeId,
    MountPath,
    ReadOnly,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Volumes {
    Table,
    Id,
}
