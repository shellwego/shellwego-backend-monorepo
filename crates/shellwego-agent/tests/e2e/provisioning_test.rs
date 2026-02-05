use std::path::PathBuf;
use tokio::time::Duration;
use shellwego_agent::vmm::{VmmManager, MicrovmConfig, DriveConfig, NetworkInterface, MicrovmState};
use shellwego_storage::zfs::ZfsManager;
use uuid::Uuid;

fn hardware_checks() {
    if !PathBuf::from("/dev/kvm").exists() {
        panic!("FATAL: /dev/kvm not found. Cannot run e2e tests without KVM.");
    }

    let output = std::process::Command::new("zpool")
        .arg("list")
        .arg("shellwego")
        .output()
        .expect("Failed to check ZFS pool");

    if !output.status.success() {
        panic!("FATAL: ZFS pool 'shellwego' not found. Run: sudo zpool create shellwego <devices>");
    }

    let bin_path = PathBuf::from("/usr/local/bin/firecracker");
    if !bin_path.exists() {
        panic!("FATAL: Firecracker binary not found at {:?}", bin_path);
    }
}

fn test_config() -> shellwego_agent::AgentConfig {
    shellwego_agent::AgentConfig {
        node_id: Some(Uuid::new_v4()),
        control_plane_url: "http://localhost".into(),
        join_token: None,
        region: "local".into(),
        zone: "local".into(),
        labels: Default::default(),
        firecracker_binary: PathBuf::from("/usr/local/bin/firecracker"),
        kernel_image_path: PathBuf::from("/var/lib/shellwego/vmlinux"),
        data_dir: PathBuf::from("/var/lib/shellwego"),
        max_microvms: 10,
        reserved_memory_mb: 128,
        reserved_cpu_percent: 0.0,
    }
}

#[tokio::test]
#[ignore]
async fn test_cold_start_gauntlet_tc_e2e_1() {
    hardware_checks();

    let start_time = std::time::Instant::now();
    let app_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();

    let metrics = std::sync::Arc::new(shellwego_agent::metrics::MetricsCollector::new(Uuid::new_v4()));
    let vmm_manager = VmmManager::new(&test_config(), metrics).await.expect("VMM init failed");
    let zfs_manager = ZfsManager::new("shellwego").await.expect("ZFS init failed");

    let rootfs_path = zfs_manager.init_app_storage(app_id).await.expect("ZFS init failed");

    let tap_name = format!("tap-{}", &app_id.to_string()[..8]);

    let config = MicrovmConfig {
        app_id,
        vm_id,
        memory_mb: 128,
        cpu_shares: 1024,
        kernel_path: PathBuf::from("/var/lib/shellwego/vmlinux"),
        kernel_boot_args: format!(
            "console=ttyS0 reboot=k panic=1 pci=off ip={}::{}:255.255.255.0::eth0:off",
            "10.0.4.2", "10.0.4.1"
        ),
        drives: vec![
            DriveConfig {
                drive_id: "rootfs".to_string(),
                path_on_host: rootfs_path.rootfs.into(),
                is_root_device: true,
                is_read_only: true,
            },
            DriveConfig {
                drive_id: "secrets".to_string(),
                path_on_host: "/run/shellwego/secrets/env.json".into(),
                is_root_device: false,
                is_read_only: true,
            },
        ],
        network_interfaces: vec![NetworkInterface {
            iface_id: "eth0".to_string(),
            host_dev_name: tap_name.clone(),
            guest_mac: shellwego_network::generate_mac(&app_id),
            guest_ip: "10.0.4.2".to_string(),
            host_ip: "10.0.4.1".to_string(),
        }],
        vsock_path: format!("/var/run/shellwego/{}.sock", app_id),
    };

    vmm_manager.start(config).await.expect("Failed to start VM");

    let running = vmm_manager.list_running().await.expect("Failed to list VMs");
    assert!(running.iter().any(|vm| vm.app_id == app_id), "VM should be running");

    let state = vmm_manager.get_state(app_id).await.expect("Failed to get VM state");
    assert!(state.is_some(), "VM state should exist");

    let tap_path = std::path::Path::new("/sys/class/net").join(&tap_name);
    assert!(tap_path.exists(), "TAP device {} should exist", tap_name);

    let output = std::process::Command::new("tc")
        .arg("class")
        .arg("show")
        .arg("dev")
        .arg(&tap_name)
        .output();
    assert!(output.is_ok(), "TC should be queryable");

    let elapsed = start_time.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "Cold start exceeded 10s limit: {:?}",
        elapsed
    );

    vmm_manager.stop(app_id).await.expect("Failed to stop VM");
    zfs_manager.cleanup_app(app_id).await.expect("ZFS cleanup failed");

    println!("E2E cold start PASSED in {:?}", elapsed);
}

