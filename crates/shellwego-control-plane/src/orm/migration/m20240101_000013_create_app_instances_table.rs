//! Migration: Create app_instances table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000013_create_app_instances_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create app_instances table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add unique constraint on (app_id, instance_id)
        // TODO: Add foreign key constraint to apps
        // TODO: Add foreign key constraint to nodes
        // TODO: Add foreign key constraint to deployments
        // TODO: Add index on app_id
        // TODO: Add index on node_id
        // TODO: Add index on status
        manager
            .create_table(
                Table::create()
                    .table(AppInstances::Table)
                    .if_not_exists()
                    .col(pk_auto(AppInstances::Id))
                    .col(uuid(AppInstances::AppId))
                    .col(string(AppInstances::InstanceId))
                    .col(uuid(AppInstances::NodeId))
                    .col(uuid_null(AppInstances::DeploymentId))
                    .col(string(AppInstances::Status))
                    .col(json(AppInstances::Resources))
                    .col(json(AppInstances::Network))
                    .col(json(AppInstances::Health))
                    .col(date_time(AppInstances::CreatedAt))
                    .col(date_time(AppInstances::UpdatedAt))
                    .col(date_time_null(AppInstances::StartedAt))
                    .col(date_time_null(AppInstances::StoppedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_instances_app")
                            .from(AppInstances::Table, AppInstances::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_instances_node")
                            .from(AppInstances::Table, AppInstances::NodeId)
                            .to(Nodes::Table, Nodes::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_instances_deployment")
                            .from(AppInstances::Table, AppInstances::DeploymentId)
                            .to(Deployments::Table, Deployments::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop app_instances table
        manager
            .drop_table(Table::drop().table(AppInstances::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum AppInstances {
    Table,
    Id,
    AppId,
    InstanceId,
    NodeId,
    DeploymentId,
    Status,
    Resources,
    Network,
    Health,
    CreatedAt,
    UpdatedAt,
    StartedAt,
    StoppedAt,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Nodes {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Deployments {
    Table,
    Id,
}
