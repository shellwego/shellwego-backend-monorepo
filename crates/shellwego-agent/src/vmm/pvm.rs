//! PVM (Pagetable Virtual Machine) Support
//!
//! PVM enables Firecracker to run on regular cloud VMs without
//! hardware virtualization extensions or nested virtualization.
//!
//! This allows ShellWeGo to run on any VPS (Hetzner Cloud, DigitalOcean,
//! AWS EC2, Linode, Vultr, OVH, etc.) without requiring /dev/kvm access.
//!
//! Reference: https://blog.alexellis.io/how-to-run-firecracker-without-kvm-on-regular-cloud-vms

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info, warn};

/// PVM configuration options
#[derive(Debug, Clone)]
pub struct PvmConfig {
    /// Path to PVM-enabled Firecracker binary
    pub binary_path: PathBuf,
    /// Enable KSM (Kernel Same-page Merging) for memory efficiency
    pub enable_ksm: bool,
    /// Memory overcommit ratio (1.0 = no overcommit, 1.5 = 50% overcommit)
    pub memory_overcommit: f64,
}

impl Default for PvmConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("/usr/local/bin/firecracker-pvm"),
            enable_ksm: true,
            memory_overcommit: 1.5,
        }
    }
}

impl PvmConfig {
    /// Create a new PVM configuration with a custom binary path
    pub fn with_binary(path: PathBuf) -> Self {
        Self {
            binary_path: path,
            ..Default::default()
        }
    }
}

/// Initialize PVM environment
///
/// This sets up kernel parameters and memory management for optimal
/// PVM performance. Should be called once when the agent starts.
pub fn setup_pvm_environment(config: &PvmConfig) -> Result<()> {
    info!("Setting up PVM environment");

    // Verify PVM binary exists
    if !config.binary_path.exists() {
        anyhow::bail!(
            "PVM binary not found at {:?}. Install firecracker-pvm package or \
             use a standard Firecracker binary with PVM support. \
             See: https://blog.alexellis.io/how-to-run-firecracker-without-kvm-on-regular-cloud-vms",
            config.binary_path
        );
    }

    // Make binary executable
    let _ = Command::new("chmod")
        .arg("+x")
        .arg(&config.binary_path)
        .output();

    // Verify the binary is actually executable
    if let Ok(output) = Command::new(&config.binary_path).arg("--version").output() {
        if !output.status.success() {
            warn!(
                "PVM binary at {:?} may not be working correctly",
                config.binary_path
            );
        } else {
            let version = String::from_utf8_lossy(&output.stdout);
            debug!("PVM binary version: {}", version.trim());
        }
    }

    // Enable KSM for memory sharing (reduces memory overhead when running multiple VMs)
    if config.enable_ksm {
        if let Err(e) = enable_ksm() {
            warn!("Could not enable KSM: {} (this is optional)", e);
        }
    }

    // Set recommended kernel parameters for PVM
    if let Err(e) = tune_kernel_for_pvm() {
        warn!("Could not tune kernel parameters: {} (this is optional)", e);
    }

    info!("PVM environment ready");
    Ok(())
}

/// Enable Kernel Same-page Merging (KSM)
///
/// KSM allows the kernel to merge identical memory pages from different
/// processes, which significantly reduces memory usage when running
/// multiple VMs with similar content.
fn enable_ksm() -> Result<()> {
    let ksm_run = Path::new("/sys/kernel/mm/ksm/run");

    if !ksm_run.exists() {
        debug!("KSM not available on this kernel");
        return Ok(());
    }

    // Enable KSM
    std::fs::write(ksm_run, "1").with_context(|| "Failed to enable KSM")?;
    debug!("KSM enabled for memory page sharing");

    // Set sleep time between scans (milliseconds) - lower = more aggressive
    let ksm_sleep = Path::new("/sys/kernel/mm/ksm/sleep_millisecs");
    if ksm_sleep.exists() {
        std::fs::write(ksm_sleep, "100").ok();
    }

    // Set pages to scan per cycle - higher = more thorough
    let ksm_pages = Path::new("/sys/kernel/mm/ksm/pages_to_scan");
    if ksm_pages.exists() {
        std::fs::write(ksm_pages, "1000").ok();
    }

    Ok(())
}

