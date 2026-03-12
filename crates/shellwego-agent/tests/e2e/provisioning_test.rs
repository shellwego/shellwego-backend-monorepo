//! E2E Provisioning Tests
//!
//! Tests the full microVM provisioning lifecycle with adaptive virtualization backend selection.

use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use shellwego_agent::vmm::{DriveConfig, MicrovmConfig, NetworkInterface, VmmManager};
use shellwego_agent::{detect_capabilities, AgentConfig, VirtualizationMode};
use shellwego_storage::zfs::ZfsManager;

/// Check if Firecracker tests can run (requires KVM or PVM)
fn firecracker_available() -> bool {
    // Standard Firecracker binary
    let fc_binary = PathBuf::from("/usr/local/bin/firecracker");
    if fc_binary.exists() {
        return true;
    }

    // PVM binary
    let pvm_binary = PathBuf::from("/usr/local/bin/firecracker-pvm");
    if pvm_binary.exists() {
        return true;
    }

    false
}

/// Check if ZFS pool is available
fn zfs_available() -> bool {
    std::process::Command::new("zpool")
        .arg("list")
        .arg("shellwego")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Create test configuration
fn test_config() -> AgentConfig {
    AgentConfig {
        node_id: Some(Uuid::new_v4()),
        control_plane_url: "http://localhost".into(),
        join_token: None,
        region: "local".into(),
        zone: "local".into(),
        labels: Default::default(),
        firecracker_binary: PathBuf::from("/usr/local/bin/firecracker"),
        firecracker_pvm_binary: PathBuf::from("/usr/local/bin/firecracker-pvm"),
        kernel_image_path: PathBuf::from("/var/lib/shellwego/vmlinux"),
        data_dir: PathBuf::from("/var/lib/shellwego"),
        max_microvms: 10,
        reserved_memory_mb: 128,
        reserved_cpu_percent: 0.0,
        force_mode: None,
    }
}

/// Print current virtualization mode
fn print_mode() {
    match detect_capabilities() {
        Ok(cap) => {
            println!(
                "Detected virtualization mode: {} (KVM: {}, PVM: {}, WASM: {})",
                cap.virtualization_mode, cap.kvm_available, cap.pvm_available, cap.wasm_available
            );
        }
        Err(e) => {
            println!("Warning: Could not detect capabilities: {}", e);
        }
    }
}

#[tokio::test]
async fn test_capability_detection() {
    let capabilities = detect_capabilities().expect("Failed to detect capabilities");

    // At minimum, WASM should always be available
    assert!(
        capabilities.wasm_available,
        "WASM should always be available"
    );

    // Mode should be one of KVM, PVM, or WASM
    assert!(
        matches!(
            capabilities.virtualization_mode,
            VirtualizationMode::Kvm | VirtualizationMode::Pvm | VirtualizationMode::Wasm
        ),
        "Mode should be KVM, PVM, or WASM"
    );

    println!("Capability detection test PASSED");
}

#[tokio::test]
#[ignore]
async fn test_cold_start_gauntlet_tc_e2e_1() {
    print_mode();

    // Skip if no Firecracker backend available
    if !firecracker_available() {
        println!("SKIPPING: No Firecracker backend available");
        return;
    }

    if !zfs_available() {
        println!("SKIPPING: ZFS pool 'shellwego' not found");
        return;
    }

    let start_time = std::time::Instant::now();
    let app_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();

    let metrics = Arc::new(shellwego_agent::metrics::MetricsCollector::new(
        Uuid::new_v4(),
    ));
    let vmm_manager = VmmManager::new(&test_config(), metrics)
        .await
        .expect("VMM init failed");
    let zfs_manager = ZfsManager::new("shellwego").await.expect("ZFS init failed");

    let rootfs_path = zfs_manager
        .init_app_storage(app_id)
        .await
        .expect("ZFS init failed");

    let tap_name = format!("tap-{}", &app_id.to_string()[..8]);

    let config = MicrovmConfig::new(app_id, vm_id)
        .with_memory(128)
        .with_cpu_shares(1024)
        .with_kernel(PathBuf::from("/var/lib/shellwego/vmlinux"))
        .with_boot_args(&format!(
            "console=ttyS0 reboot=k panic=1 pci=off ip={}::{}:255.255.255.0::eth0:off",
            "10.0.4.2", "10.0.4.1"
        ))
        .with_drive(DriveConfig::rootfs(rootfs_path.rootfs.into()))
        .with_network_interface(NetworkInterface::new(
            "eth0",
            &tap_name,
            &shellwego_network::generate_mac(&app_id),
            "10.0.4.2",
            "10.0.4.1",
        ));

    vmm_manager.start(config).await.expect("Failed to start VM");

    let running = vmm_manager
        .list_running()
        .await
        .expect("Failed to list VMs");
    assert!(
        running.iter().any(|vm| vm.app_id == app_id),
        "VM should be running"
    );

    let state = vmm_manager
        .get_state(app_id)
        .await
        .expect("Failed to get VM state");
    assert!(state.is_some(), "VM state should exist");

    let elapsed = start_time.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "Cold start exceeded 10s limit: {:?}",
        elapsed
    );

    vmm_manager.stop(app_id).await.expect("Failed to stop VM");
    zfs_manager
        .cleanup_app(app_id)
        .await
        .expect("ZFS cleanup failed");

    println!("E2E cold start PASSED in {:?}", elapsed);
}

