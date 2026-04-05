//! eBPF data-plane manager
//!
//! Loads compiled eBPF programs (XDP firewall + TC egress limiter) via `aya`.
//! When the compiled binary is unavailable (0-byte placeholder) the manager
//! operates in **fallback mode** where all operations no-op or delegate to
//! iptables / tc via the firewall and QoS sub-modules.

#[cfg(feature = "ebpf")]
use aya::{
    programs::{SchedClassifier, TcAttachType, Xdp, XdpFlags},
    Ebpf,
};
#[cfg(feature = "ebpf")]
use aya_log::EbpfLogger;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
#[allow(unused_imports)]
use tokio::sync::Mutex;
use thiserror::Error;

pub mod firewall;
pub mod qos;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum EbpfError {
    #[error("aya load failed: {0}")]
    LoadFailed(String),

    #[cfg(feature = "ebpf")]
    #[error("Bpf error: {0}")]
    Bpf(#[from] aya::EbpfError),

    #[cfg(feature = "ebpf")]
    #[error("Program error: {0}")]
    Program(#[from] aya::programs::ProgramError),

    #[cfg(feature = "ebpf")]
    #[error("Map error: {0}")]
    Map(#[from] aya::maps::MapError),

    #[error("eBPF not supported in this build")]
    NotSupported,

    #[error("eBPF binary is empty – running in fallback mode")]
    BinaryEmpty,
}

// ---------------------------------------------------------------------------
// EbpfManager
// ---------------------------------------------------------------------------

/// The heart of the data plane. Replaces legacy iptables logic.
///
/// When compiled with `feature = "ebpf"` and a valid `shellwego.bin` is
/// present, XDP/TC programs are loaded and attached.  If the binary is empty
/// (the common case during development), the manager enters **fallback mode**
/// where `attach_firewall` and `apply_qos` are no-ops (iptables/tc fallback
/// is handled by the `firewall` and `qos` sub-modules).
#[derive(Clone)]
pub struct EbpfManager {
    /// `true` when the eBPF programs were actually loaded successfully.
    ebpf_loaded: Arc<AtomicBool>,

    #[cfg(feature = "ebpf")]
    bpf: Arc<Mutex<Option<Ebpf>>>,
}

impl EbpfManager {
    /// Create a new eBPF manager.
    ///
    /// If the compiled eBPF binary is empty or cannot be loaded, the manager
    /// falls back to a no-op mode and logs a warning.
    pub async fn new() -> Result<Self, EbpfError> {
        #[cfg(feature = "ebpf")]
        {
            let bytes = include_bytes!("bin/shellwego.bin");

            if bytes.is_empty() {
                tracing::warn!(
                    "eBPF binary (shellwego.bin) is empty – running in fallback mode. \
                     Compile the eBPF programs with `make -C crates/shellwego-network ebpf` \
                     and place the output in src/ebpf/bin/shellwego.bin."
                );
                return Ok(Self {
                    ebpf_loaded: Arc::new(AtomicBool::new(false)),
                    bpf: Arc::new(Mutex::new(None)),
                });
            }

            match Ebpf::load(bytes) {
                Ok(mut bpf) => {
                    if let Err(e) = EbpfLogger::init(&mut bpf) {
                        tracing::warn!("failed to initialize eBPF logger: {}", e);
                    }
                    tracing::info!("eBPF programs loaded successfully");
                    Ok(Self {
                        ebpf_loaded: Arc::new(AtomicBool::new(true)),
                        bpf: Arc::new(Mutex::new(Some(bpf))),
                    })
                }
                Err(e) => {
                    tracing::warn!("failed to load eBPF binary, falling back to iptables/tc: {}", e);
                    Ok(Self {
                        ebpf_loaded: Arc::new(AtomicBool::new(false)),
                        bpf: Arc::new(Mutex::new(None)),
                    })
                }
            }
        }
        #[cfg(not(feature = "ebpf"))]
        {
            tracing::info!("eBPF feature not enabled – running in fallback mode");
            Ok(Self {
                ebpf_loaded: Arc::new(AtomicBool::new(false)),
            })
        }
    }

    /// Returns `true` if the eBPF programs are loaded and available.
    pub fn is_ebpf_loaded(&self) -> bool {
        self.ebpf_loaded.load(Ordering::SeqCst)
    }

    /// Attach the XDP firewall program to the given interface.
    ///
    /// When eBPF is unavailable this is a safe no-op.  Callers that need
    /// iptables fallback should use `firewall::XdpFirewall` instead, which
    /// already combines eBPF + iptables fallback.
    pub async fn attach_firewall(&self, iface: &str) -> Result<(), EbpfError> {
        if !self.ebpf_loaded.load(Ordering::SeqCst) {
            tracing::debug!(
                "eBPF not loaded – skipping XDP attach for {} (fallback mode)",
                iface
            );
            return Ok(());
        }

        #[cfg(feature = "ebpf")]
        {
            let mut bpf_guard = self.bpf.lock().await;
            let bpf = bpf_guard
                .as_mut()
                .ok_or_else(|| EbpfError::NotSupported)?;

            let program: &mut Xdp = bpf
                .program_mut("ingress_filter")
                .ok_or_else(|| EbpfError::LoadFailed("ingress_filter not found".to_string()))?
                .try_into()?;

            program.load()?;
            program.attach(iface, XdpFlags::SKB_MODE)?;

            tracing::info!("XDP firewall attached to {}", iface);
        }

        Ok(())
    }

    /// Apply TC egress rate limiting to the given interface.
    ///
    /// When eBPF is unavailable this is a safe no-op.  Callers that need
    /// tc-based fallback should use `qos::EbpfQos` instead.
    pub async fn apply_qos(&self, iface: &str, _limit_mbps: u32) -> Result<(), EbpfError> {
        if !self.ebpf_loaded.load(Ordering::SeqCst) {
            tracing::debug!(
                "eBPF not loaded – skipping TC QoS attach for {} (fallback mode)",
                iface
            );
            return Ok(());
        }

        #[cfg(feature = "ebpf")]
        {
            let mut bpf_guard = self.bpf.lock().await;
            let bpf = bpf_guard
                .as_mut()
                .ok_or_else(|| EbpfError::NotSupported)?;

            let mut rates = aya::maps::HashMap::try_from(
                bpf.map_mut("rate_config")
                    .ok_or_else(|| EbpfError::LoadFailed("rate_config not found".to_string()))?,
            )?;

            let ifindex = iface_to_u32(iface);
            // Convert Mbps to bytes per second
            let bytes_per_sec = (limit_mbps as u64) * 1_000_000 / 8;
            // Allow 100ms burst at target rate
            let burst = (bytes_per_sec / 10).max(4096);

            rates.insert(ifindex, bytes_per_sec, 0)?;

            let prog: &mut SchedClassifier = bpf
                .program_mut("tc_egress_limiter")
                .ok_or_else(|| {
                    EbpfError::LoadFailed("tc_egress_limiter not found".to_string())
                })?
                .try_into()?;
            prog.load()?;
            prog.attach(iface, TcAttachType::Egress)?;

            tracing::info!(
                "eBPF QoS applied to {} ({} Mbps, {} bytes/s burst {})",
                iface,
                limit_mbps,
                bytes_per_sec,
                burst
            );
        }

        Ok(())
    }

    /// Detach any loaded eBPF programs and clean up.
    pub async fn detach_all(&self) -> Result<(), EbpfError> {
        if !self.ebpf_loaded.load(Ordering::SeqCst) {
            return Ok(());
        }

        #[cfg(feature = "ebpf")]
        {
            // The aya Bpf object cleans up programs when dropped.
            // Here we explicitly clear the handle so future calls are no-ops.
            let mut guard = self.bpf.lock().await;
            *guard = None;
            self.ebpf_loaded.store(false, Ordering::SeqCst);
            tracing::info!("eBPF programs detached");
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[cfg(feature = "ebpf")]
fn iface_to_u32(iface: &str) -> u32 {
    nix::net::if_::if_nametoindex(iface).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ebpf_manager_new_fallback() {
        // With an empty shellwego.bin (or without the "ebpf" feature) the
        // manager should always succeed in creating itself in fallback mode.
        let manager = EbpfManager::new().await.unwrap();
        assert!(!manager.is_ebpf_loaded());
    }

    #[tokio::test]
    async fn test_attach_firewall_fallback_noop() {
        let manager = EbpfManager::new().await.unwrap();
        // Should not panic or error when eBPF is unavailable.
        let result = manager.attach_firewall("lo").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_apply_qos_fallback_noop() {
        let manager = EbpfManager::new().await.unwrap();
        let result = manager.apply_qos("lo", 100).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_detach_all_fallback_noop() {
        let manager = EbpfManager::new().await.unwrap();
        let result = manager.detach_all().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_ebpf_error_display() {
        let err = EbpfError::NotSupported;
        assert_eq!(format!("{}", err), "eBPF not supported in this build");

        let err = EbpfError::BinaryEmpty;
        assert_eq!(format!("{}", err), "eBPF binary is empty – running in fallback mode");
    }
}
