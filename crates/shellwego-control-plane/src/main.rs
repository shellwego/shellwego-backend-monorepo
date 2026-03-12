//! ShellWeGo Control Plane
//!
//! The central orchestration service for the ShellWeGo platform.
//! Provides REST API, scheduling, cluster state management, and
//! coordination of worker nodes.

use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

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

use crate::config::Config;
use crate::state::AppState;
use crate::orm::Database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .init();

    info!("Starting ShellWeGo Control Plane v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = Config::load()?;
    info!("Configuration loaded: serving on {}", config.bind_addr);

    // Initialize database
    let database = Arc::new(Database::new(config.database.clone()).await?);
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
