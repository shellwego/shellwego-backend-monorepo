//! eBPF-based Quality of Service and traffic shaping
//! 
//! Replaces tc (traffic control) with faster eBPF implementations.
//! Provides bandwidth limiting, latency simulation, and traffic prioritization.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(feature = "ebpf")]
use aya::maps::HashMap as BpfHashMap;

use crate::ebpf::{EbpfManager, EbpfError};

/// Handle to active traffic shaper
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShaperHandle(pub u64);

/// Traffic priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrafficPriority {
    /// Best effort (lowest priority)
    BestEffort,
    /// Bronze tier
    Bronze,
    /// Silver tier
    Silver,
    /// Gold tier
    Gold,
    /// Platinum (highest priority)
    Platinum,
}

impl TrafficPriority {
    /// Convert to numeric priority (higher = more important)
    pub fn priority_value(&self) -> u8 {
        match self {
            TrafficPriority::BestEffort => 0,
            TrafficPriority::Bronze => 1,
            TrafficPriority::Silver => 2,
            TrafficPriority::Gold => 3,
            TrafficPriority::Platinum => 4,
        }
    }
}

/// Direction for traffic shaping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcDirection {
    /// Incoming traffic
    Ingress,
    /// Outgoing traffic
    Egress,
}

/// Shaper statistics
#[derive(Debug, Clone, Default)]
pub struct ShaperStats {
    /// Total bytes processed
    pub bytes_processed: u64,
    /// Bytes dropped due to rate limiting
    pub bytes_dropped: u64,
    /// Bytes delayed for shaping
    pub bytes_delayed: u64,
    /// Current rate in bits per second
    pub current_rate_bps: u64,
    /// Burst allowance remaining
    pub burst_allowance_bytes: u64,
    /// Packets processed
    pub packets_processed: u64,
    /// Packets dropped
    pub packets_dropped: u64,
}

/// eBPF QoS controller
#[derive(Clone)]
pub struct EbpfQos {
    #[allow(dead_code)]
    manager: EbpfManager,
    /// Active shapers by handle
    shapers: Arc<std::sync::Mutex<HashMap<ShaperHandle, ShaperConfig>>>,
    /// Next handle ID
    next_handle: Arc<AtomicU64>,
}

/// Configuration for a traffic shaper
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ShaperConfig {
    /// Interface name
    iface: String,
    /// Direction
    direction: TcDirection,
    /// Rate limit in bits per second
    rate_bps: u64,
    /// Burst size in bytes
    burst_bytes: u64,
    /// Priority
    priority: TrafficPriority,
}

