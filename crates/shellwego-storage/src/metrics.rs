//! Storage metrics collection
//!
//! Polls ZFS pool and volume metrics periodically and exposes them
//! for the observability pipeline (Prometheus, tracing spans).

use crate::zfs::{PoolMetrics, ZfsManager};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct StorageMetrics {
    pool_metrics: Arc<RwLock<Option<PoolMetrics>>>,
    volume_compression: Arc<RwLock<HashMap<String, f64>>>,
}

impl StorageMetrics {
    pub fn new() -> Self {
        Self {
            pool_metrics: Arc::new(RwLock::new(None)),
            volume_compression: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start background metrics collection
    pub async fn start_collection(&self, zfs: Arc<ZfsManager>, interval: Duration) {
        let pool_ref = self.pool_metrics.clone();
        let compression_ref = self.volume_compression.clone();
        let zfs_clone = zfs.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                // Collect pool metrics
                match zfs_clone.get_pool_metrics().await {
                    Ok(metrics) => {
                        info!(
                            "Pool {}: {}GB / {}GB used, {}% fragmented, dedup={:.2}x",
                            metrics.name,
                            metrics.allocated_bytes / (1024 * 1024 * 1024),
                            metrics.size_bytes / (1024 * 1024 * 1024),
                            metrics.fragmentation_percent,
                            metrics.dedup_ratio,
                        );
                        *pool_ref.write().await = Some(metrics);
                    }
                    Err(e) => debug!("Failed to collect pool metrics: {}", e),
                }

                // Collect volume compression ratios
                match zfs_clone.list_volumes().await {
                    Ok(volumes) => {
                        let mut comp = compression_ref.write().await;
                        comp.clear();
                        for vol in &volumes {
                            if vol.compression_ratio > 1.0 {
                                comp.insert(vol.name.clone(), vol.compression_ratio);
                                debug!("Volume {} compression ratio: {:.2}x", vol.name, vol.compression_ratio);
                            }
                        }
                    }
                    Err(e) => debug!("Failed to list volumes for metrics: {}", e),
                }
            }
        });
    }

    /// Get latest pool metrics
    pub async fn pool_metrics(&self) -> Option<PoolMetrics> {
        self.pool_metrics.read().await.clone()
    }

    /// Get compression ratios for all volumes with ratio > 1.0
    pub async fn compression_ratios(&self) -> HashMap<String, f64> {
        self.volume_compression.read().await.clone()
    }
}

impl Default for StorageMetrics {
    fn default() -> Self {
        Self::new()
    }
}