#[tokio::test]
#[ignore]
async fn test_secret_injection_security_tc_e2e_2() {
    print_mode();

    if !firecracker_available() {
        println!("SKIPPING: No Firecracker backend available");
        return;
    }

    if !zfs_available() {
        println!("SKIPPING: ZFS pool 'shellwego' not found");
        return;
    }

    let app_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    let secrets_content =
        r#"{"SOVEREIGN_KEY":"topsecret","DATABASE_URL":"postgres://user:pass@host:5432/db"}"#;

    let secrets_dir = format!("/run/shellwego/secrets/{}", app_id);
    tokio::fs::create_dir_all(&secrets_dir)
        .await
        .expect("Failed to create secrets dir");
    let secrets_path = std::path::Path::new(&secrets_dir).join("env.json");
    tokio::fs::write(&secrets_path, secrets_content)
        .await
        .expect("Failed to write secrets");

    let metrics = Arc::new(shellwego_agent::metrics::MetricsCollector::new(
        Uuid::new_v4(),
    ));
    let vmm_manager = VmmManager::new(&test_config(), metrics)
        .await
        .expect("VMM init failed");
    let zfs_manager = ZfsManager::new("shellwego").await.expect("ZFS init failed");
    let rootfs_path = zfs_manager
        .init_app_storage(app_id)
        .await
        .expect("ZFS init failed");

    let config = MicrovmConfig::new(app_id, vm_id)
        .with_memory(128)
        .with_cpu_shares(1024)
        .with_kernel(PathBuf::from("/var/lib/shellwego/vmlinux"))
        .with_boot_args("console=ttyS0 reboot=k panic=1 pci=off")
        .with_drive(DriveConfig::rootfs(rootfs_path.rootfs.into()))
        .with_drive(DriveConfig::data("secrets", secrets_path.clone(), true));

    vmm_manager
        .start(config)
        .await
        .expect("Failed to start VM with secrets");

    let running = vmm_manager
        .list_running()
        .await
        .expect("Failed to list VMs");
    assert!(running.iter().any(|vm| vm.app_id == app_id));

    vmm_manager.stop(app_id).await.expect("Failed to stop VM");
    zfs_manager
        .cleanup_app(app_id)
        .await
        .expect("ZFS cleanup failed");
    tokio::fs::remove_dir_all(&secrets_dir).await.ok();

    println!("E2E secret injection PASSED");
}
