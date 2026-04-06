//! Content-addressable garbage collection with per-layer ref-counting.
//!
//! Implements a GC system that tracks how many images reference each layer,
//! allowing safe cleanup of unused layers while preserving shared layers
//! that are still in use by other images.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::cache::{CachedImageInfo, CacheStats, LayerCache};
use crate::RegistryError;

/// GC configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcConfig {
    /// Maximum cache size in bytes (0 = unlimited).
    pub max_size_bytes: u64,
    /// Target cache utilization (0.0 - 1.0). GC runs when usage exceeds this.
    pub high_watermark: f64,
    /// Stop GC when usage drops below this (0.0 - 1.0).
    pub low_watermark: f64,
    /// Minimum age of image in hours before eligible for GC.
    pub min_age_hours: u64,
    /// Maximum images to keep (0 = unlimited).
    pub max_images: usize,
    /// Whether to preserve images that are currently running.
    pub preserve_running: bool,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            max_size_bytes: 50 * 1024 * 1024 * 1024, // 50 GB
            high_watermark: 0.85,
            low_watermark: 0.70,
            min_age_hours: 24,
            max_images: 100,
            preserve_running: true,
        }
    }
}

/// Per-layer reference count tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerRefCount {
    /// Layer digest.
    pub digest: String,
    /// Number of images referencing this layer.
    pub ref_count: u64,
    /// Estimated size in bytes.
    pub size: u64,
    /// When this layer was first cached.
    pub created_at: DateTime<Utc>,
}

/// GC result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcResult {
    /// Number of images removed.
    pub images_removed: usize,
    /// Number of layers freed.
    pub layers_freed: usize,
    /// Bytes freed.
    pub bytes_freed: u64,
    /// Duration of GC run in seconds.
    pub duration_secs: f64,
    /// Whether this was a dry-run (no actual deletions).
    pub dry_run: bool,
}

/// Content-addressable garbage collector.
///
/// Tracks per-layer reference counts across all cached images and removes
/// images that exceed the configured thresholds while preserving shared layers.
pub struct GarbageCollector {
    /// Reference to the layer cache.
    cache: Arc<LayerCache>,
    /// GC configuration.
    config: GcConfig,
    /// Layer reference counts (digest → ref count info).
    layer_refs: Arc<RwLock<HashMap<String, LayerRefCount>>>,
}

