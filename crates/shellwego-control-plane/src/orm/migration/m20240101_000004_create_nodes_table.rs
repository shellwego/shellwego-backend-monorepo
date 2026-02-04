//! Migration: Create nodes table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000004_create_nodes_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create nodes table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add unique constraint on hostname
        // TODO: Add foreign key constraint to organizations
        // TODO: Add index on organization_id
        // TODO: Add index on status
        // TODO: Add index on region
        manager
            .create_table(
                Table::create()
                    .table(Nodes::Table)
                    .if_not_exists()
                    .col(pk_auto(Nodes::Id))
                    .col(string(Nodes::Hostname))
                    .col(string(Nodes::Status))
                    .col(string(Nodes::Region))
                    .col(string(Nodes::Zone))
                    .col(json(Nodes::Capacity))
                    .col(json(Nodes::Capabilities))
                    .col(json(Nodes::Network))
                    .col(json(Nodes::Labels))
                    .col(json(Nodes::RunningApps))
                    .col(integer(Nodes::MicrovmCapacity))
                    .col(integer(Nodes::MicrovmUsed))
                    .col(string(Nodes::KernelVersion))
                    .col(string(Nodes::FirecrackerVersion))
                    .col(string(Nodes::AgentVersion))
                    .col(date_time_null(Nodes::LastSeen))
                    .col(date_time(Nodes::CreatedAt))
                    .col(uuid(Nodes::OrganizationId))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_nodes_organization")
                            .from(Nodes::Table, Nodes::OrganizationId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop nodes table
        manager
            .drop_table(Table::drop().table(Nodes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Nodes {
    Table,
    Id,
    Hostname,
    Status,
    Region,
    Zone,
    Capacity,
    Capabilities,
    Network,
    Labels,
    RunningApps,
    MicrovmCapacity,
    MicrovmUsed,
    KernelVersion,
    FirecrackerVersion,
    AgentVersion,
    LastSeen,
    CreatedAt,
    OrganizationId,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}
