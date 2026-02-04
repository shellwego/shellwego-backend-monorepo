//! Migration: Create webhook_deliveries table

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20240101_000014_create_webhook_deliveries_table"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Create webhook_deliveries table with all columns
        // TODO: Add primary key constraint on id
        // TODO: Add foreign key constraint to webhooks
        // TODO: Add index on webhook_id
        // TODO: Add index on status
        // TODO: Add index on created_at
        manager
            .create_table(
                Table::create()
                    .table(WebhookDeliveries::Table)
                    .if_not_exists()
                    .col(pk_auto(WebhookDeliveries::Id))
                    .col(uuid(WebhookDeliveries::WebhookId))
                    .col(string(WebhookDeliveries::EventType))
                    .col(json(WebhookDeliveries::Payload))
                    .col(string(WebhookDeliveries::Status))
                    .col(integer(WebhookDeliveries::StatusCode))
                    .col(string_null(WebhookDeliveries::ErrorMessage))
                    .col(integer(WebhookDeliveries::Attempts))
                    .col(date_time(WebhookDeliveries::CreatedAt))
                    .col(date_time_null(WebhookDeliveries::DeliveredAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_webhook_deliveries_webhook")
                            .from(WebhookDeliveries::Table, WebhookDeliveries::WebhookId)
                            .to(Webhooks::Table, Webhooks::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO: Drop webhook_deliveries table
        manager
            .drop_table(Table::drop().table(WebhookDeliveries::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum WebhookDeliveries {
    Table,
    Id,
    WebhookId,
    EventType,
    Payload,
    Status,
    StatusCode,
    ErrorMessage,
    Attempts,
    CreatedAt,
    DeliveredAt,
}

#[derive(DeriveIden)]
enum Webhooks {
    Table,
    Id,
}
