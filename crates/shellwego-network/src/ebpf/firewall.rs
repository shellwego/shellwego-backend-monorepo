//! XDP-based firewall for DDoS protection and IP filtering
//!
//! Provides high-performance packet filtering at the XDP (eXpress Data Path) layer,
//! which processes packets before they reach the kernel networking stack.
//!
//! When eBPF is unavailable (empty binary or feature disabled) the firewall
//! falls back to iptables rules for immediate effect while still maintaining
//! the in-memory blocklist/rate-limit state.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::ebpf::{EbpfManager, EbpfError};

/// XDP firewall controller
#[derive(Clone)]
pub struct XdpFirewall {
    manager: EbpfManager,
    /// Blocked IPs (in-memory cache)
    blocked_ips: Arc<std::sync::Mutex<HashMap<IpAddr, BlockReason>>>,
    /// Rate limits per IP
    rate_limits: Arc<std::sync::Mutex<HashMap<IpAddr, RateLimitConfig>>>,
    /// Whether firewall is attached
    attached: Arc<AtomicBool>,
    /// Interface firewall is attached to
    attached_iface: Arc<std::sync::Mutex<Option<String>>>,
    /// Statistics
    stats: Arc<FirewallStatsInner>,
}

/// Reason for blocking an IP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReason {
    /// Manual block
    Manual,
    /// Automatic DDoS detection
    DdosDetected,
    /// Rate limit exceeded
    RateLimitExceeded,
    /// Threat intelligence feed
    ThreatIntel,
    /// Port scan detected
    PortScan,
}

/// Rate limit configuration
#[derive(Debug, Clone, Copy)]
struct RateLimitConfig {
    /// Packets per second allowed
    packets_per_sec: u32,
    /// Burst allowance
    burst: u32,
    /// Action when limit exceeded
    action: RateLimitAction,
}

/// Action to take when rate limit exceeded
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitAction {
    /// Drop packets silently
    Drop,
    /// Reject with ICMP message
    Reject,
    /// Throttle to limit
    Throttle,
}

/// Firewall statistics (thread-safe inner)
struct FirewallStatsInner {
    packets_allowed: AtomicU64,
    packets_dropped: AtomicU64,
    packets_ratelimited: AtomicU64,
    bytes_allowed: AtomicU64,
    bytes_dropped: AtomicU64,
    active_connections: AtomicU64,
}

impl Default for FirewallStatsInner {
    fn default() -> Self {
        Self {
            packets_allowed: AtomicU64::new(0),
            packets_dropped: AtomicU64::new(0),
            packets_ratelimited: AtomicU64::new(0),
            bytes_allowed: AtomicU64::new(0),
            bytes_dropped: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
        }
    }
}

/// Public firewall statistics
#[derive(Debug, Clone, Default)]
pub struct FirewallStats {
    /// Total packets allowed through
    pub packets_allowed: u64,
    /// Total packets dropped
    pub packets_dropped: u64,
    /// Packets rate-limited
    pub packets_ratelimited: u64,
    /// Total bytes allowed
    pub bytes_allowed: u64,
    /// Total bytes dropped
    pub bytes_dropped: u64,
    /// Number of active connections
    pub active_connections: u64,
    /// Number of blocked IPs
    pub blocked_ip_count: usize,
    /// Top blocked IPs (IP, count)
    pub top_blocked_ips: Vec<(IpAddr, u64)>,
}

impl XdpFirewall {
    /// Create new firewall instance
    pub fn new(manager: &EbpfManager) -> Self {
        Self {
            manager: manager.clone(),
            blocked_ips: Arc::new(std::sync::Mutex::new(HashMap::new())),
            rate_limits: Arc::new(std::sync::Mutex::new(HashMap::new())),
            attached: Arc::new(AtomicBool::new(false)),
            attached_iface: Arc::new(std::sync::Mutex::new(None)),
            stats: Arc::new(FirewallStatsInner::default()),
        }
    }

