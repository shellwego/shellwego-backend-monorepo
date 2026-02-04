//! Event bus abstraction
//! 
//! Publishes domain events to NATS for async processing.
//! Other services subscribe to react to state changes.

use async_nats::Client;
use serde::Serialize;
use tracing::{info, debug, error};

use shellwego_core::entities::app::App;
use super::ServiceContext;

/// Event bus publisher
#[derive(Clone)]
pub struct EventBus {
    nats: Option<Client>,
    // TODO: Add fallback to in-memory channel if NATS unavailable
}

impl EventBus {
    pub fn new(nats: Option<Client>) -> Self {
        Self { nats }
    }

    // === App Events ===

    pub async fn publish_app_created(&self, app: &App) -> anyhow::Result<()> {
        self.publish("apps.created", AppEvent {
            event_type: "app.created",
            app_id: app.id,
            organization_id: app.organization_id,
            timestamp: chrono::Utc::now(),
            payload: serde_json::json!({
                "name": app.name,
                "image": app.image,
            }),
        }).await
    }

    pub async fn publish_app_deployed(&self, app: &App) -> anyhow::Result<()> {
        self.publish("apps.deployed", AppEvent {
            event_type: "app.deployed",
            app_id: app.id,
            organization_id: app.organization_id,
            timestamp: chrono::Utc::now(),
            payload: serde_json::json!({
                "status": app.status,
            }),
        }).await
    }

    pub async fn publish_app_crashed(
        &self,
        app: &App,
        exit_code: i32,
        logs: &str,
    ) -> anyhow::Result<()> {
        self.publish("apps.crashed", AppEvent {
            event_type: "app.crashed",
            app_id: app.id,
            organization_id: app.organization_id,
            timestamp: chrono::Utc::now(),
            payload: serde_json::json!({
                "exit_code": exit_code,
                "logs_preview": &logs[..logs.len().min(1000)],
            }),
        }).await
    }

    // === Deployment Events ===

    pub async fn publish_deployment_started(
        &self,
        spec: &crate::services::deployment::DeploymentSpec,
    ) -> anyhow::Result<()> {
        self.publish("deployments.started", DeploymentEvent {
            event_type: "deployment.started",
            deployment_id: spec.deployment_id,
            app_id: spec.app_id,
            timestamp: chrono::Utc::now(),
            strategy: format!("{:?}", spec.strategy),
        }).await
    }

    pub async fn publish_deployment_succeeded(
        &self,
        spec: &crate::services::deployment::DeploymentSpec,
    ) -> anyhow::Result<()> {
        self.publish("deployments.succeeded", DeploymentEvent {
            event_type: "deployment.succeeded",
            deployment_id: spec.deployment_id,
            app_id: spec.app_id,
            timestamp: chrono::Utc::now(),
            strategy: format!("{:?}", spec.strategy),
        }).await
    }

    pub async fn publish_deployment_failed(
        &self,
        spec: &crate::services::deployment::DeploymentSpec,
        error: &str,
    ) -> anyhow::Result<()> {
        self.publish("deployments.failed", DeploymentFailedEvent {
            event_type: "deployment.failed",
            deployment_id: spec.deployment_id,
            app_id: spec.app_id,
            timestamp: chrono::Utc::now(),
            error: error.to_string(),
        }).await
    }

    pub async fn publish_rollback_completed(
        &self,
        app_id: uuid::Uuid,
        from_deployment: uuid::Uuid,
    ) -> anyhow::Result<()> {
        self.publish("deployments.rollback", RollbackEvent {
            event_type: "deployment.rollback",
            app_id,
            from_deployment,
            timestamp: chrono::Utc::now(),
        }).await
    }

    // === Node Events ===

    pub async fn publish_node_offline(&self, node_id: uuid::Uuid) -> anyhow::Result<()> {
        self.publish("nodes.offline", NodeEvent {
            event_type: "node.offline",
            node_id,
            timestamp: chrono::Utc::now(),
        }).await
    }

    // === Internal ===

    async fn publish<T: Serialize>(
        &self,
        subject: &str,
        payload: T,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_vec(&payload)?;
        
        if let Some(ref nats) = self.nats {
            nats.publish(subject.to_string(), json.into()).await?;
            debug!("Published to {}: {} bytes", subject, json.len());
        } else {
            // TODO: Buffer to memory or disk for later delivery
            debug!("NATS unavailable, event dropped: {}", subject);
        }
        
        Ok(())
    }
}

// === Event Schemas ===

#[derive(Serialize)]
struct AppEvent {
    event_type: &'static str,
    app_id: uuid::Uuid,
    organization_id: uuid::Uuid,
    timestamp: chrono::DateTime<chrono::Utc>,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct DeploymentEvent {
    event_type: &'static str,
    deployment_id: uuid::Uuid,
    app_id: uuid::Uuid,
    timestamp: chrono::DateTime<chrono::Utc>,
    strategy: String,
}

#[derive(Serialize)]
struct DeploymentFailedEvent {
    event_type: &'static str,
    deployment_id: uuid::Uuid,
    app_id: uuid::Uuid,
    timestamp: chrono::DateTime<chrono::Utc>,
    error: String,
}

#[derive(Serialize)]
struct RollbackEvent {
    event_type: &'static str,
    app_id: uuid::Uuid,
    from_deployment: uuid::Uuid,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct NodeEvent {
    event_type: &'static str,
    node_id: uuid::Uuid,
    timestamp: chrono::DateTime<chrono::Utc>,
}

/// Event consumer / subscriber
pub struct EventConsumer {
    // TODO: NATS subscription management
    // TODO: Durable consumer for exactly-once processing
    // TODO: Dead letter queue for failed events
}

impl EventConsumer {
    pub async fn subscribe_app_events(&self) -> anyhow::Result<()> {
        // TODO: Subscribe to "apps.>"
        // TODO: Route to appropriate handler based on event_type
        // TODO: Update read models, trigger webhooks, send notifications
        
        Ok(())
    }
}