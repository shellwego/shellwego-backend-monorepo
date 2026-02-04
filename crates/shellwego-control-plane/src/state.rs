//! Application state shared across all request handlers
//!
//! Contains the hot path: ORM database, NATS client, scheduler handle

use std::sync::Arc;
use async_nats::Client as NatsClient;
use crate::config::Config;
use crate::orm::OrmDatabase;

// TODO: Support both Postgres (HA) and SQLite (single-node) via sea-orm

pub struct AppState {
    pub config: Config,
    pub db: Arc<OrmDatabase>,
    pub nats: Option<NatsClient>,
    // TODO: Add scheduler handle
    // TODO: Add cache layer (Redis or in-memory)
    // TODO: Add metrics registry
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        // TODO: Initialize ORM database connection
        // TODO: Run migrations on startup
        let db = Arc::new(OrmDatabase::connect(&config.database_url).await?);

        // TODO: Run migrations
        // db.migrate().await?;

        // Initialize NATS connection if configured
        let nats = if let Some(ref url) = config.nats_url {
            Some(async_nats::connect(url).await?)
        } else {
            None
        };

        Ok(Arc::new(Self {
            config,
            db,
            nats,
        }))
    }

    // TODO: Add helper methods for common ORM operations
    // TODO: Add transaction helper with retry logic
}

// Axum extractor impl
impl axum::extract::FromRef<Arc<AppState>> for Arc<AppState> {
    fn from_ref(state: &Arc<AppState>) -> Arc<AppState> {
        state.clone()
    }
}