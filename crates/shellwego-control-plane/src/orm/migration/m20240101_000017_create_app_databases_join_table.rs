//! Migration: Create app_databases join table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000017_create_app_databases_join_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create app_databases join table
        // TODO: Add primary key constraint on (app_id, database_id)
        // TODO: Add foreign key constraint to apps
        // TODO: Add foreign key constraint to databases
        // TODO: Add index on app_id
        // TODO: Add index on database_id
        manager
            .create_table(
                Table::create()
                    .table(AppDatabases::Table)
                    .if_not_exists()
                    .col(uuid(AppDatabases::AppId))
                    .col(uuid(AppDatabases::DatabaseId))
                    .col(string(AppDatabases::ConnectionName))
                    .col(json(AppDatabases::ConnectionConfig))
                    .col(date_time(AppDatabases::CreatedAt))
                    .primary_key(
                        PrimaryKey::new()
                            .col(AppDatabases::AppId)
                            .col(AppDatabases::DatabaseId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_databases_app")
                            .from(AppDatabases::Table, AppDatabases::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_databases_database")
                            .from(AppDatabases::Table, AppDatabases::DatabaseId)
                            .to(Databases::Table, Databases::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop app_databases join table
        manager
            .drop_table(Table::drop().table(AppDatabases::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum AppDatabases {
    Table,
    AppId,
    DatabaseId,
    ConnectionName,
    ConnectionConfig,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Databases {
    Table,
    Id,
}
