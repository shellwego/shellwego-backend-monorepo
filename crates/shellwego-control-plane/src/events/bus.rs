//! Event bus abstraction
//!
//! Publishes domain events for async processing.
//! Other services subscribe to react to state changes.
//! Uses in-memory broadcast channel (can be extended to QUIC/Redis later).

use serde::Serialize;
use tokio::sync::broadcast;
use tracing::{info, debug};

use shellwego_core::entities::app::App;
use super::ServiceContext;

const EVENT_BUFFER_SIZE: usize = 1024;

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_BUFFER_SIZE);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    pub async fn publish(&self, event: Event) {
        if let Err(e) = self.sender.send(event.clone()) {
            debug!("Event subscriber lag: {} pending", e);
        }
        debug!("Published event: {}", event.event_type);
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Event {
    pub event_type: &'static str,
    pub payload: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

macro_rules! impl_event {
    ($name:ident, $event_type:expr) => {
        impl EventBus {
            pub async fn $name(&self, app: &App) -> anyhow::Result<()> {
                self.publish(Event {
                    event_type: $event_type,
                    payload: serde_json::json!({
                        "app_id": app.id,
                        "organization_id": app.organization_id,
                        "name": app.name,
                        "image": app.image,
                    }),
                    timestamp: chrono::Utc::now(),
                }).await;
                Ok(())
            }
        }
    };
}

impl_event!(publish_app_created, "app.created");
impl_event!(publish_app_deployed, "app.deployed");

impl EventBus {
    pub async fn publish_app_crashed(
        &self,
        app: &App,
        exit_code: i32,
        logs: &str,
    ) -> anyhow::Result<()> {
        self.publish(Event {
            event_type: "app.crashed",
            payload: serde_json::json!({
                "app_id": app.id,
                "organization_id": app.organization_id,
                "exit_code": exit_code,
                "logs_preview": &logs[..logs.len().min(1000)],
            }),
            timestamp: chrono::Utc::now(),
        }).await;
        Ok(())
    }

    pub async fn publish_node_offline(&self, node_id: uuid::Uuid) -> anyhow::Result<()> {
        self.publish(Event {
            event_type: "node.offline",
            payload: serde_json::json!({ "node_id": node_id }),
            timestamp: chrono::Utc::now(),
        }).await;
        Ok(())
    }
}

pub struct EventConsumer {
    receiver: broadcast::Receiver<Event>,
}

impl EventConsumer {
    pub fn new(bus: &EventBus) -> Self {
        Self {
            receiver: bus.subscribe(),
        }
    }

    pub async fn recv(&mut self) -> Option<Event> {
        self.recv().await.ok()
    }
}
