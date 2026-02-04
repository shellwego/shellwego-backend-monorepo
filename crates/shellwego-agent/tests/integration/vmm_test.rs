use std::path::PathBuf;
use std::os::unix::net::UnixStream;
use std::io::{Read, Write};
use shellwego_agent::vmm::{FirecrackerDriver, MicrovmConfig, DriveConfig, NetworkInterface};
use uuid::Uuid;

fn kvm_required() {
    if !PathBuf::from("/dev/kvm").exists() {
        panic!("SKIPPING: No /dev/kvm found. This test requires hardware acceleration.");
    }
}

#[tokio::test]
async fn test_firecracker_driver_new_binary_missing() {
    let missing_path = PathBuf::from("/nonexistent/firecracker");
    let result = FirecrackerDriver::new(&missing_path).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not found"), "Expected 'not found' error, got: {}", err);
}

#[tokio::test]
async fn test_firecracker_driver_new_binary_exists() {
    kvm_required();

    let paths = vec![
        PathBuf::from("/usr/local/bin/firecracker"),
        PathBuf::from("/usr/bin/firecracker"),
        PathBuf::from("/opt/firecracker/firecracker"),
    ];

    let mut found = false;
    for path in &paths {
        if path.exists() {
            let result = FirecrackerDriver::new(path).await;
            assert!(result.is_ok(), "Binary at {:?} should be valid", path);
            found = true;
            break;
        }
    }

    if !found {
        panic!("No Firecracker binary found in standard locations");
    }
}

#[tokio::test]
async fn test_firecracker_api_handshake_tc_i1() {
    kvm_required();

    let bin_path = PathBuf::from("/usr/local/bin/firecracker");
    if !bin_path.exists() {
        panic!("Firecracker binary not found at {:?}", bin_path);
    }

    let driver = FirecrackerDriver::new(&bin_path).await.expect("Binary missing");
    assert!(!driver.binary_path().as_os_str().is_empty());
}

#[tokio::test]
async fn test_firecracker_version_via_uds() {
    kvm_required();

    let bin_path = PathBuf::from("/usr/local/bin/firecracker");
    if !bin_path.exists() {
        panic!("Firecracker binary not found");
    }

    let socket_path = tempfile::NamedTempFile::new().unwrap().path().with_extension("sock");
    let log_path = socket_path.with_extension("log");

    let mut child = std::process::Command::new(&bin_path)
        .arg("--api-sock")
        .arg(&socket_path)
        .arg("--id")
        .arg("test-version")
        .arg("--log-path")
        .arg(&log_path)
        .arg("--level")
        .arg("Debug")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn Firecracker");

    let start = std::time::Instant::now();
    let mut connected = false;
    while start.elapsed().as_secs() < 5 {
        if socket_path.exists() {
            if let Ok(mut stream) = UnixStream::connect(&socket_path) {
                let request = "GET /version HTTP/1.1\r\nHost: localhost\r\n\r\n";
                stream.write_all(request.as_bytes()).expect("Failed to write");

                let mut response = String::new();
                stream.read_to_string(&mut response).expect("Failed to read");

                assert!(
                    response.contains("200") || response.contains("HTTP"),
                    "Expected HTTP response, got: {}",
                    response
                );
                assert!(
                    response.contains("firecracker") || response.contains("Firecracker"),
                    "Response should mention Firecracker"
                );
                connected = true;
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let _ = child.kill();

    if !connected {
        panic!("Failed to connect to Firecracker socket within 5s");
    }

    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(&log_path);
}

#[tokio::test]
async fn test_microvm_config_validation() {
    let config = MicrovmConfig {
        app_id: Uuid::new_v4(),
        vm_id: Uuid::new_v4(),
        memory_mb: 512,
        cpu_shares: 1024,
        kernel_path: PathBuf::from("/var/lib/shellwego/vmlinux"),
        kernel_boot_args: "console=ttyS0".to_string(),
        drives: vec![],
        network_interfaces: vec![],
        vsock_path: format!("/var/run/shellwego/{}.sock", Uuid::new_v4()),
    };

    assert_eq!(config.memory_mb, 512);
    assert_eq!(config.cpu_shares, 1024);
    assert!(config.vm_id != config.app_id);
    assert_eq!((config.cpu_shares / 1024) as u32, 1);
}

#[tokio::test]
async fn test_drive_config_validation() {
    let root_drive = DriveConfig {
        drive_id: "rootfs".to_string(),
        path_on_host: PathBuf::from("/var/lib/shellwego/rootfs/base.ext4"),
        is_root_device: true,
        is_read_only: true,
    };

    assert_eq!(root_drive.drive_id, "rootfs");
    assert!(root_drive.is_root_device);
    assert!(root_drive.is_read_only);

    let data_drive = DriveConfig {
        drive_id: "vol-data".to_string(),
        path_on_host: PathBuf::from("/dev/zvol/shellwego/volumes/data"),
        is_root_device: false,
        is_read_only: false,
    };

    assert!(!data_drive.is_root_device);
    assert!(!data_drive.is_read_only);
}

#[tokio::test]
async fn test_network_interface_validation() {
    let iface = NetworkInterface {
        iface_id: "eth0".to_string(),
        host_dev_name: "tap-abc12345".to_string(),
        guest_mac: "aa:bb:cc:dd:ee:ff".to_string(),
        guest_ip: "10.0.0.2".to_string(),
        host_ip: "10.0.0.1".to_string(),
    };

    assert_eq!(iface.iface_id, "eth0");
    assert!(iface.host_dev_name.starts_with("tap-"));
    assert!(iface.guest_mac.parse::<std::net::Ipv4Addr>().is_err());
    assert!(iface.guest_ip.parse::<std::net::Ipv4Addr>().is_ok());
}