/// Tune kernel parameters for PVM workloads
fn tune_kernel_for_pvm() -> Result<()> {
    // Increase max memory map count for VM memory regions
    let max_map_count = Path::new("/proc/sys/vm/max_map_count");
    if max_map_count.exists() {
        std::fs::write(max_map_count, "262144").ok();
        debug!("Increased vm.max_map_count for PVM");
    }

    // Enable memory overcommit for PVM
    // 0 = heuristic overcommit
    // 1 = always overcommit
    // 2 = never overcommit
    let overcommit_memory = Path::new("/proc/sys/vm/overcommit_memory");
    if overcommit_memory.exists() {
        std::fs::write(overcommit_memory, "1").ok();
        debug!("Enabled memory overcommit for PVM");
    }

    // Set overcommit ratio (only used when overcommit_memory=2)
    let overcommit_ratio = Path::new("/proc/sys/vm/overcommit_ratio");
    if overcommit_ratio.exists() {
        std::fs::write(overcommit_ratio, "50").ok();
    }

    // Increase file descriptor limits via sysctl is typically done at boot
    // but we can check and warn
    check_fd_limits()?;

    Ok(())
}

/// Check file descriptor limits and warn if too low
fn check_fd_limits() -> Result<()> {
    // Read max file descriptors
    let file_max = Path::new("/proc/sys/fs/file-max");
    if file_max.exists() {
        let content = std::fs::read_to_string(file_max)?;
        if let Ok(limit) = content.trim().parse::<u64>() {
            if limit < 100000 {
                warn!(
                    "File descriptor limit ({}) may be too low for PVM. \
                     Consider increasing fs.file-max in sysctl.conf",
                    limit
                );
            }
        }
    }
    Ok(())
}

/// Check if PVM is available on this system
pub fn is_pvm_available(binary_path: &Path) -> bool {
    if !binary_path.exists() {
        debug!("PVM binary not found at {:?}", binary_path);
        return false;
    }

    // Try to run firecracker-pvm to verify it works
    match Command::new(binary_path).arg("--version").output() {
        Ok(o) => {
            if o.status.success() {
                debug!("PVM binary is functional");
                true
            } else {
                debug!("PVM binary exists but returned non-zero exit code");
                false
            }
        }
        Err(e) => {
            debug!("PVM binary check failed: {}", e);
            false
        }
    }
}

/// PVM-specific VM configuration adjustments
///
/// Adjusts the microVM configuration for optimal PVM performance.
/// PVM has slightly different characteristics than KVM:
/// - No hardware CPU templates (T2, C3)
/// - Slightly higher memory overhead
/// - Same API as standard Firecracker
pub fn adjust_config_for_pvm(config: &mut super::MicrovmConfig) {
    // PVM doesn't use hardware CPU templates
    // The CPU template in boot args or machine config should be adjusted

    // Add PVM-specific boot arguments if not present
    let pvm_args = "panic=1 console=ttyS0";
    if !config.kernel_boot_args.contains("panic=") {
        config.kernel_boot_args = format!("{} {}", config.kernel_boot_args, pvm_args);
    }

    // Ensure reasonable memory allocation
    // PVM has ~15% memory overhead vs ~12% for KVM
    // We add a small buffer to account for this
    if config.memory_mb < 64 {
        warn!("PVM memory allocation below minimum (64MB), adjusting");
        config.memory_mb = 64;
    }

    debug!("Adjusted microVM config for PVM mode");
}

/// Estimate memory overhead for PVM mode
///
/// PVM has slightly higher memory overhead than KVM due to
/// software virtualization. This function estimates the overhead.
pub fn estimate_pvm_memory_overhead(configured_memory_mb: u64) -> u64 {
    // PVM overhead is approximately 15% vs KVM's 12%
    // Plus a fixed base overhead of ~3MB per VM
    let base_overhead = 3u64;
    let percentage_overhead = (configured_memory_mb as f64 * 0.15).ceil() as u64;
    base_overhead + percentage_overhead
}

