//! Application state shared across all request handlers
//! 
//! Contains the hot path: DB pool, NATS client, scheduler handle

use std::sync::Arc;
use sqlx::{Pool, Postgres, Sqlite};
use async_nats::Client as NatsClient;
use crate::config::Config;

// TODO: Support both Postgres (HA) and SQLite (single-node) via enum or generic

pub struct AppState {
    pub config: Config,
    pub db: DatabasePool,
    pub nats: Option<NatsClient>,
    // TODO: Add scheduler handle
    // TODO: Add cache layer (Redis or in-memory)
    // TODO: Add metrics registry
}

pub enum DatabasePool {
    Postgres(Pool<Postgres>),
    Sqlite(Pool<Sqlite>),
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        // Initialize database pool
        let db = if config.database_url.starts_with("postgres://") {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(20)
                .connect(&config.database_url)
                .await?;
            DatabasePool::Postgres(pool)
        } else {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(5)
                .connect(&config.database_url)
                .await?;
            DatabasePool::Sqlite(pool)
        };
        
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
    
    // TODO: Add helper methods for common DB operations
    // TODO: Add transaction helper with retry logic
}

// Axum extractor impl
impl axum::extract::FromRef<Arc<AppState>> for Arc<AppState> {
    fn from_ref(state: &Arc<AppState>) -> Arc<AppState> {
        state.clone()
    }
}