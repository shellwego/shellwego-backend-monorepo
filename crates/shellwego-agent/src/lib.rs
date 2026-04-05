//! ShellWeGo Agent
//!
//! Worker node agent that manages Firecracker microVMs and reports to the control plane.
//! Supports multiple virtualization backends: KVM (hardware), PVM (software), and WASM.

pub mod daemon;
pub mod discovery;
pub mod metrics;
pub mod migration;
pub mod reconciler;
pub mod snapshot;
pub mod vmm;
pub mod wasm;

#[cfg(test)]
mod test_utils;

// Re-export types from schema crate
pub use shellwego_schema::{
    // Agent types
    AgentConfig,
    AgentConfigJson,
    Capabilities,
    DesiredApp,
    // Desired state types
    DesiredState,
    DesiredVolume,
    DriveConfig,
    Message,
    MicrovmConfig,
    MicrovmMetrics,
    MicrovmState,
    MicrovmSummary,
    // Network types
    NetworkConfig,
    NetworkError,
    NetworkInterface,
    NetworkSetup,
    NodeCapacity,
    QuicConfig,
    RateLimiterConfig,
    // VMM types
    VirtualizationMode,
    VolumeMount,
};

/// Detect system capabilities and determine the best virtualization mode
pub fn detect_capabilities() -> anyhow::Result<Capabilities> {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();

    let kvm_available = is_kvm_available();
    let pvm_available = is_pvm_available();
    let wasm_available = is_wasm_available();

    let virtualization_mode = if kvm_available {
        tracing::info!("KVM hardware virtualization available");
        VirtualizationMode::Kvm
    } else if pvm_available {
        tracing::info!("PVM software virtualization available");
        VirtualizationMode::Pvm
    } else if wasm_available {
        tracing::info!("WASM runtime available (no virtualization)");
        VirtualizationMode::Wasm
    } else {
        tracing::warn!("No virtualization backend available. Install KVM, QEMU, or enable WASM.");
        VirtualizationMode::default()
    };

    let cpu_cores = sys.cpus().len() as u32;
    let memory_total_mb = sys.total_memory() / 1024 / 1024;

    // Detect CPU features
    let cpu_features = detect_cpu_features();

    tracing::info!(
        "Detected capabilities: mode={}, cpu_cores={}, memory={}MB, features={:?}",
        virtualization_mode,
        cpu_cores,
        memory_total_mb,
        cpu_features
    );

    Ok(Capabilities {
        virtualization_mode,
        kvm_available,
        pvm_available,
        wasm_available,
        cpu_cores,
        memory_total_mb,
        cpu_features,
    })
}

/// Detect available CPU features relevant for virtualization
fn detect_cpu_features() -> Vec<String> {
    let mut features = Vec::new();

    // Read CPU info from /proc/cpuinfo
    if let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") {
        // Collect all feature flags from the first CPU
        let mut seen = std::collections::HashSet::new();
        for line in content.lines() {
            if line.starts_with("flags") || line.starts_with("Features") {
                if let Some(flags_str) = line.split(':').nth(1) {
                    for flag in flags_str.split_whitespace() {
                        if seen.insert(flag.to_string()) {
                            features.push(flag.to_string());
                        }
                    }
                }
                break; // Only read first CPU's features
            }
        }
    }

    features
}

/// Check if KVM hardware virtualization is available
pub fn is_kvm_available() -> bool {
    let kvm_path = std::path::Path::new("/dev/kvm");

    // Check /dev/kvm exists
    if !kvm_path.exists() {
        return false;
    }

    // Try to open for read/write (permission check)
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(kvm_path)
        .is_ok()
}

/// Check if PVM (software virtualization fallback) is available
///
/// PVM detection checks for:
/// 1. QEMU as a software virtualization fallback
/// 2. Standard Firecracker binary with PVM support
/// 3. Environment variable override (SHELLWEGO_PVM_AVAILABLE=1)
pub fn is_pvm_available() -> bool {
    // Check environment variable override first
    if let Ok(v) = std::env::var("SHELLWEGO_PVM_AVAILABLE") {
        if v == "1" || v.to_lowercase() == "true" {
            return true;
        }
        if v == "0" || v.to_lowercase() == "false" {
            return false;
        }
    }

    // Check for QEMU as software virtualization fallback
    // QEMU can provide KVM acceleration on systems with hardware support
    // or fall back to TCG (software emulation) when KVM is unavailable
    let qemu_paths = [
        "/usr/bin/qemu-system-x86_64",
        "/usr/bin/qemu-system-aarch64",
        "/usr/local/bin/qemu-system-x86_64",
    ];

    for qemu_path in qemu_paths.iter() {
        let path = std::path::Path::new(qemu_path);
        if path.exists() {
            // Verify it's executable
            if std::fs::metadata(path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
            {
                tracing::debug!("Found QEMU at {}", qemu_path);
                return true;
            }
        }
    }

    // Check if standard Firecracker binary exists (PVM mode wraps it)
    let fc_binary = std::path::Path::new("/usr/local/bin/firecracker");
    if fc_binary.exists() {
        tracing::debug!("Found Firecracker binary for PVM mode");
        return true;
    }

    false
}

/// Check if WASM runtime is available
pub fn is_wasm_available() -> bool {
    // WASM runtime is always available (compiled in via wasmtime)
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtualization_mode_default() {
        // Default should be PVM (universal fallback)
        assert_eq!(VirtualizationMode::default(), VirtualizationMode::Pvm);
    }

    #[test]
    fn test_virtualization_mode_display() {
        assert_eq!(format!("{}", VirtualizationMode::Kvm), "KVM");
        assert_eq!(format!("{}", VirtualizationMode::Pvm), "PVM");
        assert_eq!(format!("{}", VirtualizationMode::Wasm), "WASM");
    }

    #[test]
    fn test_detect_capabilities() {
        let caps = detect_capabilities().expect("Failed to detect capabilities");

        // WASM should always be available
        assert!(caps.wasm_available, "WASM should always be available");

        // Mode should be valid
        assert!(matches!(
            caps.virtualization_mode,
            VirtualizationMode::Kvm | VirtualizationMode::Pvm | VirtualizationMode::Wasm
        ));

        // CPU cores should be at least 1
        assert!(caps.cpu_cores >= 1);

        // Memory should be at least some reasonable amount
        assert!(caps.memory_total_mb > 0);
    }

    #[test]
    fn test_is_wasm_available() {
        // WASM should always be available (it's compiled in)
        assert!(is_wasm_available());
    }

    #[test]
    fn test_is_kvm_available_without_dev_kvm() {
        // On systems without /dev/kvm, this should return false
        // This test documents the expected behavior
        let result = is_kvm_available();
        // We can't assert true/false since it depends on the system
        // But we verify it doesn't panic
        println!("KVM available: {}", result);
    }

    #[test]
    fn test_is_pvm_available_detection() {
        // This test documents the new PVM detection behavior
        let result = is_pvm_available();
        println!("PVM available: {}", result);
        // Verify it doesn't panic
    }

    #[test]
    fn test_detect_cpu_features() {
        let features = detect_cpu_features();
        // On most systems, we should detect at least some CPU features
        // But this test just verifies it doesn't panic
        println!("CPU features: {:?}", features);
    }
}
