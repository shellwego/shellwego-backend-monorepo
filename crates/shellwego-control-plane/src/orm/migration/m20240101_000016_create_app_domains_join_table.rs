//! Migration: Create app_domains join table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000016_create_app_domains_join_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create app_domains join table
        // TODO: Add primary key constraint on (app_id, domain_id)
        // TODO: Add foreign key constraint to apps
        // TODO: Add foreign key constraint to domains
        // TODO: Add index on app_id
        // TODO: Add index on domain_id
        manager
            .create_table(
                Table::create()
                    .table(AppDomains::Table)
                    .if_not_exists()
                    .col(uuid(AppDomains::AppId))
                    .col(uuid(AppDomains::DomainId))
                    .col(json(AppDomains::RoutingConfig))
                    .col(date_time(AppDomains::CreatedAt))
                    .primary_key(
                        PrimaryKey::new()
                            .col(AppDomains::AppId)
                            .col(AppDomains::DomainId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_domains_app")
                            .from(AppDomains::Table, AppDomains::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_domains_domain")
                            .from(AppDomains::Table, AppDomains::DomainId)
                            .to(Domains::Table, Domains::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop app_domains join table
        manager
            .drop_table(Table::drop().table(AppDomains::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum AppDomains {
    Table,
    AppId,
    DomainId,
    RoutingConfig,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Domains {
    Table,
    Id,
}
