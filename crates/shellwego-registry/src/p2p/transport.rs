//! P2P transport layer for piece transfer.
//!
//! Phase 1: HTTP-based (each peer runs a small HTTP server serving pieces).
//! Phase 2: Can be upgraded to libp2p RequestResponse protocol over QUIC.

use bytes::Bytes;
use std::net::SocketAddr;
use tracing::{debug, warn};

/// Transport layer for P2P piece transfers.
///
/// In Phase 1, this uses HTTP GET requests to fetch pieces from peers.
/// Each peer exposes an endpoint at `/v1/p2p/pieces/{digest}/{offset}`.
pub struct PeerTransport {
    client: reqwest::Client,
}

impl PeerTransport {
    /// Create a new transport instance.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create P2P transport client");
        Self { client }
    }

    /// Fetch a piece from a peer.
    ///
    /// # Arguments
    /// * `peer_addr` - Peer's listen address (IP:port)
    /// * `digest` - Layer digest (e.g., "sha256:abc...")
    /// * `offset` - Byte offset within the layer
    /// * `length` - Length of the piece in bytes
    ///
    /// # Returns
    /// The piece data as `Bytes`.
    pub async fn fetch_piece(
        &self,
        peer_addr: SocketAddr,
        digest: &str,
        offset: u64,
        length: u64,
    ) -> Result<Bytes, crate::RegistryError> {
        let url = format!(
            "http://{}/v1/p2p/pieces/{}/{}",
            peer_addr, digest, offset
        );

        debug!(
            "Fetching piece from peer {} ({} bytes at offset {})",
            peer_addr, length, offset
        );

        let response = self
            .client
            .get(&url)
            .header(
                "Range",
                format!("bytes={}-{}", offset, offset + length - 1),
            )
            .send()
            .await
            .map_err(|e| {
                crate::RegistryError::Http(format!("P2P fetch failed from {}: {}", peer_addr, e))
            })?;

        if !response.status().is_success() && response.status().as_u16() != 206 {
            return Err(crate::RegistryError::Http(format!(
                "P2P peer {} returned status {}",
                peer_addr,
                response.status()
            )));
        }

        let data = response
            .bytes()
            .await
            .map_err(|e| {
                crate::RegistryError::Http(format!(
                    "P2P body read failed from {}: {}",
                    peer_addr, e
                ))
            })?;

        Ok(data)
    }
}

impl Default for PeerTransport {
    fn default() -> Self {
        Self::new()
    }
}
