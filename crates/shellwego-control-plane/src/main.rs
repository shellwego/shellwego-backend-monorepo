//! ShellWeGo Control Plane
//! 
//! The brain. HTTP API + Scheduler + State management.
//! Runs on the control plane nodes, talks to agents over NATS.

use std::net::SocketAddr;
use tracing::{info, warn};

mod api;
mod config;
mod orm;
mod events;
mod services;
mod state;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // TODO: Initialize tracing with JSON subscriber for production
    tracing_subscriber::fmt::init();
    
    info!("Starting ShellWeGo Control Plane v{}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration from env + file
    let config = Config::load()?;
    info!("Configuration loaded: serving on {}", config.bind_addr);
    
    // Initialize application state (DB pool, NATS conn, etc)
    let state = AppState::new(config).await?;
    info!("State initialized successfully");
    
    // Build router with all routes
    let app = api::create_router(state);
    
    // Bind and serve
    let addr: SocketAddr = state.config.bind_addr.parse()?;
    info!("Control plane listening on http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}