//! Image layer caching with ZFS backend
//!
//! Converts OCI layers to ZFS datasets for instant cloning.
//! Provides efficient storage and fast provisioning of container images.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::RegistryError;

// Import OCI types from schema
use shellwego_schema::oci::{Manifest, Descriptor, Platform};

/// Layer cache manager
pub struct LayerCache {
    /// ZFS pool name
    pool: String,
    /// Base dataset for images
    base_dataset: String,
    /// Manifest index (image_ref -> metadata)
    manifest_index: Arc<RwLock<HashMap<String, CachedImageInfo>>>,
    /// Cache directory for temporary files
    cache_dir: PathBuf,
}

/// Information about a cached image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedImageInfo {
    /// Image reference (e.g., "docker.io/library/nginx:latest")
    pub image_ref: String,
    /// ZFS dataset path
    pub dataset: String,
    /// Mount point for rootfs
    pub rootfs_path: PathBuf,
    /// Total size in bytes
    pub size_bytes: u64,
    /// Number of layers
    pub layer_count: usize,
    /// When the image was cached
    pub cached_at: DateTime<Utc>,
    /// Last access time
    pub last_accessed: DateTime<Utc>,
    /// Pull count
    pub pull_count: u64,
    /// Image digest
    pub digest: String,
}

/// Cache statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total bytes used by cached images
    pub total_bytes: u64,
    /// Number of cached layers
    pub layer_count: usize,
    /// Number of cached images
    pub image_count: usize,
    /// Cache hit rate (0.0 - 1.0)
    pub hit_rate: f64,
    /// Total cache hits
    pub hits: u64,
    /// Total cache misses
    pub misses: u64,
}

/// Layer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerInfo {
    /// Layer digest
    pub digest: String,
    /// Layer size in bytes
    pub size: u64,
    /// ZFS dataset name
    pub dataset: Option<String>,
    /// Whether layer is extracted
    pub extracted: bool,
}

impl LayerCache {
    /// Initialize cache on ZFS pool
    ///
    /// # Arguments
    /// * `pool` - ZFS pool name (e.g., "tank")
    ///
    /// # Returns
    /// A new LayerCache instance
    pub async fn new(pool: &str) -> Result<Self, RegistryError> {
        let base_dataset = format!("{}/registry", pool);
        let cache_dir = PathBuf::from("/var/lib/shellwego/registry/cache");

        // Verify ZFS is available
        Self::check_zfs_available()?;

        // Verify pool exists
        Self::verify_pool(pool).await?;

        // Create base dataset if needed
        Self::ensure_dataset(&base_dataset).await?;

        // Create sub-datasets
        let images_dataset = format!("{}/images", base_dataset);
        let layers_dataset = format!("{}/layers", base_dataset);

        Self::ensure_dataset(&images_dataset).await?;
        Self::ensure_dataset(&layers_dataset).await?;

        // Create cache directory
        tokio::fs::create_dir_all(&cache_dir).await?;

        // Load manifest index
        let manifest_index = Arc::new(RwLock::new(HashMap::new()));

        let cache = Self {
            pool: pool.to_string(),
            base_dataset,
            manifest_index,
            cache_dir,
        };

        // Load existing images
        cache.load_existing_images().await?;

        info!("Layer cache initialized on pool {}", pool);
        Ok(cache)
    }

    /// Check if ZFS is available
    fn check_zfs_available() -> Result<(), RegistryError> {
        let output = std::process::Command::new("which")
            .arg("zfs")
            .output()
            .map_err(|e| RegistryError::Io(e))?;

        if !output.status.success() {
            return Err(RegistryError::CacheCorrupted(
                "ZFS not available. Install zfsutils-linux.".to_string()
            ));
        }

        Ok(())
    }

    /// Verify pool exists and is healthy
    async fn verify_pool(pool: &str) -> Result<(), RegistryError> {
        let output = tokio::process::Command::new("zpool")
            .args(["list", "-H", "-o", "health", pool])
            .output()
            .await?;

        if !output.status.success() {
            return Err(RegistryError::NotFound(format!("ZFS pool: {}", pool)));
        }

        let health = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if health != "ONLINE" {
            warn!("ZFS pool {} health: {}", pool, health);
        }

        Ok(())
    }

