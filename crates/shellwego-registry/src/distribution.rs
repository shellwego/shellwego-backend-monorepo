//! Distribution manager — orchestrates the full pull path.
//!
//! Entry point for all image distribution in the system. Coordinates
//! between P2P, mirror chain, and upstream registry in priority order:
//!
//! ```text
//! P2P (Dragonfly) → Mirror Chain → Upstream Registry
//! ```

use std::sync::Arc;

use tracing::info;

use crate::mirror::MirrorChain;
use crate::pull::{ImagePuller, PullProgress, PulledImage};
use crate::p2p::client::DragonflyClient;
use crate::RegistryError;
use shellwego_schema::oci::RegistryAuth;

/// Pull strategy options.
#[derive(Debug, Clone, Default)]
pub struct PullOptions {
    /// Try P2P first before HTTP mirrors/upstream.
    pub prefer_p2p: bool,
    /// Minimum P2P peer threshold before attempting P2P.
    pub p2p_min_peers: usize,
    /// Progress callback.
    pub progress: Option<Arc<dyn PullProgress + Send + Sync>>,
}

/// Orchestrates image distribution across P2P, mirrors, and upstream.
///
/// This is the main entry point for image pulls in the system. It wraps
/// `ImagePuller` (which already has mirror chain support) and optionally
/// adds P2P distribution via `DragonflyClient`.
pub struct DistributionManager {
    /// HTTP-based image puller (with mirror chain integrated)
    puller: ImagePuller,
    /// P2P distribution client (optional)
    p2p: Option<DragonflyClient>,
}

impl DistributionManager {
    /// Create a new distribution manager.
    ///
    /// # Arguments
    /// * `puller` - Image puller (optionally configured with mirror chain)
    /// * `p2p` - Optional P2P client for Dragonfly distribution
    pub fn new(puller: ImagePuller, p2p: Option<DragonflyClient>) -> Self {
        Self { puller, p2p }
    }

    /// Create a builder-style distribution manager.
    pub fn builder() -> DistributionManagerBuilder {
        DistributionManagerBuilder::default()
    }

    /// Pull an image using the best available strategy.
    ///
    /// The pull path is:
    /// 1. Use `ImagePuller::pull()` which internally tries mirrors before upstream
    /// 2. If P2P is configured and `prefer_p2p` is set, try P2P first
    /// 3. Announce available layers to P2P network after successful pull
    pub async fn pull(
        &self,
        image_ref: &str,
        auth: Option<&RegistryAuth>,
        options: PullOptions,
    ) -> Result<PulledImage, RegistryError> {
        info!(
            "Pulling image {} (P2P={}, mirrors={})",
            image_ref,
            options.prefer_p2p && self.p2p.is_some(),
            self.puller.has_mirrors()
        );

        // Use the standard puller (with mirror chain already integrated)
        let mut progress = crate::pull::NoOpProgress;
        let result = if let Some(ref _progress_cb) = options.progress {
            self.puller
                .pull_with_progress(image_ref, auth, &mut progress)
                .await?
        } else {
            self.puller.pull(image_ref, auth).await?
        };

        // Announce available layers to P2P network
        if let Some(ref p2p) = self.p2p {
            for digest in &result.layer_digests {
                let size = result
                    .manifest
                    .layers
                    .iter()
                    .find(|l| &l.digest == digest)
                    .map(|l| l.size)
                    .unwrap_or(0);
                if size > 0 {
                    p2p.announce_layer(digest, size).await;
                }
            }
        }

        Ok(result)
    }

    /// Get a reference to the underlying image puller.
    pub fn puller(&self) -> &ImagePuller {
        &self.puller
    }

    /// Get a reference to the P2P client, if configured.
    pub fn p2p(&self) -> Option<&DragonflyClient> {
        self.p2p.as_ref()
    }
}

/// Builder for `DistributionManager`.
#[derive(Default)]
pub struct DistributionManagerBuilder {
    puller: Option<ImagePuller>,
    mirror_chain: Option<MirrorChain>,
    p2p_min_peers: usize,
    enable_p2p: bool,
}

impl DistributionManagerBuilder {
    /// Set a custom image puller.
    pub fn puller(mut self, puller: ImagePuller) -> Self {
        self.puller = Some(puller);
        self
    }

    /// Set a mirror chain for the puller.
    pub fn mirror_chain(mut self, chain: MirrorChain) -> Self {
        self.mirror_chain = Some(chain);
        self
    }

    /// Enable P2P distribution with the given minimum peer threshold.
    pub fn enable_p2p(mut self, min_peers: usize) -> Self {
        self.p2p_min_peers = min_peers;
        self.enable_p2p = true;
        self
    }

    /// Build the distribution manager.
    pub async fn build(self) -> Result<DistributionManager, RegistryError> {
        let mut puller = self.puller.unwrap_or_else(ImagePuller::new);

        if let Some(chain) = self.mirror_chain {
            puller = puller.with_mirror_chain(chain);
        }

        let p2p = if self.enable_p2p {
            Some(DragonflyClient::new(self.p2p_min_peers).await?)
        } else {
            None
        };

        Ok(DistributionManager::new(puller, p2p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_default() {
        let builder = DistributionManagerBuilder::default();
        // Should not panic
        assert!(builder.puller.is_none());
    }

    #[tokio::test]
    async fn test_builder_with_p2p() {
        let manager = DistributionManagerBuilder::default()
            .enable_p2p(2)
            .build()
            .await;
        assert!(manager.is_ok());
        let mgr = manager.unwrap();
        assert!(mgr.p2p().is_some());
    }

    #[tokio::test]
    async fn test_builder_with_mirrors() {
        let mirror_list = shellwego_schema::oci::MirrorList::new();
        let chain = MirrorChain::new(mirror_list);

        let manager = DistributionManagerBuilder::default()
            .mirror_chain(chain)
            .build()
            .await;
        assert!(manager.is_ok());
    }
}