impl GarbageCollector {
    /// Create a new garbage collector.
    ///
    /// # Arguments
    /// * `cache` - Layer cache to manage
    /// * `config` - GC configuration
    pub fn new(cache: Arc<LayerCache>, config: GcConfig) -> Self {
        Self {
            cache,
            config,
            layer_refs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Rebuild reference counts from current cached images.
    ///
    /// Walks all cached images and builds a map of layer digest → reference count.
    pub async fn rebuild_ref_counts(&self) -> Result<(), RegistryError> {
        let images = self.cache.list_images().await;
        let mut counts: HashMap<String, LayerRefCount> = HashMap::new();

        for image in &images {
            let digest = &image.digest;
            if !digest.is_empty() {
                let entry = counts.entry(digest.clone()).or_insert_with(|| {
                    let size = if image.layer_count > 0 {
                        image.size_bytes / image.layer_count as u64
                    } else {
                        image.size_bytes
                    };
                    LayerRefCount {
                        digest: digest.clone(),
                        ref_count: 0,
                        size,
                        created_at: image.cached_at,
                    }
                });
                entry.ref_count += 1;
            }
        }

        let mut refs = self.layer_refs.write().await;
        *refs = counts;
        info!("Rebuilt ref counts: {} unique layers tracked", refs.len());
        Ok(())
    }

    /// Run garbage collection.
    ///
    /// If `dry_run` is true, reports what would be freed without deleting.
    /// Respects high/low watermarks, min age, and max images configuration.
    pub async fn run(&self, dry_run: bool) -> Result<GcResult, RegistryError> {
        let start = std::time::Instant::now();
        info!(
            "Starting garbage collection (dry_run={}, config: {:?})",
            dry_run, self.config
        );

        // Rebuild ref counts
        self.rebuild_ref_counts().await?;

        let stats = self.cache.stats().await;
        let current_usage = if self.config.max_size_bytes > 0 {
            stats.total_bytes as f64 / self.config.max_size_bytes as f64
        } else {
            0.0
        };

        // Check if GC is needed
        if current_usage < self.config.high_watermark
            && (self.config.max_images == 0 || stats.image_count <= self.config.max_images)
        {
            info!(
                "Cache usage {:.1}% below watermark {:.1}%, skipping GC",
                current_usage * 100.0,
                self.config.high_watermark * 100.0
            );
            return Ok(GcResult {
                images_removed: 0,
                layers_freed: 0,
                bytes_freed: 0,
                duration_secs: start.elapsed().as_secs_f64(),
                dry_run,
            });
        }

        let images = self.cache.list_images().await;
        let min_age = Utc::now() - chrono::Duration::hours(self.config.min_age_hours as i64);

        // Sort candidates by last accessed (oldest first)
        let mut candidates: Vec<&CachedImageInfo> = images
            .iter()
            .filter(|img| img.last_accessed < min_age)
            .collect();
        candidates.sort_by(|a, b| a.last_accessed.cmp(&b.last_accessed));

        let mut result = GcResult {
            images_removed: 0,
            layers_freed: 0,
            bytes_freed: 0,
            duration_secs: 0.0,
            dry_run,
        };

        let target_removals = if self.config.max_images > 0 && stats.image_count > self.config.max_images {
            stats.image_count - self.config.max_images
        } else if current_usage > self.config.high_watermark && self.config.max_size_bytes > 0 {
            let excess_bytes = stats.total_bytes as f64
                - (self.config.max_size_bytes as f64 * self.config.low_watermark);
            // Estimate number of images to remove based on average size
            if stats.image_count > 0 {
                let avg_size = stats.total_bytes / stats.image_count as u64;
                (excess_bytes / avg_size as f64).ceil() as usize
            } else {
                0
            }
        } else {
            0
        };

        for image in candidates.into_iter().take(target_removals) {
            if dry_run {
                info!("DRY-RUN: Would remove image {}", image.image_ref);
                result.images_removed += 1;
                result.bytes_freed += image.size_bytes;
            } else {
                match self.cache.remove_image(&image.image_ref).await {
                    Ok(()) => {
                        result.images_removed += 1;
                        result.bytes_freed += image.size_bytes;
                        info!(
                            "GC: Removed image {} ({} bytes)",
                            image.image_ref, image.size_bytes
                        );
                    }
                    Err(e) => {
                        warn!("GC: Failed to remove image {}: {}", image.image_ref, e);
                    }
                }
            }
        }

        result.duration_secs = start.elapsed().as_secs_f64();
        info!(
            "GC complete: {} images, {} bytes freed in {:.2}s (dry_run={})",
            result.images_removed,
            result.bytes_freed,
            result.duration_secs,
            dry_run
        );

        Ok(result)
    }

    /// Spawn a periodic GC task.
    ///
    /// Returns a `JoinHandle` that can be used to cancel the task.
    pub fn spawn_periodic(
        self: Arc<Self>,
        interval: std::time::Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if let Err(e) = self.run(false).await {
                    warn!("Periodic GC failed: {}", e);
                }
            }
        })
    }

    /// Get current reference counts (for diagnostics).
    pub async fn ref_counts(&self) -> Vec<LayerRefCount> {
        self.layer_refs.read().await.values().cloned().collect()
    }

    /// Get shared layers (ref_count > 1) for optimization insights.
    pub async fn shared_layers(&self) -> Vec<LayerRefCount> {
        self.layer_refs
            .read()
            .await
            .values()
            .filter(|r| r.ref_count > 1)
            .cloned()
            .collect()
    }

    /// Get the GC configuration.
    pub fn config(&self) -> &GcConfig {
        &self.config
    }

    /// Update the GC configuration.
    pub fn set_config(&mut self, config: GcConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gc_config_default() {
        let config = GcConfig::default();
        assert_eq!(config.max_size_bytes, 50 * 1024 * 1024 * 1024);
        assert_eq!(config.high_watermark, 0.85);
        assert_eq!(config.low_watermark, 0.70);
        assert_eq!(config.min_age_hours, 24);
        assert_eq!(config.max_images, 100);
        assert!(config.preserve_running);
    }

    #[test]
    fn test_gc_result_serialization() {
        let result = GcResult {
            images_removed: 5,
            layers_freed: 12,
            bytes_freed: 1024 * 1024 * 100,
            duration_secs: 2.5,
            dry_run: false,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: GcResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.images_removed, 5);
        assert_eq!(parsed.bytes_freed, 1024 * 1024 * 100);
    }

    #[test]
    fn test_layer_ref_count() {
        let ref_count = LayerRefCount {
            digest: "sha256:abc123".to_string(),
            ref_count: 3,
            size: 1024 * 1024 * 50,
            created_at: Utc::now(),
        };

        assert_eq!(ref_count.ref_count, 3);
        assert_eq!(ref_count.size, 1024 * 1024 * 50);
    }

    #[test]
    fn test_gc_config_custom() {
        let config = GcConfig {
            max_size_bytes: 10 * 1024 * 1024 * 1024,
            high_watermark: 0.90,
            low_watermark: 0.75,
            min_age_hours: 48,
            max_images: 50,
            preserve_running: false,
        };

        assert_eq!(config.max_images, 50);
        assert!(!config.preserve_running);
        assert_eq!(config.min_age_hours, 48);
    }
}