    /// Ensure dataset exists
    async fn ensure_dataset(dataset: &str) -> Result<(), RegistryError> {
        let output = tokio::process::Command::new("zfs")
            .args(["list", "-H", "-o", "name", dataset])
            .output()
            .await?;

        if output.status.success() {
            return Ok(());
        }

        // Create dataset
        info!("Creating ZFS dataset: {}", dataset);
        let output = tokio::process::Command::new("zfs")
            .args(["create", "-p", dataset])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RegistryError::CacheCorrupted(
                format!("Failed to create dataset {}: {}", dataset, stderr)
            ));
        }

        // Set compression
        let _ = tokio::process::Command::new("zfs")
            .args(["set", "compression=zstd-3", dataset])
            .output()
            .await;

        Ok(())
    }

    /// Load existing cached images into index
    async fn load_existing_images(&self) -> Result<(), RegistryError> {
        let images_dataset = format!("{}/images", self.base_dataset);

        let output = tokio::process::Command::new("zfs")
            .args(["list", "-H", "-r", "-o", "name", &images_dataset])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut index = self.manifest_index.write().await;

        for line in stdout.lines() {
            let name = line.trim();
            if name == images_dataset || name.is_empty() {
                continue;
            }

            // Parse image name from dataset
            if let Some(image_ref) = name.strip_prefix(&format!("{}/", images_dataset)) {
                // Try to load metadata
                let metadata_path = self.cache_dir.join(image_ref.replace('/', "_")).join("metadata.json");
                if let Ok(content) = tokio::fs::read_to_string(&metadata_path).await {
                    if let Ok(info) = serde_json::from_str::<CachedImageInfo>(&content) {
                        index.insert(image_ref.to_string(), info);
                    }
                }
            }
        }

        info!("Loaded {} cached images", index.len());
        Ok(())
    }

    /// Check if image is cached locally
    ///
    /// # Arguments
    /// * `image_ref` - Image reference (e.g., "docker.io/library/nginx:latest")
    ///
    /// # Returns
    /// true if image is cached, false otherwise
    pub async fn is_cached(&self, image_ref: &str) -> bool {
        // Check in-memory index first
        {
            let index = self.manifest_index.read().await;
            if let Some(_info) = index.get(image_ref) {
                // Update last accessed
                return true;
            }
        }

        // Check filesystem
        let sanitized = Self::sanitize_image_ref(image_ref);
        let dataset = format!("{}/images/{}", self.base_dataset, sanitized);

        let output = tokio::process::Command::new("zfs")
            .args(["list", "-H", "-o", "name", &dataset])
            .output()
            .await;

        output.map(|o| o.status.success()).unwrap_or(false)
    }

    /// Get cached image rootfs path
    ///
    /// # Arguments
    /// * `image_ref` - Image reference
    ///
    /// # Returns
    /// Path to the rootfs mountpoint
    pub async fn get_rootfs(&self, image_ref: &str) -> Result<PathBuf, RegistryError> {
        let sanitized = Self::sanitize_image_ref(image_ref);
        let dataset = format!("{}/images/{}", self.base_dataset, sanitized);

        // Check if cached
        if !self.is_cached(image_ref).await {
            return Err(RegistryError::NotFound(format!("Image: {}", image_ref)));
        }

        // Get mountpoint
        let output = tokio::process::Command::new("zfs")
            .args(["get", "-H", "-o", "value", "mountpoint", &dataset])
            .output()
            .await?;

        if !output.status.success() {
            return Err(RegistryError::NotFound(format!("Dataset: {}", dataset)));
        }

        let mountpoint = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Update last accessed
        {
            let mut index = self.manifest_index.write().await;
            if let Some(info) = index.get_mut(image_ref) {
                info.last_accessed = Utc::now();
                info.pull_count += 1;
            }
        }

        Ok(PathBuf::from(mountpoint))
    }

    /// Import OCI image into ZFS cache
    ///
    /// # Arguments
    /// * `image_ref` - Image reference
    /// * `manifest` - OCI manifest
    /// * `layers` - Layer data (tar.gz or tar)
    ///
    /// # Returns
    /// Path to the cached rootfs
    pub async fn import_image(
        &self,
        image_ref: &str,
        manifest: &Manifest,
        layers: &[Vec<u8>],
    ) -> Result<PathBuf, RegistryError> {
        info!("Importing image {} ({} layers)", image_ref, manifest.layers.len());

        let sanitized = Self::sanitize_image_ref(image_ref);
        let image_dataset = format!("{}/images/{}", self.base_dataset, sanitized);

        // Create image dataset
        Self::ensure_dataset(&image_dataset).await?;

        let mut total_size = 0u64;

        // Import each layer
        for (i, layer) in layers.iter().enumerate() {
            let layer_digest = manifest.layers.get(i)
                .map(|l| l.digest.clone())
                .unwrap_or_else(|| format!("layer-{}", i));

            info!("Importing layer {} of {} ({})", i + 1, manifest.layers.len(), layer_digest);

            // Write layer to temp file
            let temp_path = self.cache_dir.join(format!("{}.tar", layer_digest.replace(':', "_")));
            tokio::fs::write(&temp_path, layer).await?;

            // Extract layer to dataset
            self.extract_layer(&image_dataset, &temp_path, i == 0).await?;

            total_size += layer.len() as u64;

            // Cleanup temp file
            let _ = tokio::fs::remove_file(&temp_path).await;
        }

        // Create base snapshot for fast cloning
        let snapshot = format!("{}@base", image_dataset);
        let _ = tokio::process::Command::new("zfs")
            .args(["snapshot", &snapshot])
            .output()
            .await;

        // Get mountpoint
        let output = tokio::process::Command::new("zfs")
            .args(["get", "-H", "-o", "value", "mountpoint", &image_dataset])
            .output()
            .await?;

        let mountpoint = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let rootfs_path = PathBuf::from(mountpoint);

        // Store metadata
        let info = CachedImageInfo {
            image_ref: image_ref.to_string(),
            dataset: image_dataset.clone(),
            rootfs_path: rootfs_path.clone(),
            size_bytes: total_size,
            layer_count: manifest.layers.len(),
            cached_at: Utc::now(),
            last_accessed: Utc::now(),
            pull_count: 1,
            digest: manifest.config.as_ref().map(|c| c.digest.clone()).unwrap_or_default(),
        };

        // Save metadata
        let metadata_dir = self.cache_dir.join(sanitized.clone());
        tokio::fs::create_dir_all(&metadata_dir).await?;
        let metadata_path = metadata_dir.join("metadata.json");
        tokio::fs::write(&metadata_path, serde_json::to_string_pretty(&info)?).await?;

        // Update index
        {
            let mut index = self.manifest_index.write().await;
            index.insert(image_ref.to_string(), info);
        }

        info!("Successfully imported image {} to {:?}", image_ref, rootfs_path);
        Ok(rootfs_path)
    }

    /// Extract a layer to the dataset
    async fn extract_layer(
        &self,
        dataset: &str,
        layer_path: &Path,
        is_first: bool,
    ) -> Result<(), RegistryError> {
        // Get mountpoint
        let output = tokio::process::Command::new("zfs")
            .args(["get", "-H", "-o", "value", "mountpoint", dataset])
            .output()
            .await?;

        let mountpoint = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if mountpoint == "none" || mountpoint == "-" {
            // Set mountpoint
            let mount_path = format!("/var/lib/shellwego/registry/{}", dataset.replace('/', "_"));
            tokio::fs::create_dir_all(&mount_path).await?;

            let _ = tokio::process::Command::new("zfs")
                .args(["set", &format!("mountpoint={}", mount_path), dataset])
                .output()
                .await;
        }

        // Get actual mountpoint
        let output = tokio::process::Command::new("zfs")
            .args(["get", "-H", "-o", "value", "mountpoint", dataset])
            .output()
            .await?;

        let mountpoint = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Decompress if needed
        let decompressed_path = if layer_path.extension().map(|e| e == "gz").unwrap_or(false) {
            let output_path = layer_path.with_extension("");
            let output = tokio::process::Command::new("gunzip")
                .args(["-c", layer_path.to_str().unwrap()])
                .output()
                .await?;
            tokio::fs::write(&output_path, &output.stdout).await?;
            output_path
        } else {
            layer_path.to_path_buf()
        };

        // Extract tar
        let output = tokio::process::Command::new("tar")
            .args([
                "-xf", decompressed_path.to_str().unwrap(),
                "-C", &mountpoint,
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Tar extraction warning: {}", stderr);
        }

        // Cleanup decompressed file if we created it
        if decompressed_path != *layer_path {
            let _ = tokio::fs::remove_file(&decompressed_path).await;
        }

        Ok(())
    }

    /// Garbage collect unused layers
    ///
    /// # Arguments
    /// * `keep_recent` - Number of recent images to keep
    ///
    /// # Returns
    /// Bytes freed
    pub async fn gc(&self, keep_recent: usize) -> Result<u64, RegistryError> {
        info!("Running garbage collection (keeping {} most recent)", keep_recent);

        let mut index = self.manifest_index.write().await;
        let total_images = index.len();

        if total_images <= keep_recent {
            info!("No images to garbage collect");
            return Ok(0);
        }

        // Sort by last accessed time
        let mut images: Vec<_> = index.iter().collect();
        images.sort_by(|a, b| a.1.last_accessed.cmp(&b.1.last_accessed));

        // Remove oldest
        let to_remove = total_images - keep_recent;
        let mut bytes_freed = 0u64;

        for (image_ref, info) in images.into_iter().take(to_remove) {
            info!("GC: Removing image {}", image_ref);

            // Destroy dataset
            let output = tokio::process::Command::new("zfs")
                .args(["destroy", "-r", &info.dataset])
                .output()
                .await?;

            if output.status.success() {
                bytes_freed += info.size_bytes;
                index.remove(image_ref);

                // Remove metadata
                let sanitized = Self::sanitize_image_ref(image_ref);
                let metadata_dir = self.cache_dir.join(sanitized);
                let _ = tokio::fs::remove_dir_all(&metadata_dir).await;
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Failed to destroy dataset {}: {}", info.dataset, stderr);
            }
        }

        info!("GC: Freed {} bytes", bytes_freed);
        Ok(bytes_freed)
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let index = self.manifest_index.read().await;

        let total_bytes: u64 = index.values().map(|i| i.size_bytes).sum();
        let layer_count: usize = index.values().map(|i| i.layer_count).sum();
        let image_count = index.len();

        // Calculate hit rate (would need counters for this)
        let hits = index.values().map(|i| i.pull_count).sum();
        let hit_rate = if hits > 0 { 1.0 } else { 0.0 };

        CacheStats {
            total_bytes,
            layer_count,
            image_count,
            hit_rate,
            hits,
            misses: 0,
        }
    }

    /// Clone cached image for use by a container
    ///
    /// Creates a writable clone from the cached image snapshot.
    ///
    /// # Arguments
    /// * `image_ref` - Source image reference
    /// * `container_id` - ID of the container
    ///
    /// # Returns
    /// Path to the cloned rootfs
    pub async fn clone_for_container(
        &self,
        image_ref: &str,
        container_id: &str,
    ) -> Result<PathBuf, RegistryError> {
        let sanitized = Self::sanitize_image_ref(image_ref);
        let source_snapshot = format!("{}/images/{}@base", self.base_dataset, sanitized);

        // Check if source exists
        let output = tokio::process::Command::new("zfs")
            .args(["list", "-t", "snapshot", &source_snapshot])
            .output()
            .await?;

        if !output.status.success() {
            return Err(RegistryError::NotFound(format!("Image snapshot: {}", source_snapshot)));
        }

        // Create clone
        let clone_dataset = format!("{}/containers/{}", self.base_dataset, container_id);

        let output = tokio::process::Command::new("zfs")
            .args(["clone", &source_snapshot, &clone_dataset])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RegistryError::CacheCorrupted(
                format!("Failed to clone image: {}", stderr)
            ));
        }

        // Get mountpoint
        let output = tokio::process::Command::new("zfs")
            .args(["get", "-H", "-o", "value", "mountpoint", &clone_dataset])
            .output()
            .await?;

        let mountpoint = String::from_utf8_lossy(&output.stdout).trim().to_string();

        info!("Cloned image {} for container {} to {}", image_ref, container_id, mountpoint);
        Ok(PathBuf::from(mountpoint))
    }

    /// Remove a cloned container filesystem
    pub async fn remove_container_clone(&self, container_id: &str) -> Result<(), RegistryError> {
        let clone_dataset = format!("{}/containers/{}", self.base_dataset, container_id);

        let output = tokio::process::Command::new("zfs")
            .args(["destroy", "-r", &clone_dataset])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to remove container clone {}: {}", container_id, stderr);
        }

        Ok(())
    }

    /// Sanitize image reference for use in dataset names
    fn sanitize_image_ref(image_ref: &str) -> String {
        image_ref
            .replace(['/', ':'], "_")
            .replace(['@', '+', ' '], "_")
    }

    /// List all cached images
    pub async fn list_images(&self) -> Vec<CachedImageInfo> {
        let index = self.manifest_index.read().await;
        index.values().cloned().collect()
    }

    /// Remove a specific image from cache
    pub async fn remove_image(&self, image_ref: &str) -> Result<(), RegistryError> {
        let mut index = self.manifest_index.write().await;

        if let Some(info) = index.remove(image_ref) {
            let output = tokio::process::Command::new("zfs")
                .args(["destroy", "-r", &info.dataset])
                .output()
                .await?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(RegistryError::CacheCorrupted(
                    format!("Failed to remove image: {}", stderr)
                ));
            }

            let sanitized = Self::sanitize_image_ref(image_ref);
            let metadata_dir = self.cache_dir.join(sanitized);
            let _ = tokio::fs::remove_dir_all(&metadata_dir).await;

            info!("Removed image {} from cache", image_ref);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_image_ref() {
        assert_eq!(
            LayerCache::sanitize_image_ref("docker.io/library/nginx:latest"),
            "docker.io_library_nginx_latest"
        );
        assert_eq!(
            LayerCache::sanitize_image_ref("gcr.io/project/image@sha256:abc123"),
            "gcr.io_project_image_sha256_abc123"
        );
    }

    #[test]
    fn test_cache_stats_default() {
        let stats = CacheStats::default();
        assert_eq!(stats.total_bytes, 0);
        assert_eq!(stats.image_count, 0);
    }
}