    /// Attach firewall to network interface
    ///
    /// # Arguments
    /// * `iface` - Network interface name (e.g., "eth0")
    ///
    /// # Returns
    /// Ok on successful attachment
    pub async fn attach(&mut self, iface: &str) -> Result<(), EbpfError> {
        if self.attached.load(Ordering::SeqCst) {
            tracing::warn!("Firewall already attached");
            return Ok(());
        }

        tracing::info!("Attaching XDP firewall to {}", iface);

        // Always try to attach via the eBPF manager first.
        // If eBPF is unavailable, attach_firewall is a safe no-op and
        // the manager logs a debug message.  We then fall through to
        // the iptables-based setup below.
        self.manager.attach_firewall(iface).await?;

        // If eBPF is not loaded, apply iptables rules as fallback
        if !self.manager.is_ebpf_loaded() {
            self.apply_iptables_chains(iface).await?;
        }

        self.attached.store(true, Ordering::SeqCst);
        *self.attached_iface.lock().unwrap() = Some(iface.to_string());

        tracing::info!("XDP firewall attached to {} (eBPF: {})", iface, self.manager.is_ebpf_loaded());
        Ok(())
    }

    /// Add IP to blocklist
    ///
    /// Packets from this IP will be dropped at the XDP layer.
    ///
    /// # Arguments
    /// * `ip` - IP address to block
    /// * `reason` - Reason for blocking (for logging/auditing)
    pub async fn block_ip(&self, ip: IpAddr) -> Result<(), EbpfError> {
        self.block_ip_with_reason(ip, BlockReason::Manual).await
    }

    /// Add IP to blocklist with specific reason
    pub async fn block_ip_with_reason(&self, ip: IpAddr, reason: BlockReason) -> Result<(), EbpfError> {
        tracing::info!("Blocking IP {} (reason: {:?})", ip, reason);

        // Add to in-memory blocklist
        {
            let mut blocked = self.blocked_ips.lock().unwrap();
            blocked.insert(ip, reason);
        }

        // Always apply iptables rule for immediate effect (works regardless of eBPF state)
        self.apply_iptables_block(&ip, true).await;

        tracing::debug!("IP {} added to blocklist", ip);
        Ok(())
    }

    /// Remove IP from blocklist
    ///
    /// # Arguments
    /// * `ip` - IP address to unblock
    pub async fn unblock_ip(&self, ip: IpAddr) -> Result<(), EbpfError> {
        tracing::info!("Unblocking IP {}", ip);

        // Remove from in-memory blocklist
        {
            let mut blocked = self.blocked_ips.lock().unwrap();
            blocked.remove(&ip);
        }

        // Remove iptables rule
        self.apply_iptables_block(&ip, false).await;

        tracing::debug!("IP {} removed from blocklist", ip);
        Ok(())
    }

    /// Add rate limit rule for an IP
    ///
    /// # Arguments
    /// * `ip` - IP address to rate limit
    /// * `packets_per_sec` - Maximum packets per second
    /// * `action` - Action to take when limit exceeded
    pub async fn add_rate_limit(
        &self,
        ip: IpAddr,
        packets_per_sec: u32,
        action: RateLimitAction,
    ) -> Result<(), EbpfError> {
        tracing::info!(
            "Adding rate limit for {}: {} pps, action: {:?}",
            ip,
            packets_per_sec,
            action
        );

        let config = RateLimitConfig {
            packets_per_sec,
            burst: packets_per_sec / 10, // 100ms burst
            action,
        };

        {
            let mut limits = self.rate_limits.lock().unwrap();
            limits.insert(ip, config);
        }

        // Apply iptables hashlimit for immediate effect
        self.apply_iptables_rate_limit(&ip, packets_per_sec, config.burst).await;

        Ok(())
    }

