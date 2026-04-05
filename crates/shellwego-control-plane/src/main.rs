//! ShellWeGo Control Plane
//!
//! The central orchestration service for the ShellWeGo platform.
//! Provides REST API, scheduling, cluster state management, and
//! coordination of worker nodes.

use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{filter::LevelFilter, fmt};

// Module declarations
mod api;
mod config;
mod state;
mod services;
mod operators;
mod git;
mod kms;
mod federation;
mod orm;
mod auth;

use crate::config::Config;
use crate::state::AppState;
use crate::orm::{Database, DatabaseConfig as OrmDatabaseConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    fmt()
        .with_max_level(LevelFilter::INFO)
        .init();

    info!("Starting ShellWeGo Control Plane v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = Config::load()?;
    info!("Configuration loaded: serving on {}", config.bind_addr);

    // Convert config::DatabaseConfig to orm::DatabaseConfig
    let orm_db_config = OrmDatabaseConfig {
        url: config.database.url.clone(),
        max_connections: config.database.max_connections,
        min_connections: config.database.min_connections,
        connect_timeout_secs: config.database.connect_timeout_secs,
        idle_timeout_secs: 600,
        logging: config.database.logging,
        auto_migrate: config.database.auto_migrate,
    };

    // Initialize database
    let database = Arc::new(Database::new(orm_db_config).await?);
    info!("Database connection established");

    // Run migrations if enabled
    if config.database.auto_migrate {
        database.migrate().await?;
        info!("Database migrations completed");
    }

    // Initialize application state
    let state = AppState::new(config.clone(), database).await?;
    info!("Application state initialized");

    // Create API router
    let app = api::create_router(state);

    // Start server
    let addr: SocketAddr = config.bind_addr.parse()?;
    info!("API Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
