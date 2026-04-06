//! Configuration file watcher for hot-reload
//!
//! Watches a configuration file for changes using the `notify` crate and
//! triggers a route reload when the file is modified. Includes debouncing
//! to prevent reload storms from rapid file saves.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::router::Route;
use crate::EdgeError;

/// Debounce interval between file change events and actual reload (in ms).
const DEBOUNCE_MS: u64 = 500;

/// Watch a configuration file for changes and trigger reload.
///
/// Spawns a background tokio task that listens for filesystem events from
/// the `notify` crate. When a file modification is detected (with debouncing),
/// the file is read, parsed as a JSON array of routes, and the callback
/// is invoked with the new route list.
///
/// The returned `notify::RecommendedWatcher` must be kept alive for the
/// duration of the watch. Dropping it stops the file watcher.
pub fn watch_config_file(
    path: &str,
    router: Arc<RwLock<crate::router::Router>>,
) -> Result<notify::RecommendedWatcher, EdgeError> {
    let path = Path::new(path).to_path_buf();

    if !path.exists() {
        return Err(EdgeError::ConfigError(format!(
            "Config file does not exist: {}",
            path.display()
        )));
    }

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();

    let mut watcher = notify::recommended_watcher(tx).map_err(|e| {
        EdgeError::ConfigError(format!("Failed to create file watcher: {}", e))
    })?;

    watcher
        .watch(&path, notify::RecursiveMode::NonRecursive)
        .map_err(|e| {
            EdgeError::ConfigError(format!("Failed to watch config file: {}", e))
        })?;

    info!("Watching config file for hot-reload: {}", path.display());

    // Spawn a tokio task to process filesystem events
    tokio::spawn(async move {
        process_file_events(path, rx, router).await;
    });

    Ok(watcher)
}

/// Process filesystem events from the notify channel.
async fn process_file_events(
    path: PathBuf,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    router: Arc<RwLock<crate::router::Router>>,
) {
    let mut last_reload = Instant::now();

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                debug!("File event: kind={:?}, paths={:?}", event.kind, event.paths);

                // Only react to modify/create events
                if !event.kind.is_modify() && !event.kind.is_create() {
                    continue;
                }

                // Debounce: ignore events that arrive too quickly after the last reload
                if last_reload.elapsed() < Duration::from_millis(DEBOUNCE_MS) {
                    debug!("Debouncing config reload ({}ms since last reload)", last_reload.elapsed().as_millis());
                    continue;
                }

                // Perform the reload
                match reload_routes_from_file(&path, &router).await {
                    Ok(()) => {
                        last_reload = Instant::now();
                    }
                    Err(e) => {
                        error!("Failed to reload config: {}", e);
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("File watch error: {}", e);
            }
            Err(_) => {
                // Channel closed — watcher dropped
                info!("Config file watcher channel closed, stopping");
                break;
            }
        }
    }
}

/// Read and parse the config file, then update the router.
async fn reload_routes_from_file(
    path: &Path,
    router: &Arc<RwLock<crate::router::Router>>,
) -> Result<(), EdgeError> {
    info!("Config file changed, reloading: {}", path.display());

    let content = tokio::fs::read_to_string(path).await.map_err(|e| {
        EdgeError::ConfigError(format!("Failed to read config file: {}", e))
    })?;

    let routes: Vec<Route> = serde_json::from_str(&content).map_err(|e| {
        EdgeError::ConfigError(format!("Failed to parse config JSON: {}", e))
    })?;

    info!("Parsed {} routes from config file", routes.len());

    // Update the router
    let mut router_guard = router.write().await;
    router_guard.clear();

    for route in routes {
        router_guard.add_route(route)?;
    }

    info!("Config reloaded successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_nonexistent_file() {
        let router = Arc::new(RwLock::new(crate::router::Router::new()));
        let result = watch_config_file("/nonexistent/path/config.json", router);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not exist"));
    }
}
