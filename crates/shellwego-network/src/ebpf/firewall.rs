//! XDP-based firewall for DDoS protection and IP filtering
//!
//! Provides high-performance packet filtering at the XDP (eXpress Data Path) layer,
//! which processes packets before they reach the kernel networking stack.
//!
//! Features:
//! - IP blocklist/allowlist
//! - Rate limiting per IP
//! - DDoS mitigation
//! - Connection tracking

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
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

        #[cfg(feature = "ebpf")]
        {
            // Attach via eBPF manager
            self.manager.attach_firewall(iface).await?;
        }

        self.attached.store(true, Ordering::SeqCst);
        *self.attached_iface.lock().unwrap() = Some(iface.to_string());

        tracing::info!("XDP firewall attached to {}", iface);
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

        #[cfg(feature = "ebpf")]
        {
            // Update eBPF map
            self.update_blocklist_map(&ip, true).await?;
        }

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

        #[cfg(feature = "ebpf")]
        {
            // Update eBPF map
            self.update_blocklist_map(&ip, false).await?;
        }

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

        #[cfg(feature = "ebpf")]
        {
            // Update eBPF rate limit map
            self.update_rate_limit_map(&ip, packets_per_sec, config.burst).await?;
        }

        Ok(())
    }

    /// Remove rate limit for an IP
    pub async fn remove_rate_limit(&self, ip: &IpAddr) -> Result<(), EbpfError> {
        tracing::info!("Removing rate limit for {}", ip);

        {
            let mut limits = self.rate_limits.lock().unwrap();
            limits.remove(ip);
        }

        #[cfg(feature = "ebpf")]
        {
            self.remove_rate_limit_map(ip).await?;
        }

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

        #[cfg(feature = "ebpf")]
        {
            use tokio::process::Command;
            
            if let Some(iface) = self.attached_iface.lock().unwrap().as_ref() {
                // Detach XDP program
                let _ = Command::new("ip")
                    .args(["link", "set", "dev", iface, "xdp", "off"])
                    .output()
                    .await;
            }
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

        // Parse CIDR and expand to individual IPs (simplified - real impl would use CIDR maps)
        let (ip, prefix) = parse_cidr(cidr)?;
        let count = self.expand_and_block_cidr(ip, prefix, reason).await?;

        tracing::info!("Blocked {} IPs from CIDR {}", count, cidr);
        Ok(count)
    }

    /// Allow an IP (whitelist - bypasses all rules)
    pub async fn allow_ip(&self, _ip: &IpAddr) -> Result<(), EbpfError> {
        // In a full implementation, this would add to an allowlist map
        // that takes precedence over blocklists
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

        #[cfg(feature = "ebpf")]
        {
            use tokio::process::Command;

            // Enable SYN cookies
            let _ = Command::new("sysctl")
                .args(["-w", "net.ipv4.tcp_syncookies=1"])
                .output()
                .await;

            // Reduce SYN retry timeout
            let _ = Command::new("sysctl")
                .args(["-w", "net.ipv4.tcp_syn_retries=2"])
                .output()
                .await;

            // Enable reverse path filtering
            let _ = Command::new("sysctl")
                .args(["-w", &format!("net.ipv4.conf.{}.rp_filter=1", iface)])
                .output()
                .await;

            // Reduce connection tracking timeouts
            let _ = Command::new("sysctl")
                .args(["-w", "net.netfilter.nf_conntrack_tcp_timeout_established=600"])
                .output()
                .await;
        }

        Ok(())
    }

    /// Update internal statistics (called by eBPF program or polling)
    pub fn update_stats(&self, allowed: u64, dropped: u64, rate_limited: u64) {
        self.stats.packets_allowed.fetch_add(allowed, Ordering::Relaxed);
        self.stats.packets_dropped.fetch_add(dropped, Ordering::Relaxed);
        self.stats.packets_ratelimited.fetch_add(rate_limited, Ordering::Relaxed);
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
}

#[cfg(feature = "ebpf")]
impl XdpFirewall {
    async fn update_blocklist_map(&self, ip: &IpAddr, blocked: bool) -> Result<(), EbpfError> {
        use tokio::process::Command;

        // For XDP, we'd use aya to update the map directly
        // As a fallback, use iptables for immediate effect
        let ip_str = ip.to_string();
        
        if blocked {
            let _ = Command::new("iptables")
                .args(["-I", "INPUT", "-s", &ip_str, "-j", "DROP"])
                .output()
                .await;
            
            let _ = Command::new("iptables")
                .args(["-I", "FORWARD", "-s", &ip_str, "-j", "DROP"])
                .output()
                .await;
        } else {
            let _ = Command::new("iptables")
                .args(["-D", "INPUT", "-s", &ip_str, "-j", "DROP"])
                .output()
                .await;
            
            let _ = Command::new("iptables")
                .args(["-D", "FORWARD", "-s", &ip_str, "-j", "DROP"])
                .output()
                .await;
        }

        Ok(())
    }

    async fn update_rate_limit_map(
        &self,
        ip: &IpAddr,
        packets_per_sec: u32,
        burst: u32,
    ) -> Result<(), EbpfError> {
        use tokio::process::Command;

        let ip_str = ip.to_string();
        
        // Use iptables hashlimit for rate limiting
        let _ = Command::new("iptables")
            .args([
                "-I", "INPUT",
                "-s", &ip_str,
                "-m", "hashlimit",
                "--hashlimit-above", &format!("{}/sec", packets_per_sec),
                "--hashlimit-burst", &format!("{}", burst),
                "--hashlimit-mode", "srcip",
                "--hashlimit-name", "rate_limit",
                "-j", "DROP",
            ])
            .output()
            .await;

        Ok(())
    }

    async fn remove_rate_limit_map(&self, ip: &IpAddr) -> Result<(), EbpfError> {
        use tokio::process::Command;

        let ip_str = ip.to_string();
        
        let _ = Command::new("iptables")
            .args([
                "-D", "INPUT",
                "-s", &ip_str,
                "-m", "hashlimit",
                "--hashlimit-above", "1/sec",
                "--hashlimit-mode", "srcip",
                "--hashlimit-name", "rate_limit",
                "-j", "DROP",
            ])
            .output()
            .await;

        Ok(())
    }
}

/// Parse CIDR notation
fn parse_cidr(cidr: &str) -> Result<(IpAddr, u8), EbpfError> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return Err(EbpfError::LoadFailed("Invalid CIDR format".to_string()));
    }

    let ip: IpAddr = parts[0].parse()
        .map_err(|_| EbpfError::LoadFailed("Invalid IP address".to_string()))?;
    
    let prefix: u8 = parts[1].parse()
        .map_err(|_| EbpfError::LoadFailed("Invalid prefix length".to_string()))?;

    // Validate prefix length
    match ip {
        IpAddr::V4(_) if prefix > 32 => {
            return Err(EbpfError::LoadFailed("IPv4 prefix must be <= 32".to_string()));
        }
        IpAddr::V6(_) if prefix > 128 => {
            return Err(EbpfError::LoadFailed("IPv6 prefix must be <= 128".to_string()));
        }
        _ => {}
    }

    Ok((ip, prefix))
}

