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
    // VMM types
    VirtualizationMode, MicrovmConfig, MicrovmState, MicrovmSummary, 
    DriveConfig, NetworkInterface, RateLimiterConfig, MicrovmMetrics,
    // Agent types
    AgentConfig, AgentConfigJson, Capabilities, NodeCapacity,
    // Desired state types
    DesiredState, DesiredApp, DesiredVolume, VolumeMount,
    // Network types
    NetworkConfig, NetworkSetup, NetworkError, Message, QuicConfig,
};

/// Detect system capabilities and determine the best virtualization mode
pub fn detect_capabilities() -> anyhow::Result<Capabilities> {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();

    // Check availability of each backend
    let kvm_available = is_kvm_available();
    let pvm_available = is_pvm_available();
    let wasm_available = is_wasm_available();

    // Determine the best available virtualization mode
    // Priority: KVM (fastest) > PVM (universal) > WASM (lightest)
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
        tracing::warn!("No virtualization backend available. Install KVM, PVM, or enable WASM.");
        VirtualizationMode::default()
    };

    let cpu_cores = sys.cpus().len() as u32;
    let memory_total_mb = sys.total_memory() / 1024 / 1024;

    tracing::info!(
        "Detected capabilities: mode={}, cpu_cores={}, memory={}MB",
        virtualization_mode,
        cpu_cores,
        memory_total_mb
    );

    Ok(Capabilities {
        virtualization_mode,
        kvm_available,
        pvm_available,
        wasm_available,
        cpu_cores,
        memory_total_mb,
        cpu_features: vec![],
    })
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

/// Check if PVM (Pagetable Virtual Machine) is available
pub fn is_pvm_available() -> bool {
    // Check for PVM-enabled Firecracker binary
    let pvm_binary = std::path::Path::new("/usr/local/bin/firecracker-pvm");
    if pvm_binary.exists() {
        return true;
    }

    // Check for PVM kernel module
    let pvm_module = std::path::Path::new("/sys/module/pvm");
    if pvm_module.exists() {
        return true;
    }

    // Check if standard Firecracker has PVM support built-in
    let fc_binary = std::path::Path::new("/usr/local/bin/firecracker");
    if fc_binary.exists() {
        if let Ok(output) = std::process::Command::new(fc_binary)
            .arg("--version")
            .output()
        {
            let version_str = String::from_utf8_lossy(&output.stdout);
            if version_str.contains("pvm") || version_str.contains("PVM") {
                return true;
            }
        }
    }

    // Check environment variable override
    std::env::var("SHELLWEGO_PVM_AVAILABLE")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
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
    fn test_is_pvm_available_without_binary() {
        // On systems without firecracker-pvm binary
        let result = is_pvm_available();
        // We can't assert true/false since it depends on the system
        // But we verify it doesn't panic
        println!("PVM available: {}", result);
    }
}
