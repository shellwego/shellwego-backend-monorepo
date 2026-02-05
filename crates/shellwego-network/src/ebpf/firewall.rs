//! XDP-based firewall for DDoS protection and filtering

use crate::ebpf::{EbpfManager, EbpfError};

/// XDP firewall controller
pub struct XdpFirewall {
    _manager: EbpfManager,
}

impl XdpFirewall {
    /// Create new firewall instance
    pub fn new(manager: &EbpfManager) -> Self {
        Self {
            _manager: manager.clone(),
        }
    }

    /// Attach firewall to network interface
    pub async fn attach(&mut self, _iface: &str) -> Result<(), EbpfError> {
        // TODO: Load xdp_firewall.o
        // TODO: Attach XDP program
        // TODO: Store handle for later detach
        unimplemented!("XdpFirewall::attach")
    }

    /// Add IP to blocklist
    pub async fn block_ip(&self, _ip: std::net::IpAddr) -> Result<(), EbpfError> {
        // TODO: Convert IP to u32/u128
        // TODO: Insert into blocked_ips map
        unimplemented!("XdpFirewall::block_ip")
    }

    /// Remove IP from blocklist
    pub async fn unblock_ip(&self, _ip: std::net::IpAddr) -> Result<(), EbpfError> {
        // TODO: Remove from blocked_ips map
        unimplemented!("XdpFirewall::unblock_ip")
    }

    /// Add rate limit rule
    pub async fn add_rate_limit(
        &self,
        _ip: std::net::IpAddr,
        _packets_per_sec: u32,
    ) -> Result<(), EbpfError> {
        // TODO: Insert into rate_limits map
        unimplemented!("XdpFirewall::add_rate_limit")
    }

    /// Get dropped packet statistics
    pub async fn stats(&self) -> FirewallStats {
        // TODO: Read stats map from eBPF
        unimplemented!("XdpFirewall::stats")
    }

    /// Detach firewall
    pub async fn detach(&mut self) -> Result<(), EbpfError> {
        // TODO: Detach all programs
        unimplemented!("XdpFirewall::detach")
    }
}

/// Firewall statistics
#[derive(Debug, Clone, Default)]
pub struct FirewallStats {
    // TODO: Add packets_allowed, packets_dropped, packets_ratelimited
    // TODO: Add bytes_allowed, bytes_dropped
    // TODO: Add top_blocked_ips
}