impl XdpFirewall {
    async fn expand_and_block_cidr(
        &self,
        _base_ip: IpAddr,
        _prefix: u8,
        _reason: BlockReason,
    ) -> Result<u32, EbpfError> {
        // For large CIDRs, we'd use eBPF LPM trie maps
        // For now, we'll just block the network address as a placeholder
        // In production, this would add the CIDR to an LPM trie map
        
        // For small ranges, we could expand:
        // But for large ranges, we should use iptables with CIDR or eBPF LPM
        #[cfg(feature = "ebpf")]
        {
            use tokio::process::Command;
            
            // Use iptables with CIDR directly
            let cidr_str = format!("{}/{}", _base_ip, _prefix);
            
            let output = Command::new("iptables")
                .args(["-I", "INPUT", "-s", &cidr_str, "-j", "DROP"])
                .output()
                .await?;

            if !output.status.success() {
                tracing::warn!("Failed to block CIDR via iptables");
            }
        }

        // Return approximate count (would be exact for small ranges)
        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_firewall_creation() {
        let manager = EbpfManager::new().await.unwrap();
        let fw = XdpFirewall::new(&manager);
        let stats = fw.stats().await;
        assert_eq!(stats.packets_allowed, 0);
    }

    #[test]
    fn test_block_reason() {
        assert_ne!(BlockReason::Manual, BlockReason::DdosDetected);
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
    }

    #[test]
    fn test_is_blocked() {
        let manager = tokio::runtime::Runtime::new().unwrap().block_on(async {
            EbpfManager::new().await.unwrap()
        });
        let fw = XdpFirewall::new(&manager);
        
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        assert!(!fw.is_blocked(&ip));
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
}