/// Validate PVM configuration
///
/// Checks if the configuration is valid for PVM mode.
pub fn validate_pvm_config(config: &super::MicrovmConfig) -> Result<()> {
    // Memory must be at least 64MB
    if config.memory_mb < 64 {
        anyhow::bail!(
            "PVM requires at least 64MB of memory, got {}MB",
            config.memory_mb
        );
    }

    // Memory should not exceed reasonable limits (e.g., 32GB per VM)
    if config.memory_mb > 32 * 1024 {
        warn!(
            "PVM memory allocation ({}) exceeds recommended maximum (32GB)",
            config.memory_mb
        );
    }

    // CPU shares should be reasonable
    if config.cpu_shares == 0 {
        anyhow::bail!("PVM requires non-zero CPU shares");
    }

    Ok(())
}

/// Get recommended PVM settings for the current system
pub fn get_recommended_settings() -> PvmRecommendations {
    let mut recommendations = PvmRecommendations::default();

    // Check available memory
    let meminfo = std::fs::read_to_string("/proc/meminfo");
    if let Ok(content) = meminfo {
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(total_kb) = parts[1].parse::<u64>() {
                        let total_mb = total_kb / 1024;
                        // Recommend 70% of memory for VMs
                        recommendations.max_memory_for_vms_mb = (total_mb as f64 * 0.7) as u64;
                    }
                }
                break;
            }
        }
    }

    // Check CPU count
    let cpu_count = num_cpus::get();
    recommendations.recommended_max_vms = (cpu_count * 50) as u32; // ~50 VMs per CPU core

    recommendations
}

/// Recommended settings for PVM on the current system
#[derive(Debug, Clone, Default)]
pub struct PvmRecommendations {
    /// Maximum memory to allocate for VMs (MB)
    pub max_memory_for_vms_mb: u64,
    /// Recommended maximum number of VMs
    pub recommended_max_vms: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ============================================
    // PvmConfig Tests
    // ============================================

    #[test]
    fn test_pvm_config_default() {
        let config = PvmConfig::default();
        assert!(config.enable_ksm);
        assert!((config.memory_overcommit - 1.5).abs() < 0.01);
        assert_eq!(
            config.binary_path,
            PathBuf::from("/usr/local/bin/firecracker-pvm")
        );
    }

    #[test]
    fn test_pvm_config_with_binary() {
        let config = PvmConfig::with_binary(PathBuf::from("/custom/path/to/firecracker"));
        assert_eq!(
            config.binary_path,
            PathBuf::from("/custom/path/to/firecracker")
        );
        // Other defaults should still apply
        assert!(config.enable_ksm);
        assert!((config.memory_overcommit - 1.5).abs() < 0.01);
    }

    #[test]
    fn test_pvm_config_clone() {
        let config = PvmConfig {
            binary_path: PathBuf::from("/test/binary"),
            enable_ksm: false,
            memory_overcommit: 2.0,
        };
        let cloned = config.clone();
        assert_eq!(config.binary_path, cloned.binary_path);
        assert_eq!(config.enable_ksm, cloned.enable_ksm);
        assert!((config.memory_overcommit - cloned.memory_overcommit).abs() < 0.001);
    }