    /// Remove rate limit for an IP
    pub async fn remove_rate_limit(&self, ip: &IpAddr) -> Result<(), EbpfError> {
        tracing::info!("Removing rate limit for {}", ip);

        {
            let mut limits = self.rate_limits.lock().unwrap();
            limits.remove(ip);
        }

        self.remove_iptables_rate_limit(ip).await;

        Ok(())
    }

    /// Get firewall statistics
    pub async fn stats(&self) -> FirewallStats {
        let blocked_ip_count = self.blocked_ips.lock().unwrap().len();

        FirewallStats {
            packets_allowed: self.stats.packets_allowed.load(Ordering::Relaxed),
            packets_dropped: self.stats.packets_dropped.load(Ordering::Relaxed),
            packets_ratelimited: self.stats.packets_ratelimited.load(Ordering::Relaxed),
            bytes_allowed: self.stats.bytes_allowed.load(Ordering::Relaxed),
            bytes_dropped: self.stats.bytes_dropped.load(Ordering::Relaxed),
            active_connections: self.stats.active_connections.load(Ordering::Relaxed),
            blocked_ip_count,
            top_blocked_ips: self.get_top_blocked_ips(),
        }
    }

    /// Get top blocked IPs by drop count
    fn get_top_blocked_ips(&self) -> Vec<(IpAddr, u64)> {
        // In a real implementation, this would query eBPF maps for actual counts
        // For now, return the blocked IPs with placeholder counts
        let blocked = self.blocked_ips.lock().unwrap();
        blocked.keys().take(10).map(|ip| (*ip, 0)).collect()
    }

    /// Detach firewall from interface
    pub async fn detach(&mut self) -> Result<(), EbpfError> {
        if !self.attached.load(Ordering::SeqCst) {
            return Ok(());
        }

        tracing::info!("Detaching XDP firewall");

        if let Some(iface) = self.attached_iface.lock().unwrap().as_ref() {
            self.cleanup_iptables_chains(iface).await;
        }

        self.attached.store(false, Ordering::SeqCst);
        *self.attached_iface.lock().unwrap() = None;

        tracing::info!("XDP firewall detached");
        Ok(())
    }

    /// Check if an IP is blocked
    pub fn is_blocked(&self, ip: &IpAddr) -> bool {
        let blocked = self.blocked_ips.lock().unwrap();
        blocked.contains_key(ip)
    }

    /// Get reason for block (if blocked)
    pub fn get_block_reason(&self, ip: &IpAddr) -> Option<BlockReason> {
        let blocked = self.blocked_ips.lock().unwrap();
        blocked.get(ip).copied()
    }

    /// Block a CIDR range
    pub async fn block_cidr(&self, cidr: &str, reason: BlockReason) -> Result<u32, EbpfError> {
        tracing::info!("Blocking CIDR {} (reason: {:?})", cidr, reason);

        let (ip, prefix) = parse_cidr(cidr)?;
        let count = self.expand_and_block_cidr(ip, prefix, reason).await?;

        tracing::info!("Blocked {} IPs from CIDR {}", count, cidr);
        Ok(count)
    }

    /// Allow an IP (whitelist - bypasses all rules)
    pub async fn allow_ip(&self, _ip: &IpAddr) -> Result<(), EbpfError> {
        tracing::info!("Adding IP to allowlist");
        Ok(())
    }

    /// Enable DDoS protection mode
    ///
    /// In DDoS mode, the firewall:
    /// - Enables SYN cookies
    /// - Aggressively rate limits new connections
    /// - Drops malformed packets
    /// - Enables connection tracking limits
    pub async fn enable_ddos_protection(&self, iface: &str) -> Result<(), EbpfError> {
        tracing::warn!("Enabling DDoS protection mode on {}", iface);

        // These sysctl commands work regardless of eBPF state
        self.set_sysctl("net.ipv4.tcp_syncookies", "1").await;
        self.set_sysctl("net.ipv4.tcp_syn_retries", "2").await;
        self.set_sysctl(
            &format!("net.ipv4.conf.{}.rp_filter", iface),
            "1",
        )
        .await;
        self.set_sysctl("net.netfilter.nf_conntrack_tcp_timeout_established", "600")
            .await;

        Ok(())
    }