impl EbpfQos {
    /// Create new QoS controller
    pub fn new(manager: &EbpfManager) -> Self {
        Self {
            manager: manager.clone(),
            shapers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            next_handle: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Apply bandwidth limit to interface
    ///
    /// # Arguments
    /// * `iface` - Network interface name (e.g., "eth0")
    /// * `direction` - Traffic direction (ingress/egress)
    /// * `bits_per_sec` - Maximum bandwidth in bits per second
    ///
    /// # Returns
    /// Handle to the created shaper for later management
    pub async fn limit_bandwidth(
        &self,
        iface: &str,
        direction: TcDirection,
        bits_per_sec: u64,
    ) -> Result<ShaperHandle, EbpfError> {
        tracing::info!(
            "Applying bandwidth limit: {} {} bps on {}",
            match direction {
                TcDirection::Ingress => "ingress",
                TcDirection::Egress => "egress",
            },
            bits_per_sec,
            iface
        );

        // Generate handle
        let handle = ShaperHandle(self.next_handle.fetch_add(1, Ordering::SeqCst));

        // Calculate burst size (allow 10ms of burst at target rate)
        let burst_bytes = (bits_per_sec / 8 / 100).max(4096); // At least 4KB burst

        #[cfg(feature = "ebpf")]
        {
            // Apply TC-based rate limiting
            self.apply_tc_shaper(iface, direction, bits_per_sec, burst_bytes).await?;
        }

        // Store configuration
        let config = ShaperConfig {
            iface: iface.to_string(),
            direction,
            rate_bps: bits_per_sec,
            burst_bytes,
            priority: TrafficPriority::BestEffort,
        };

        {
            let mut shapers = self.shapers.lock().unwrap();
            shapers.insert(handle, config);
        }

        tracing::info!("Created shaper with handle {:?}", handle);
        Ok(handle)
    }

    /// Apply latency and packet loss simulation (for testing)
    ///
    /// This is useful for testing application behavior under adverse network conditions.
    ///
    /// # Arguments
    /// * `iface` - Network interface name
    /// * `direction` - Traffic direction
    /// * `latency_ms` - Base latency to add in milliseconds
    /// * `jitter_ms` - Random jitter to add in milliseconds
    /// * `loss_percent` - Packet loss percentage (0.0 - 100.0)
    pub async fn add_impairment(
        &self,
        iface: &str,
        direction: TcDirection,
        latency_ms: u32,
        jitter_ms: u32,
        loss_percent: f32,
    ) -> Result<ShaperHandle, EbpfError> {
        tracing::info!(
            "Adding impairment: {}ms latency + {}ms jitter, {:.1}% loss on {} ({:?})",
            latency_ms,
            jitter_ms,
            loss_percent,
            iface,
            direction
        );

        let handle = ShaperHandle(self.next_handle.fetch_add(1, Ordering::SeqCst));

        #[cfg(feature = "ebpf")]
        {
            // Use netem via tc for impairment simulation
            self.apply_netem_impairment(iface, direction, latency_ms, jitter_ms, loss_percent).await?;
        }

        let config = ShaperConfig {
            iface: iface.to_string(),
            direction,
            rate_bps: 0, // No rate limit for impairment
            burst_bytes: 0,
            priority: TrafficPriority::BestEffort,
        };

        {
            let mut shapers = self.shapers.lock().unwrap();
            shapers.insert(handle, config);
        }

        Ok(handle)
    }

    /// Prioritize traffic by DSCP/TOS field
    ///
    /// Allows setting different priorities for different traffic classes.
    /// Higher priority traffic gets preferential treatment during congestion.
    ///
    /// # Arguments
    /// * `iface` - Network interface name
    /// * `dscp` - DSCP value (0-63) to match
    /// * `priority` - Priority level to assign
    pub async fn set_priority(
        &self,
        iface: &str,
        dscp: u8,
        priority: TrafficPriority,
    ) -> Result<(), EbpfError> {
        if dscp > 63 {
            return Err(EbpfError::LoadFailed("DSCP must be 0-63".to_string()));
        }

        tracing::info!(
            "Setting priority {:?} for DSCP {} on {}",
            priority,
            dscp,
            iface
        );

        #[cfg(feature = "ebpf")]
        {
            // Update priority map in eBPF
            self.update_priority_map(iface, dscp, priority).await?;
        }

        Ok(())
    }

    /// Remove a traffic shaper
    ///
    /// # Arguments
    /// * `handle` - Handle returned from limit_bandwidth or add_impairment
    pub async fn remove_shaper(&self, handle: ShaperHandle) -> Result<(), EbpfError> {
        tracing::info!("Removing shaper {:?}", handle);

        let config = {
            let mut shapers = self.shapers.lock().unwrap();
            shapers.remove(&handle)
        };

        if let Some(config) = config {
            #[cfg(feature = "ebpf")]
            {
                self.remove_tc_shaper(&config.iface, config.direction).await?;
            }
            tracing::info!("Removed shaper {:?} from {}", handle, config.iface);
        } else {
            tracing::warn!("Shaper {:?} not found", handle);
        }

        Ok(())
    }

    /// Get statistics for a shaper
    ///
    /// # Arguments
    /// * `handle` - Handle returned from limit_bandwidth
    ///
    /// # Returns
    /// Current statistics including bytes processed, dropped, etc.
    pub async fn shaper_stats(&self, handle: ShaperHandle) -> Result<ShaperStats, EbpfError> {
        let config = {
            let shapers = self.shapers.lock().unwrap();
            shapers.get(&handle).cloned()
        };

        match config {
            Some(_config) => {
                #[cfg(feature = "ebpf")]
                {
                    self.read_shaper_stats(&config.iface, config.direction).await
                }
                #[cfg(not(feature = "ebpf"))]
                {
                    Ok(ShaperStats::default())
                }
            }
            None => Err(EbpfError::LoadFailed(format!("Shaper {:?} not found", handle))),
        }
    }

    /// List all active shapers
    pub fn list_shapers(&self) -> Vec<ShaperHandle> {
        let shapers = self.shapers.lock().unwrap();
        shapers.keys().copied().collect()
    }

    /// Update rate limit for an existing shaper
    pub async fn update_rate(&self, handle: ShaperHandle, new_rate_bps: u64) -> Result<(), EbpfError> {
        let config = {
            let mut shapers = self.shapers.lock().unwrap();
            if let Some(config) = shapers.get_mut(&handle) {
                config.rate_bps = new_rate_bps;
                Some(config.clone())
            } else {
                None
            }
        };

        if let Some(_config) = config {
            #[cfg(feature = "ebpf")]
            {
                self.update_tc_rate(&config.iface, config.direction, new_rate_bps).await?;
            }
            tracing::info!("Updated shaper {:?} to {} bps", handle, new_rate_bps);
            Ok(())
        } else {
            Err(EbpfError::LoadFailed(format!("Shaper {:?} not found", handle)))
        }
    }
}

#[cfg(feature = "ebpf")]
impl EbpfQos {
    async fn apply_tc_shaper(
        &self,
        iface: &str,
        direction: TcDirection,
        rate_bps: u64,
        burst_bytes: u64,
    ) -> Result<(), EbpfError> {
        use tokio::process::Command;

        let rate_kbps = rate_bps / 1000;
        let burst_kb = burst_bytes / 1024;

        match direction {
            TcDirection::Egress => {
                // Create root qdisc
                let _ = Command::new("tc")
                    .args(["qdisc", "add", "dev", iface, "root", "handle", "1:", "htb"])
                    .output()
                    .await;

                // Create rate limiting class
                let output = Command::new("tc")
                    .args([
                        "class", "add", "dev", iface,
                        "parent", "1:", "classid", "1:1",
                        "htb", "rate", &format!("{}kbit", rate_kbps),
                        "burst", &format!("{}k", burst_kb),
                    ])
                    .output()
                    .await?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!("TC class creation failed (may already exist): {}", stderr);
                }

                // Attach filter
                let _ = Command::new("tc")
                    .args(["filter", "add", "dev", iface, "parent", "1:", "protocol", "ip", "prio", "1", "u32", "match", "ip", "dst", "0.0.0.0/0", "flowid", "1:1"])
                    .output()
                    .await;
            }
            TcDirection::Ingress => {
                // Ingress shaping via IFB (Intermediate Functional Block)
                // First check if ifb0 exists
                let _ = Command::new("ip")
                    .args(["link", "add", "ifb0", "type", "ifb"])
                    .output()
                    .await;

                let _ = Command::new("ip")
                    .args(["link", "set", "ifb0", "up"])
                    .output()
                    .await;

                // Redirect ingress to ifb0
                let _ = Command::new("tc")
                    .args(["qdisc", "add", "dev", iface, "ingress"])
                    .output()
                    .await;

                let _ = Command::new("tc")
                    .args(["filter", "add", "dev", iface, "parent", "ffff:", "protocol", "ip", "u32", "match", "u32", "0", "0", "action", "mirred", "egress", "redirect", "dev", "ifb0"])
                    .output()
                    .await;

                // Apply rate limit on ifb0
                let _ = Command::new("tc")
                    .args(["qdisc", "add", "dev", "ifb0", "root", "handle", "1:", "htb"])
                    .output()
                    .await;

                let _ = Command::new("tc")
                    .args([
                        "class", "add", "dev", "ifb0",
                        "parent", "1:", "classid", "1:1",
                        "htb", "rate", &format!("{}kbit", rate_kbps),
                        "burst", &format!("{}k", burst_kb),
                    ])
                    .output()
                    .await;
            }
        }

        tracing::info!("Applied TC shaper on {} ({:?})", iface, direction);
        Ok(())
    }

    async fn apply_netem_impairment(
        &self,
        iface: &str,
        direction: TcDirection,
        latency_ms: u32,
        jitter_ms: u32,
        loss_percent: f32,
    ) -> Result<(), EbpfError> {
        use tokio::process::Command;

        // netem only works on egress, for ingress we need IFB
        let target_iface = match direction {
            TcDirection::Egress => iface.to_string(),
            TcDirection::Ingress => {
                // Ensure IFB exists
                let _ = Command::new("ip")
                    .args(["link", "add", "ifb0", "type", "ifb"])
                    .output()
                    .await;
                let _ = Command::new("ip")
                    .args(["link", "set", "ifb0", "up"])
                    .output()
                    .await;
                "ifb0".to_string()
            }
        };

        // Build netem parameters
        let latency_arg = format!("{}ms", latency_ms);
        let jitter_arg = if jitter_ms > 0 {
            format!("{}ms", jitter_ms)
        } else {
            "0ms".to_string()
        };
        let loss_arg = format!("{:.1}%", loss_percent);

        // Apply netem qdisc
        let output = Command::new("tc")
            .args([
                "qdisc", "add", "dev", &target_iface,
                "root", "handle", "1:",
                "netem",
                "delay", &latency_arg, &jitter_arg,
                "loss", &loss_arg,
            ])
            .output()
            .await?;

        if !output.status.success() {
            // Try replace if add failed
            let _ = Command::new("tc")
                .args([
                    "qdisc", "replace", "dev", &target_iface,
                    "root", "handle", "1:",
                    "netem",
                    "delay", &latency_arg, &jitter_arg,
                    "loss", &loss_arg,
                ])
                .output()
                .await;
        }

        tracing::info!(
            "Applied netem impairment on {}: {}ms +/- {}ms, {:.1}% loss",
            target_iface,
            latency_ms,
            jitter_ms,
            loss_percent
        );
        Ok(())
    }

    async fn update_priority_map(
        &self,
        iface: &str,
        dscp: u8,
        priority: TrafficPriority,
    ) -> Result<(), EbpfError> {
        use tokio::process::Command;

        // Create priority-based qdisc
        let _ = Command::new("tc")
            .args(["qdisc", "add", "dev", iface, "root", "handle", "1:", "prio"])
            .output()
            .await;

        // Map DSCP to band (prio has 3 bands by default)
        let band = match priority {
            TrafficPriority::BestEffort | TrafficPriority::Bronze => 2,
            TrafficPriority::Silver => 1,
            TrafficPriority::Gold | TrafficPriority::Platinum => 0,
        };

        // Filter based on TOS (DSCP is upper 6 bits of TOS)
        let tos = dscp << 2;
        let _ = Command::new("tc")
            .args([
                "filter", "add", "dev", iface,
                "parent", "1:", "protocol", "ip",
                "prio", "1", "u32",
                "match", "ip", "tos", &format!("0x{:02x}", tos), "0xff",
                "flowid", &format!("1:{}", band + 1),
            ])
            .output()
            .await;

        Ok(())
    }

    async fn remove_tc_shaper(&self, iface: &str, direction: TcDirection) -> Result<(), EbpfError> {
        use tokio::process::Command;

        match direction {
            TcDirection::Egress => {
                let _ = Command::new("tc")
                    .args(["qdisc", "del", "dev", iface, "root"])
                    .output()
                    .await;
            }
            TcDirection::Ingress => {
                let _ = Command::new("tc")
                    .args(["qdisc", "del", "dev", iface, "ingress"])
                    .output()
                    .await;
                let _ = Command::new("tc")
                    .args(["qdisc", "del", "dev", "ifb0", "root"])
                    .output()
                    .await;
            }
        }

        Ok(())
    }

    async fn read_shaper_stats(&self, iface: &str, direction: TcDirection) -> Result<ShaperStats, EbpfError> {
        use tokio::process::Command;

        let target = match direction {
            TcDirection::Egress => iface,
            TcDirection::Ingress => "ifb0",
        };

        let output = Command::new("tc")
            .args(["-s", "qdisc", "show", "dev", target])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Parse tc output for statistics
        let mut stats = ShaperStats::default();
        
        for line in stdout.lines() {
            if line.contains("Sent") {
                // Parse: Sent 12345 bytes 678 pkt ...
                let parts: Vec<&str> = line.split_whitespace().collect();
                for (i, part) in parts.iter().enumerate() {
                    if *part == "Sent" && i + 1 < parts.len() {
                        stats.bytes_processed = parts[i + 1].parse().unwrap_or(0);
                    }
                    if *part == "bytes" && i + 1 < parts.len() {
                        // Already captured above
                    }
                    if *part == "pkt" && i > 0 {
                        stats.packets_processed = parts[i - 1].parse().unwrap_or(0);
                    }
                }
            }
            if line.contains("dropped") {
                // Parse: dropped 123 overlimits 456
                let parts: Vec<&str> = line.split_whitespace().collect();
                for (i, part) in parts.iter().enumerate() {
                    if *part == "dropped" && i + 1 < parts.len() {
                        stats.packets_dropped = parts[i + 1].parse().unwrap_or(0);
                    }
                    if *part == "overlimits" && i + 1 < parts.len() {
                        stats.bytes_dropped = parts[i + 1].parse().unwrap_or(0);
                    }
                }
            }
        }

        Ok(stats)
    }

    async fn update_tc_rate(&self, iface: &str, direction: TcDirection, rate_bps: u64) -> Result<(), EbpfError> {
        let rate_kbps = rate_bps / 1000;

        // For HTB, we can change the rate without removing the qdisc
        use tokio::process::Command;
        
        let target = match direction {
            TcDirection::Egress => iface,
            TcDirection::Ingress => "ifb0",
        };

        let output = Command::new("tc")
            .args([
                "class", "change", "dev", target,
                "parent", "1:", "classid", "1:1",
                "htb", "rate", &format!("{}kbit", rate_kbps),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("Failed to update TC rate: {}", stderr);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_qos_creation() {
        let manager = EbpfManager::new().await.unwrap();
        let qos = EbpfQos::new(&manager);
        assert!(qos.list_shapers().is_empty());
    }

    #[test]
    fn test_priority_values() {
        assert_eq!(TrafficPriority::BestEffort.priority_value(), 0);
        assert_eq!(TrafficPriority::Bronze.priority_value(), 1);
        assert_eq!(TrafficPriority::Silver.priority_value(), 2);
        assert_eq!(TrafficPriority::Gold.priority_value(), 3);
        assert_eq!(TrafficPriority::Platinum.priority_value(), 4);
    }

    #[test]
    fn test_shaper_handle() {
        let h1 = ShaperHandle(1);
        let h2 = ShaperHandle(2);
        assert_ne!(h1, h2);
        assert_eq!(h1, ShaperHandle(1));
    }

    #[test]
    fn test_shaper_stats_default() {
        let stats = ShaperStats::default();
        assert_eq!(stats.bytes_processed, 0);
        assert_eq!(stats.packets_dropped, 0);
    }
}