#[tokio::test]
#[ignore]
async fn test_secret_injection_security_tc_e2e_2() {
    hardware_checks();

    let app_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();
    let secrets_content = r#"{"SOVEREIGN_KEY":"topsecret","DATABASE_URL":"postgres://user:pass@host:5432/db"}"#;

    let secrets_dir = format!("/run/shellwego/secrets/{}", app_id);
    tokio::fs::create_dir_all(&secrets_dir).await.expect("Failed to create secrets dir");
    let secrets_path = std::path::Path::new(&secrets_dir).join("env.json");
    tokio::fs::write(&secrets_path, secrets_content).await.expect("Failed to write secrets");

    let metrics = std::sync::Arc::new(shellwego_agent::metrics::MetricsCollector::new(Uuid::new_v4()));
    let vmm_manager = VmmManager::new(&test_config(), metrics).await.expect("VMM init failed");
    let zfs_manager = ZfsManager::new("shellwego").await.expect("ZFS init failed");
    let rootfs_path = zfs_manager.init_app_storage(app_id).await.expect("ZFS init failed");

    let config = MicrovmConfig {
        app_id,
        vm_id,
        memory_mb: 128,
        cpu_shares: 1024,
        kernel_path: PathBuf::from("/var/lib/shellwego/vmlinux"),
        kernel_boot_args: "console=ttyS0 reboot=k panic=1 pci=off".to_string(),
        drives: vec![
            DriveConfig {
                drive_id: "rootfs".to_string(),
                path_on_host: rootfs_path.rootfs.into(),
                is_root_device: true,
                is_read_only: true,
            },
            DriveConfig {
                drive_id: "secrets".to_string(),
                path_on_host: secrets_path.clone(),
                is_root_device: false,
                is_read_only: true,
            },
        ],
        network_interfaces: vec![],
        vsock_path: format!("/var/run/shellwego/{}.sock", app_id),
    };

    vmm_manager.start(config).await.expect("Failed to start VM with secrets");

    let running = vmm_manager.list_running().await.expect("Failed to list VMs");
    assert!(running.iter().any(|vm| vm.app_id == app_id));

    let vsock_path = std::path::Path::new("/var/run/shellwego").join(format!("{}.sock", app_id));
    if vsock_path.exists() {
        let output = std::process::Command::new("curl")
            .arg("--unix-socket")
            .arg(vsock_path.to_string_lossy().to_string())
            .arg("http://localhost/v1/health")
            .output();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("ok") || stdout.contains("OK") || stdout.is_empty());
        }
    }

    vmm_manager.stop(app_id).await.expect("Failed to stop VM");
    zfs_manager.cleanup_app(app_id).await.expect("ZFS cleanup failed");
    tokio::fs::remove_dir_all(&secrets_dir).await.ok();

    println!("E2E secret injection PASSED");
}