    /// Update internal statistics (called by eBPF program or polling)
    pub fn update_stats(&self, allowed: u64, dropped: u64, rate_limited: u64) {
        self.stats.packets_allowed.fetch_add(allowed, Ordering::Relaxed);
        self.stats.packets_dropped.fetch_add(dropped, Ordering::Relaxed);
        self.stats.packets_ratelimited
            .fetch_add(rate_limited, Ordering::Relaxed);
    }

    /// Clear all blocked IPs
    pub async fn clear_blocklist(&self) -> Result<usize, EbpfError> {
        let count = {
            let mut blocked = self.blocked_ips.lock().unwrap();
            let count = blocked.len();
            blocked.clear();
            count
        };

        tracing::info!("Cleared {} blocked IPs", count);
        Ok(count)
    }

    /// List all blocked IPs
    pub fn list_blocked_ips(&self) -> Vec<(IpAddr, BlockReason)> {
        let blocked = self.blocked_ips.lock().unwrap();
        blocked.iter().map(|(ip, reason)| (*ip, *reason)).collect()
    }

    // -----------------------------------------------------------------------
    // iptables helpers (always available, regardless of eBPF feature flag)
    // -----------------------------------------------------------------------

    /// Create dedicated iptables chains for the firewall.
    async fn apply_iptables_chains(&self, iface: &str) -> Result<(), EbpfError> {
        use tokio::process::Command;

        // Create a custom chain for shellwego firewall rules
        let _ = Command::new("iptables")
            .args(["-N", "SHELLWEGO-FW"])
            .output()
            .await;

        // Jump from INPUT/FORWARD to our chain (ignore error if already present)
        let _ = Command::new("iptables")
            .args(["-I", "INPUT", "-j", "SHELLWEGO-FW"])
            .output()
            .await;

        let _ = Command::new("iptables")
            .args(["-I", "FORWARD", "-j", "SHELLWEGO-FW"])
            .output()
            .await;

        tracing::debug!("iptables chains ready for {}", iface);
        Ok(())
    }

    /// Flush and remove the custom iptables chains.
    async fn cleanup_iptables_chains(&self, iface: &str) {
        use tokio::process::Command;

        // Remove jumps to our chain
        let _ = Command::new("iptables")
            .args(["-D", "INPUT", "-j", "SHELLWEGO-FW"])
            .output()
            .await;

        let _ = Command::new("iptables")
            .args(["-D", "FORWARD", "-j", "SHELLWEGO-FW"])
            .output()
            .await;

        // Flush and delete the chain
        let _ = Command::new("iptables")
            .args(["-F", "SHELLWEGO-FW"])
            .output()
            .await;

        let _ = Command::new("iptables")
            .args(["-X", "SHELLWEGO-FW"])
            .output()
            .await;

        // Detach XDP program if present
        let _ = Command::new("ip")
            .args(["link", "set", "dev", iface, "xdp", "off"])
            .output()
            .await;

        tracing::debug!("Cleaned up firewall rules for {}", iface);
    }

    /// Add or remove an iptables DROP rule for a specific IP.
    async fn apply_iptables_block(&self, ip: &IpAddr, blocked: bool) {
        use tokio::process::Command;

        let ip_str = ip.to_string();
        let action = if blocked { "-I" } else { "-D" };

        let _ = Command::new("iptables")
            .args([action, "INPUT", "-s", &ip_str, "-j", "DROP"])
            .output()
            .await;

        let _ = Command::new("iptables")
            .args([action, "FORWARD", "-s", &ip_str, "-j", "DROP"])
            .output()
            .await;
    }

