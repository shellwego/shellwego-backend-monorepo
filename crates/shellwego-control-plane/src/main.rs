use std::net::SocketAddr;
use tracing::{info, warn, error};
use shellwego_network::{QuinnServer, QuicConfig, Message};

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
    tracing_subscriber::fmt::init();

    info!("Starting ShellWeGo Control Plane v{}", env!("CARGO_PKG_VERSION"));

    let config = Config::load()?;
    info!("Configuration loaded: serving on {}", config.bind_addr);

    let state = AppState::new(config).await?;
    info!("State initialized successfully");

    let quic_state = state.clone();
    tokio::spawn(async move {
        let quic_conf = QuicConfig::default();
        let server = QuinnServer::new(quic_conf).await.expect("Failed to create QUIC server");
        let listener = server.bind("0.0.0.0:4433").await.expect("Failed to bind QUIC");

        info!("QUIC Mesh listening on :4433");

        loop {
            match listener.accept().await {
                Ok(mut conn) => {
                    let inner_state = quic_state.clone();
                    tokio::spawn(async move {
                        if let Ok(Message::Register { hostname, .. }) = conn.receive().await {
                            let node_id = uuid::Uuid::new_v4();
                            info!("Node {} ({}) registered via QUIC", hostname, node_id);
                            conn.set_node_id(node_id);
                            conn.set_hostname(hostname.clone());
                            inner_state.agents.insert(node_id, conn);
                        }
                    });
                }
                Err(e) => error!("QUIC accept error: {}", e),
            }
        }
    });

    let app = api::create_router(state);

    let addr: SocketAddr = state.config.bind_addr.parse()?;
    info!("API Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
