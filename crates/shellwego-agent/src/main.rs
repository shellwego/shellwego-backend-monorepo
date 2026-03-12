use std::sync::Arc;
use tokio::signal;
use tracing::{error, info};

use shellwego_agent::{
    daemon::Daemon, detect_capabilities, metrics::MetricsCollector, migration::MigrationManager,
    reconciler::Reconciler, snapshot::SnapshotManager, vmm::VmmManager, wasm, AgentConfig, metrics,
};
use shellwego_network::CniNetwork;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    info!("ShellWeGo Agent starting...");

    let config = AgentConfig::load()?;
    let capabilities = detect_capabilities()?;

    let metrics = Arc::new(MetricsCollector::new(config.node_id.unwrap_or_default()));
    let vmm = VmmManager::new(&config, metrics.clone()).await?;

    let _wasm_runtime = wasm::WasmRuntime::new(&wasm::WasmConfig { max_memory_mb: 512 }).await?;
    let network = Arc::new(CniNetwork::new("sw0", "10.0.0.0/16").await?);

    let daemon = Daemon::new(
        config.clone(),
        capabilities,
        vmm.clone(),
        metrics.clone(),
    )
    .await?;

    let reconciler = Reconciler::new(vmm.clone(), network, daemon.state_client());

    let _snapshot_manager = SnapshotManager::new(&config.data_dir).await?;
    let mut migration_manager = MigrationManager::new(&config.data_dir, vmm.clone()).await?;
    let migration_transport = std::sync::Arc::new(shellwego_agent::migration::QuicMigrationTransport::new(9001).await?);
    migration_manager.set_transport(migration_transport);

    let heartbeat_handle = tokio::spawn({
        let daemon = daemon.clone();
        async move {
            if let Err(e) = daemon.heartbeat_loop().await {
                error!("Heartbeat loop failed: {}", e);
            }
        }
    });

    let reconciler_handle = tokio::spawn({
        let reconciler = reconciler.clone();
        async move {
            if let Err(e) = reconciler.run().await {
                error!("Reconciler failed: {}", e);
            }
        }
    });

    let command_handle = tokio::spawn({
        let daemon = daemon.clone();
        async move {
            if let Err(e) = daemon.command_consumer().await {
                error!("Command consumer failed: {}", e);
            }
        }
    });

    let metrics_handle = tokio::spawn({
        let metrics = metrics.clone();
        async move {
            let collection = metrics.run_collection_loop();
            let server = metrics::start_metrics_server(metrics.clone(), 9100);
            
            // Run both collection loop and server
            let res = tokio::try_join!(collection, server);
            if let Err(e) = res {
                error!("Metrics collection failed: {}", e);
            }
        }
    });

    let mut term_signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    tokio::select! {
        _ = signal::ctrl_c() => { info!("Received SIGINT..."); }
        _ = term_signal.recv() => { info!("Received SIGTERM..."); }
    }

    heartbeat_handle.abort();
    reconciler_handle.abort();
    command_handle.abort();
    metrics_handle.abort();

    info!("Agent shutdown complete");
    Ok(())
}