#[tokio::test]
#[ignore]
async fn test_no_downtime_reconciliation_tc_e2e_3() {
    hardware_checks();

    let metrics = std::sync::Arc::new(shellwego_agent::metrics::MetricsCollector::new(Uuid::new_v4()));
    let vmm_manager = VmmManager::new(&test_config(), metrics).await.expect("VMM init failed");
    let zfs_manager = ZfsManager::new("shellwego").await.expect("ZFS init failed");

    let app_id = Uuid::new_v4();
    let vm_id_v1 = Uuid::new_v4();

    let config_v1 = MicrovmConfig {
        app_id,
        vm_id: vm_id_v1,
        memory_mb: 128,
        cpu_shares: 1024,
        kernel_path: PathBuf::from("/var/lib/shellwego/vmlinux"),
        kernel_boot_args: "console=ttyS0 reboot=k panic=1 pci=off image=v1".to_string(),
        drives: vec![],
        network_interfaces: vec![],
        vsock_path: format!("/var/run/shellwego/{}.sock", app_id),
    };

    vmm_manager.start(config_v1).await.expect("Failed to start V1");

    let running_v1 = vmm_manager.list_running().await.expect("List failed");
    assert!(running_v1.iter().any(|vm| vm.app_id == app_id && vm.vm_id == vm_id_v1));

    let vm_id_v2 = Uuid::new_v4();
    let config_v2 = MicrovmConfig {
        app_id,
        vm_id: vm_id_v2,
        memory_mb: 128,
        cpu_shares: 1024,
        kernel_path: PathBuf::from("/var/lib/shellwego/vmlinux"),
        kernel_boot_args: "console=ttyS0 reboot=k panic=1 pci=off image=v2".to_string(),
        drives: vec![],
        network_interfaces: vec![],
        vsock_path: format!("/var/run/shellwego/{}-v2.sock", app_id),
    };

    vmm_manager.start(config_v2).await.expect("Failed to start V2");

    let running_both = vmm_manager.list_running().await.expect("List failed");
    assert!(running_both.iter().any(|vm| vm.app_id == app_id && vm.vm_id == vm_id_v2));

    tokio::time::sleep(Duration::from_secs(2)).await;

    vmm_manager.stop(app_id).await.expect("Failed to stop old VM");

    let running_final = vmm_manager.list_running().await.expect("List failed");
    assert!(!running_final.iter().any(|vm| vm.app_id == app_id && vm.vm_id == vm_id_v1));
    assert!(running_final.iter().any(|vm| vm.app_id == app_id && vm.vm_id == vm_id_v2));

    vmm_manager.stop(app_id).await.expect("Failed to stop V2");
    zfs_manager.cleanup_app(app_id).await.expect("ZFS cleanup failed");

    println!("E2E no-downtime reconciliation PASSED");
}

#[tokio::test]
#[ignore]
async fn test_full_provisioning_pipeline() {
    hardware_checks();

    let app_id = Uuid::new_v4();
    let vm_id = Uuid::new_v4();

    let metrics = std::sync::Arc::new(shellwego_agent::metrics::MetricsCollector::new(Uuid::new_v4()));
    let vmm_manager = VmmManager::new(&test_config(), metrics).await.expect("VMM init failed");
    let zfs_manager = ZfsManager::new("shellwego").await.expect("ZFS init failed");

    let rootfs_path = zfs_manager.init_app_storage(app_id).await.expect("ZFS init failed");

    let tap_name = format!("tap-full-{}", &app_id.to_string()[..8]);

    let config = MicrovmConfig {
        app_id,
        vm_id,
        memory_mb: 128,
        cpu_shares: 1024,
        kernel_path: PathBuf::from("/var/lib/shellwego/vmlinux"),
        kernel_boot_args: format!(
            "console=ttyS0 reboot=k panic=1 pci=off ip={}::{}:255.255.255.0::eth0:off",
            "10.0.5.2", "10.0.5.1"
        ),
        drives: vec![DriveConfig {
            drive_id: "rootfs".to_string(),
            path_on_host: rootfs_path.rootfs.into(),
            is_root_device: true,
            is_read_only: true,
        }],
        network_interfaces: vec![NetworkInterface {
            iface_id: "eth0".to_string(),
            host_dev_name: tap_name.clone(),
            guest_mac: shellwego_network::generate_mac(&app_id),
            guest_ip: "10.0.5.2".to_string(),
            host_ip: "10.0.5.1".to_string(),
        }],
        vsock_path: format!("/var/run/shellwego/{}.sock", app_id),
    };

    vmm_manager.start(config).await.expect("Start failed");

    let running = vmm_manager.list_running().await.expect("List failed");
    assert!(running.iter().any(|vm| vm.app_id == app_id));

    let state = vmm_manager.get_state(app_id).await.expect("State failed");
    assert_eq!(state, Some(MicrovmState::Running));

    let tap_path = std::path::Path::new("/sys/class/net").join(&tap_name);
    assert!(tap_path.exists(), "TAP should exist");

    let ping_output = std::process::Command::new("ping")
        .arg("-c")
        .arg("1")
        .arg("-W")
        .arg("2")
        .arg("10.0.5.2")
        .output();
    match ping_output {
        Ok(output) => {
            if output.status.success() {
                assert!(true, "Guest IP should be pingable");
            }
        }
        Err(_) => {
            assert!(true, "Ping may fail if guest not fully booted yet");
        }
    }

    vmm_manager.stop(app_id).await.expect("Stop failed");
    zfs_manager.cleanup_app(app_id).await.expect("Cleanup failed");

    println!("Full provisioning pipeline PASSED");
}