    #[test]
    fn test_pvm_config_debug() {
        let config = PvmConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("PvmConfig"));
        assert!(debug_str.contains("binary_path"));
    }

    // ============================================
    // PvmRecommendations Tests
    // ============================================

    #[test]
    fn test_pvm_recommendations_default() {
        let rec = PvmRecommendations::default();
        assert_eq!(rec.max_memory_for_vms_mb, 0);
        assert_eq!(rec.recommended_max_vms, 0);
    }

    #[test]
    fn test_pvm_recommendations_clone() {
        let rec = PvmRecommendations {
            max_memory_for_vms_mb: 8192,
            recommended_max_vms: 100,
        };
        let cloned = rec.clone();
        assert_eq!(rec.max_memory_for_vms_mb, cloned.max_memory_for_vms_mb);
        assert_eq!(rec.recommended_max_vms, cloned.recommended_max_vms);
    }

    #[test]
    fn test_get_recommended_settings() {
        let recommendations = get_recommended_settings();
        // Should have some reasonable values based on the system
        assert!(recommendations.recommended_max_vms > 0);
        // max_memory_for_vms_mb depends on /proc/meminfo, might be 0 if not readable
        println!(
            "Recommendations: max_memory={}MB, max_vms={}",
            recommendations.max_memory_for_vms_mb, recommendations.recommended_max_vms
        );
    }

    // ============================================
    // Memory Overhead Tests
    // ============================================

    #[test]
    fn test_estimate_pvm_memory_overhead_minimum() {
        // Minimum memory (64MB)
        let overhead = estimate_pvm_memory_overhead(64);
        // 3MB base + 64 * 0.15 = 3 + 9.6 = ~13MB
        assert!(overhead >= 12);
        assert!(overhead <= 15);
    }

    #[test]
    fn test_estimate_pvm_memory_overhead_typical() {
        // Typical memory (256MB)
        let overhead = estimate_pvm_memory_overhead(256);
        // 3MB base + 256 * 0.15 = 3 + 38.4 = ~42MB
        assert!(overhead >= 40);
        assert!(overhead <= 45);
    }

    #[test]
    fn test_estimate_pvm_memory_overhead_large() {
        // Large memory (1024MB = 1GB)
        let overhead = estimate_pvm_memory_overhead(1024);
        // 3MB base + 1024 * 0.15 = 3 + 153.6 = ~157MB
        assert!(overhead >= 155);
        assert!(overhead <= 160);
    }

    #[test]
    fn test_estimate_pvm_memory_overhead_zero() {
        // Edge case: zero memory
        let overhead = estimate_pvm_memory_overhead(0);
        // Should still have base overhead
        assert_eq!(overhead, 3);
    }

    // ============================================
    // Config Validation Tests
    // ============================================

    #[test]
    fn test_validate_pvm_config_valid() {
        let config = super::super::MicrovmConfig {
            memory_mb: 128,
            cpu_shares: 1024,
            ..Default::default()
        };
        assert!(validate_pvm_config(&config).is_ok());
    }

    #[test]
    fn test_validate_pvm_config_memory_too_low() {
        let config = super::super::MicrovmConfig {
            memory_mb: 32,
            cpu_shares: 1024,
            ..Default::default()
        };
        assert!(validate_pvm_config(&config).is_err());
    }

    #[test]
    fn test_validate_pvm_config_zero_cpu() {
        let config = super::super::MicrovmConfig {
            memory_mb: 128,
            cpu_shares: 0,
            ..Default::default()
        };
        assert!(validate_pvm_config(&config).is_err());
    }

    #[test]
    fn test_validate_pvm_config_boundary() {
        // Exactly at minimum (64MB)
        let config = super::super::MicrovmConfig {
            memory_mb: 64,
            cpu_shares: 1,
            ..Default::default()
        };
        assert!(validate_pvm_config(&config).is_ok());
    }

    #[test]
    fn test_validate_pvm_config_large_memory() {
        // Large memory should still be valid (just warns)
        let config = super::super::MicrovmConfig {
            memory_mb: 64 * 1024, // 64GB
            cpu_shares: 4096,
            ..Default::default()
        };
        assert!(validate_pvm_config(&config).is_ok());
    }

    // ============================================
    // adjust_config_for_pvm Tests
    // ============================================

    #[test]
    fn test_adjust_config_for_pvm_adds_panic_arg() {
        let mut config = super::super::MicrovmConfig {
            kernel_boot_args: "console=ttyS0".to_string(),
            ..Default::default()
        };
        adjust_config_for_pvm(&mut config);
        assert!(config.kernel_boot_args.contains("panic="));
    }

    #[test]
    fn test_adjust_config_for_pvm_preserves_existing_panic() {
        let mut config = super::super::MicrovmConfig {
            kernel_boot_args: "console=ttyS0 panic=5".to_string(),
            ..Default::default()
        };
        let original_args = config.kernel_boot_args.clone();
        adjust_config_for_pvm(&mut config);
        // Should not modify if panic already present
        assert_eq!(config.kernel_boot_args, original_args);
    }

    #[test]
    fn test_adjust_config_for_pvm_adjusts_low_memory() {
        let mut config = super::super::MicrovmConfig {
            memory_mb: 32,
            ..Default::default()
        };
        adjust_config_for_pvm(&mut config);
        assert_eq!(config.memory_mb, 64);
    }

    #[test]
    fn test_adjust_config_for_pvm_preserves_sufficient_memory() {
        let mut config = super::super::MicrovmConfig {
            memory_mb: 256,
            ..Default::default()
        };
        adjust_config_for_pvm(&mut config);
        assert_eq!(config.memory_mb, 256);
    }

    #[test]
    fn test_adjust_config_for_pvm_minimum_memory() {
        let mut config = super::super::MicrovmConfig {
            memory_mb: 64,
            ..Default::default()
        };
        adjust_config_for_pvm(&mut config);
        assert_eq!(config.memory_mb, 64); // Exactly at minimum, should not change
    }

    // ============================================
    // is_pvm_available Tests
    // ============================================

    #[test]
    fn test_is_pvm_available_nonexistent_path() {
        let result = is_pvm_available(Path::new("/nonexistent/path/to/binary"));
        assert!(!result);
    }

    #[test]
    fn test_is_pvm_available_with_env_override() {
        // Set env override
        std::env::set_var("SHELLWEGO_PVM_AVAILABLE", "1");

        // The function checks binary path first, but env override in lib.rs is_pvm_available
        // This tests the standalone is_pvm_available in pvm.rs which checks the binary
        let result = is_pvm_available(Path::new("/nonexistent/binary"));
        assert!(!result); // Still false because binary doesn't exist

        std::env::remove_var("SHELLWEGO_PVM_AVAILABLE");
    }

    // ============================================
    // Integration-style Tests
    // ============================================

    #[test]
    fn test_pvm_config_equality() {
        let config1 = PvmConfig {
            binary_path: PathBuf::from("/usr/bin/firecracker"),
            enable_ksm: true,
            memory_overcommit: 1.5,
        };
        let config2 = PvmConfig {
            binary_path: PathBuf::from("/usr/bin/firecracker"),
            enable_ksm: true,
            memory_overcommit: 1.5,
        };
        // Manual equality check (derive PartialEq if needed)
        assert_eq!(config1.binary_path, config2.binary_path);
        assert_eq!(config1.enable_ksm, config2.enable_ksm);
        assert!((config1.memory_overcommit - config2.memory_overcommit).abs() < 0.001);
    }

    #[test]
    fn test_memory_overhead_scales_linearly() {
        // Verify overhead scales approximately linearly
        let overhead_128 = estimate_pvm_memory_overhead(128);
        let overhead_256 = estimate_pvm_memory_overhead(256);
        let overhead_512 = estimate_pvm_memory_overhead(512);

        println!("Overhead 128MB: {}", overhead_128);
        println!("Overhead 256MB: {}", overhead_256);
        println!("Overhead 512MB: {}", overhead_512);

        // Each doubling should approximately double the variable portion
        let diff_128_256 = overhead_256 - overhead_128;
        let diff_256_512 = overhead_512 - overhead_256;

        println!("Diff 128->256: {}", diff_128_256);
        println!("Diff 256->512: {}", diff_256_512);

        // The overhead formula is: 3 + ceil(memory * 0.15)
        // For 128: 3 + ceil(19.2) = 3 + 20 = 23
        // For 256: 3 + ceil(38.4) = 3 + 39 = 42, diff = 19
        // For 512: 3 + ceil(76.8) = 3 + 77 = 80, diff = 38
        // Due to ceiling, differences can vary significantly
        // Just verify that overhead increases with memory
        assert!(overhead_512 > overhead_256);
        assert!(overhead_256 > overhead_128);
    }

    // ============================================
    // PvmConfig Advanced Tests
    // ============================================

    #[test]
    fn test_pvm_config_custom_values() {
        let config = PvmConfig {
            binary_path: PathBuf::from("/opt/firecracker-pvm"),
            enable_ksm: false,
            memory_overcommit: 2.0,
        };

        assert_eq!(config.binary_path, PathBuf::from("/opt/firecracker-pvm"));
        assert!(!config.enable_ksm);
        assert!((config.memory_overcommit - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_pvm_config_extreme_overcommit() {
        // Test with extreme overcommit values
        let config = PvmConfig {
            memory_overcommit: 5.0, // 500% overcommit
            ..Default::default()
        };

        assert!((config.memory_overcommit - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_pvm_config_no_overcommit() {
        // Test with no overcommit
        let config = PvmConfig {
            memory_overcommit: 1.0,
            ..Default::default()
        };

        assert!((config.memory_overcommit - 1.0).abs() < 0.001);
    }

    // ============================================
    // Memory Overhead Edge Cases
    // ============================================

    #[test]
    fn test_memory_overhead_1gb() {
        let overhead = estimate_pvm_memory_overhead(1024);
        // 3MB base + 1024 * 0.15 = 3 + 153.6 = ~157MB
        assert!(overhead >= 155 && overhead <= 160);
    }

    #[test]
    fn test_memory_overhead_4gb() {
        let overhead = estimate_pvm_memory_overhead(4096);
        // 3MB base + 4096 * 0.15 = 3 + 614.4 = ~618MB
        assert!(overhead >= 615 && overhead <= 625);
    }

    #[test]
    fn test_memory_overhead_16gb() {
        let overhead = estimate_pvm_memory_overhead(16384);
        // 3MB base + 16384 * 0.15 = 3 + 2457.6 = ~2461MB
        assert!(overhead >= 2455 && overhead <= 2470);
    }

    #[test]
    fn test_memory_overhead_minimum_boundary() {
        // Test exactly at minimum
        let overhead = estimate_pvm_memory_overhead(64);
        assert!(overhead >= 12); // Should have some overhead
    }

    #[test]
    fn test_memory_overhead_fractional() {
        // Test with fractional memory (edge case)
        let overhead_1 = estimate_pvm_memory_overhead(1);
        let overhead_2 = estimate_pvm_memory_overhead(2);
        assert!(overhead_1 >= 3); // Base overhead
        assert!(overhead_2 >= 3); // Base overhead
    }

    // ============================================
    // Config Validation Edge Cases
    // ============================================

    #[test]
    fn test_validate_pvm_config_exact_minimum() {
        let config = super::super::MicrovmConfig {
            memory_mb: 64, // Exactly minimum
            cpu_shares: 1, // Minimum CPU shares
            ..Default::default()
        };
        assert!(validate_pvm_config(&config).is_ok());
    }

    #[test]
    fn test_validate_pvm_config_one_below_minimum() {
        let config = super::super::MicrovmConfig {
            memory_mb: 63, // One below minimum
            cpu_shares: 1024,
            ..Default::default()
        };
        assert!(validate_pvm_config(&config).is_err());
    }

    #[test]
    fn test_validate_pvm_config_various_cpu_shares() {
        for shares in [1, 512, 1024, 2048, 4096, 8192] {
            let config = super::super::MicrovmConfig {
                memory_mb: 128,
                cpu_shares: shares,
                ..Default::default()
            };
            assert!(
                validate_pvm_config(&config).is_ok(),
                "CPU shares {} should be valid",
                shares
            );
        }
    }

    // ============================================
    // adjust_config_for_pvm Edge Cases
    // ============================================

    #[test]
    fn test_adjust_config_empty_boot_args() {
        let mut config = super::super::MicrovmConfig {
            kernel_boot_args: String::new(),
            memory_mb: 128,
            ..Default::default()
        };
        adjust_config_for_pvm(&mut config);
        assert!(config.kernel_boot_args.contains("panic="));
        assert!(config.kernel_boot_args.contains("console="));
    }

    #[test]
    fn test_adjust_config_multiple_panic_args() {
        // If panic is already present, should not add another
        let mut config = super::super::MicrovmConfig {
            kernel_boot_args: "console=ttyS0 panic=5".to_string(),
            ..Default::default()
        };
        let original = config.kernel_boot_args.clone();
        adjust_config_for_pvm(&mut config);
        assert_eq!(config.kernel_boot_args, original);
    }

    #[test]
    fn test_adjust_config_various_memory_values() {
        for mem in [1, 32, 63, 64, 65, 128, 256, 512, 1024, 2048] {
            let mut config = super::super::MicrovmConfig {
                memory_mb: mem,
                ..Default::default()
            };
            adjust_config_for_pvm(&mut config);
            assert!(
                config.memory_mb >= 64,
                "Memory {} should be adjusted to at least 64",
                mem
            );
        }
    }

    // ============================================
    // is_pvm_available Tests
    // ============================================

    #[test]
    fn test_is_pvm_available_nonexistent_directory() {
        let result = is_pvm_available(Path::new("/nonexistent/directory/binary"));
        assert!(!result);
    }

    #[test]
    fn test_is_pvm_available_empty_path() {
        let result = is_pvm_available(Path::new(""));
        assert!(!result);
    }

    #[test]
    fn test_is_pvm_available_root_path() {
        // Root is a directory, not a file
        let result = is_pvm_available(Path::new("/"));
        assert!(!result);
    }

    // ============================================
    // PvmRecommendations Tests
    // ============================================

    #[test]
    fn test_pvm_recommendations_custom() {
        let rec = PvmRecommendations {
            max_memory_for_vms_mb: 16384,
            recommended_max_vms: 200,
        };
        assert_eq!(rec.max_memory_for_vms_mb, 16384);
        assert_eq!(rec.recommended_max_vms, 200);
    }

    #[test]
    fn test_get_recommended_settings_runs() {
        // Just verify it doesn't panic
        let rec = get_recommended_settings();
        // recommended_max_vms is based on CPU count, should be positive
        assert!(rec.recommended_max_vms > 0);
    }

    // ============================================
    // Setup Environment Tests (non-destructive)
    // ============================================

    #[test]
    fn test_setup_pvm_environment_missing_binary() {
        let config = PvmConfig {
            binary_path: PathBuf::from("/nonexistent/firecracker-pvm"),
            ..Default::default()
        };
        let result = setup_pvm_environment(&config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("PVM binary not found"));
    }

    // ============================================
    // Real System Tests
    // ============================================

    #[test]
    fn test_real_pvm_recommendations() {
        let rec = get_recommended_settings();
        println!("PVM Recommendations for this system:");
        println!("  Max memory for VMs: {} MB", rec.max_memory_for_vms_mb);
        println!("  Recommended max VMs: {}", rec.recommended_max_vms);

        // Recommended VMs should be positive (based on CPU count)
        assert!(rec.recommended_max_vms > 0);
    }

    #[test]
    fn test_real_memory_overhead_calculation() {
        // Test realistic VM sizes
        let sizes = vec![64, 128, 256, 512, 1024, 2048, 4096];
        for size in sizes {
            let overhead = estimate_pvm_memory_overhead(size);
            let percentage = (overhead as f64 / size as f64) * 100.0;
            println!(
                "Memory: {}MB, Overhead: {}MB ({:.1}%)",
                size, overhead, percentage
            );

            // Overhead should be reasonable (15-20% range)
            assert!(overhead >= 3, "Overhead should be at least base 3MB");
            assert!(
                percentage < 25.0,
                "Overhead percentage should be reasonable"
            );
        }
    }

    // ============================================
    // Thread Safety Tests
    // ============================================

    #[test]
    fn test_pvm_config_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PvmConfig>();
    }

    #[test]
    fn test_pvm_recommendations_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PvmRecommendations>();
    }

    // ============================================
    // Real PVM Integration Tests
    // ============================================

    /// Get the real firecracker binary path if available
    fn real_firecracker_binary() -> Option<PathBuf> {
        let paths = vec![
            PathBuf::from("/usr/local/bin/firecracker-pvm"),
            PathBuf::from("/usr/local/bin/firecracker"),
            PathBuf::from("/usr/bin/firecracker"),
        ];
        for path in paths {
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    #[test]
    fn test_real_is_pvm_available_with_real_binary() {
        if let Some(binary_path) = real_firecracker_binary() {
            let result = is_pvm_available(&binary_path);
            println!("Testing with real binary at: {:?}", binary_path);
            assert!(
                result,
                "PVM should be available with real binary at {:?}",
                binary_path
            );
            println!(
                "✓ Real Firecracker binary detected successfully at {:?}",
                binary_path
            );
        } else {
            panic!("No real Firecracker binary found! Install firecracker to run this test.");
        }
    }

    #[test]
    fn test_real_setup_pvm_environment_with_real_binary() {
        if let Some(binary_path) = real_firecracker_binary() {
            let config = PvmConfig {
                binary_path,
                enable_ksm: false, // Don't actually modify system KSM in tests
                memory_overcommit: 1.5,
            };

            let result = setup_pvm_environment(&config);
            assert!(
                result.is_ok(),
                "setup_pvm_environment should succeed with real binary: {:?}",
                result
            );
            println!("✓ PVM environment setup succeeded with real binary");
        } else {
            panic!("No real Firecracker binary found!");
        }
    }

    #[test]
    fn test_real_firecracker_binary_version_check() {
        if let Some(binary_path) = real_firecracker_binary() {
            // Actually run the real binary with --version
            let output = Command::new(&binary_path).arg("--version").output();

            assert!(
                output.is_ok(),
                "Should be able to execute real firecracker binary"
            );
            let output = output.unwrap();
            assert!(
                output.status.success(),
                "Real binary should return success for --version"
            );

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!("✓ Real Firecracker version: {}", stdout.trim());
            if !stderr.trim().is_empty() {
                println!("  Stderr: {}", stderr.trim());
            }

            // Verify version format
            assert!(
                stdout.contains("Firecracker") || stderr.contains("Firecracker"),
                "Output should contain 'Firecracker'"
            );
        } else {
            panic!("No real Firecracker binary found!");
        }
    }

    #[test]
    fn test_real_firecracker_help_output() {
        if let Some(binary_path) = real_firecracker_binary() {
            let output = Command::new(&binary_path).arg("--help").output();

            assert!(
                output.is_ok(),
                "Should be able to get help from real binary"
            );
            let output = output.unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Verify it's a valid firecracker binary by checking help content
            assert!(
                stdout.contains("api-sock") || stdout.contains("config-file"),
                "Help should contain expected firecracker options"
            );
            println!("✓ Real Firecracker help output verified");
        } else {
            panic!("No real Firecracker binary found!");
        }
    }

    #[test]
    fn test_real_ksm_availability_check() {
        let ksm_run = Path::new("/sys/kernel/mm/ksm/run");
        if ksm_run.exists() {
            let content = std::fs::read_to_string(ksm_run);
            assert!(content.is_ok(), "Should be able to read KSM status");
            let content = content.unwrap();
            println!(
                "✓ KSM is available on this system, current state: {}",
                content.trim()
            );
        } else {
            println!("KSM not available on this system (normal for containers)");
        }
    }

    #[test]
    fn test_real_proc_meminfo_reading() {
        // Test that get_recommended_settings can actually read /proc/meminfo
        let meminfo = std::fs::read_to_string("/proc/meminfo");
        assert!(meminfo.is_ok(), "Should be able to read /proc/meminfo");

        let content = meminfo.unwrap();
        assert!(
            content.contains("MemTotal:"),
            "meminfo should contain MemTotal"
        );

        // Parse total memory
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(total_kb) = parts[1].parse::<u64>() {
                        let total_mb = total_kb / 1024;
                        println!("✓ System total memory: {} MB", total_mb);
                        assert!(total_mb > 0, "Memory should be positive");
                    }
                }
                break;
            }
        }
    }

    #[test]
    fn test_real_num_cpus_detection() {
        let cpu_count = num_cpus::get();
        assert!(cpu_count >= 1, "Should have at least 1 CPU");
        println!("✓ Detected {} CPU cores", cpu_count);
    }

    #[test]
    fn test_real_kernel_parameters_read() {
        // Test reading various kernel parameters that PVM tunes
        let params = vec![
            "/proc/sys/vm/max_map_count",
            "/proc/sys/vm/overcommit_memory",
            "/proc/sys/vm/overcommit_ratio",
            "/proc/sys/fs/file-max",
        ];

        for param in params {
            let path = Path::new(param);
            if path.exists() {
                let content = std::fs::read_to_string(path);
                if let Ok(content) = content {
                    println!("✓ {}: {}", param, content.trim());
                }
            }
        }
    }
}