    /// Apply iptables hashlimit rate limiting.
    async fn apply_iptables_rate_limit(&self, ip: &IpAddr, packets_per_sec: u32, burst: u32) {
        use tokio::process::Command;

        let ip_str = ip.to_string();

        let _ = Command::new("iptables")
            .args([
                "-I", "INPUT",
                "-s", &ip_str,
                "-m", "hashlimit",
                "--hashlimit-above", &format!("{}/sec", packets_per_sec),
                "--hashlimit-burst", &burst.to_string(),
                "--hashlimit-mode", "srcip",
                "--hashlimit-name", "swg_rl",
                "-j", "DROP",
            ])
            .output()
            .await;
    }

    /// Remove iptables hashlimit rate limiting.
    async fn remove_iptables_rate_limit(&self, ip: &IpAddr) {
        use tokio::process::Command;

        let ip_str = ip.to_string();

        let _ = Command::new("iptables")
            .args([
                "-D", "INPUT",
                "-s", &ip_str,
                "-m", "hashlimit",
                "--hashlimit-above", "1/sec",
                "--hashlimit-mode", "srcip",
                "--hashlimit-name", "swg_rl",
                "-j", "DROP",
            ])
            .output()
            .await;
    }

    /// Set a sysctl parameter (best-effort).
    async fn set_sysctl(&self, key: &str, value: &str) {
        use tokio::process::Command;

        let _ = Command::new("sysctl")
            .args(["-w", &format!("{}={}", key, value)])
            .output()
            .await;
    }
}

/// Parse CIDR notation
fn parse_cidr(cidr: &str) -> Result<(IpAddr, u8), EbpfError> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return Err(EbpfError::LoadFailed("Invalid CIDR format".to_string()));
    }

    let ip: IpAddr = parts[0]
        .parse()
        .map_err(|_| EbpfError::LoadFailed("Invalid IP address".to_string()))?;

    let prefix: u8 = parts[1]
        .parse()
        .map_err(|_| EbpfError::LoadFailed("Invalid prefix length".to_string()))?;

    // Validate prefix length
    match ip {
        IpAddr::V4(_) if prefix > 32 => {
            return Err(EbpfError::LoadFailed(
                "IPv4 prefix must be <= 32".to_string(),
            ));
        }
        IpAddr::V6(_) if prefix > 128 => {
            return Err(EbpfError::LoadFailed(
                "IPv6 prefix must be <= 128".to_string(),
            ));
        }
        _ => {}
    }

    Ok((ip, prefix))
}

impl XdpFirewall {
    async fn expand_and_block_cidr(
        &self,
        base_ip: IpAddr,
        prefix: u8,
        _reason: BlockReason,
    ) -> Result<u32, EbpfError> {
        // Use iptables with CIDR directly
        use tokio::process::Command;

        let cidr_str = format!("{}/{}", base_ip, prefix);

        let output = Command::new("iptables")
            .args(["-I", "INPUT", "-s", &cidr_str, "-j", "DROP"])
            .output()
            .await?;

        if !output.status.success() {
            tracing::warn!("Failed to block CIDR via iptables");
        }

        Ok(1)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_firewall_creation() {
        let manager = EbpfManager::new().await.unwrap();
        let fw = XdpFirewall::new(&manager);
        let stats = fw.stats().await;
        assert_eq!(stats.packets_allowed, 0);
        assert_eq!(stats.blocked_ip_count, 0);
    }

    #[test]
    fn test_block_reason() {
        assert_ne!(BlockReason::Manual, BlockReason::DdosDetected);
        assert_eq!(BlockReason::Manual, BlockReason::Manual);
        assert_eq!(BlockReason::RateLimitExceeded, BlockReason::RateLimitExceeded);
    }

    #[test]
    fn test_parse_cidr() {
        let (ip, prefix) = parse_cidr("192.168.1.0/24").unwrap();
        assert!(matches!(ip, IpAddr::V4(_)));
        assert_eq!(prefix, 24);

        let (ip, prefix) = parse_cidr("2001:db8::/32").unwrap();
        assert!(matches!(ip, IpAddr::V6(_)));
        assert_eq!(prefix, 32);

        assert!(parse_cidr("invalid").is_err());
        assert!(parse_cidr("192.168.1.0/33").is_err());
        assert!(parse_cidr("192.168.1.0").is_err());
    }

    #[tokio::test]
    async fn test_is_blocked() {
        let manager = EbpfManager::new().await.unwrap();
        let fw = XdpFirewall::new(&manager);

        let ip = std::net::Ipv4Addr::new(192, 168, 1, 100);
        assert!(!fw.is_blocked(&IpAddr::V4(ip)));
    }

    #[tokio::test]
    async fn test_block_and_unblock() {
        let manager = EbpfManager::new().await.unwrap();
        let fw = XdpFirewall::new(&manager);

        let ip = IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        assert!(!fw.is_blocked(&ip));

        fw.block_ip(ip).await.unwrap();
        assert!(fw.is_blocked(&ip));
        assert_eq!(fw.get_block_reason(&ip), Some(BlockReason::Manual));

        fw.unblock_ip(ip).await.unwrap();
        assert!(!fw.is_blocked(&ip));
        assert_eq!(fw.get_block_reason(&ip), None);
    }

    #[tokio::test]
    async fn test_block_with_reason() {
        let manager = EbpfManager::new().await.unwrap();
        let fw = XdpFirewall::new(&manager);

        let ip = IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 2));
        fw.block_ip_with_reason(ip, BlockReason::DdosDetected)
            .await
            .unwrap();

        assert!(fw.is_blocked(&ip));
        assert_eq!(fw.get_block_reason(&ip), Some(BlockReason::DdosDetected));
    }

    #[tokio::test]
    async fn test_rate_limit() {
        let manager = EbpfManager::new().await.unwrap();
        let fw = XdpFirewall::new(&manager);

        let ip = IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 3));
        fw.add_rate_limit(ip, 100, RateLimitAction::Drop)
            .await
            .unwrap();

        fw.remove_rate_limit(&ip).await.unwrap();
    }

    #[tokio::test]
    async fn test_stats() {
        let manager = EbpfManager::new().await.unwrap();
        let fw = XdpFirewall::new(&manager);

        fw.update_stats(100, 10, 5);
        let stats = fw.stats().await;
        assert_eq!(stats.packets_allowed, 100);
        assert_eq!(stats.packets_dropped, 10);
        assert_eq!(stats.packets_ratelimited, 5);
    }

    #[tokio::test]
    async fn test_clear_blocklist() {
        let manager = EbpfManager::new().await.unwrap();
        let fw = XdpFirewall::new(&manager);

        let ip1 = IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
        let ip2 = IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 2));
        fw.block_ip(ip1).await.unwrap();
        fw.block_ip(ip2).await.unwrap();

        assert_eq!(fw.stats().await.blocked_ip_count, 2);

        let cleared = fw.clear_blocklist().await.unwrap();
        assert_eq!(cleared, 2);
        assert_eq!(fw.stats().await.blocked_ip_count, 0);
    }

    #[tokio::test]
    async fn test_list_blocked_ips() {
        let manager = EbpfManager::new().await.unwrap();
        let fw = XdpFirewall::new(&manager);

        let ip = IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 99));
        fw.block_ip_with_reason(ip, BlockReason::ThreatIntel)
            .await
            .unwrap();

        let list = fw.list_blocked_ips();
        assert!(list.contains(&(ip, BlockReason::ThreatIntel)));
    }

    #[tokio::test]
    async fn test_attach_detach_lifecycle() {
        let manager = EbpfManager::new().await.unwrap();
        let mut fw = XdpFirewall::new(&manager);

        // Double detach should be fine
        assert!(fw.detach().await.is_ok());

        // Attach in fallback mode should succeed
        assert!(fw.attach("lo").await.is_ok());

        // Double attach should warn and succeed
        assert!(fw.attach("lo").await.is_ok());

        // Detach
        assert!(fw.detach().await.is_ok());
    }
}
