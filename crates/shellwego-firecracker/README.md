# ShellWeGo Firecracker SDK

A comprehensive Rust SDK for the [Firecracker](https://github.com/firecracker-microvm/firecracker) microVM API.

[![Crates.io](https://img.shields.io/crates/v/shellwego-firecracker.svg)](https://crates.io/crates/shellwego-firecracker)
[![Documentation](https://docs.rs/shellwego-firecracker/badge.svg)](https://docs.rs/shellwego-firecracker)
[![License: AGPL-3.0-or-later](https://img.shields.io/badge/License-AGPL%203.0%2B-blue.svg)](https://opensource.org/licenses/AGPL-3.0)

## Features

- **Full API Coverage**: All Firecracker API endpoints supported
- **Async/Await**: Built on Tokio for async operations
- **Type-Safe**: Strongly typed models with serde serialization
- **Unix Domain Sockets**: Direct communication with Firecracker process
- **Backward Compatible**: Legacy API methods preserved for migration

## Supported Firecracker Versions

| Version | Status |
|---------|--------|
| v1.7.0  | Tested |
| v1.8.x  | Supported |
| v1.9.x  | Supported |
| v1.10.x | Supported |
| v1.11.x | Supported |
| v1.12.x | Supported |
| v1.13.x | Supported |
| v1.14.x | Latest Stable |

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
shellwego-firecracker = "0.5"
```

## Quick Start

```rust,no_run
use shellwego_firecracker::{FirecrackerClient, models::*};
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connect to Firecracker's API socket
    let client = FirecrackerClient::new(Path::new("/tmp/firecracker.sock"));

    // Configure the boot source
    client.put_boot_source(BootSource {
        kernel_image_path: "/path/to/vmlinux".to_string(),
        boot_args: Some("console=ttyS0 reboot=k panic=1 pci=off".to_string()),
        initrd_path: None,
    }).await?;

    // Configure machine resources
    client.put_machine_config(MachineConfiguration {
        vcpu_count: 2,
        mem_size_mib: 1024,
        smt: Some(false),
        ..Default::default()
    }).await?;

    // Add a root drive
    client.put_drive("rootfs", Drive {
        drive_id: "rootfs".to_string(),
        is_root_device: true,
        is_read_only: Some(false),
        path_on_host: Some("/path/to/rootfs.ext4".to_string()),
        cache_type: Some(CacheType::Unsafe),
        ..Default::default()
    }).await?;

    // Add a network interface
    client.put_network_interface("eth0", NetworkInterface {
        iface_id: "eth0".to_string(),
        host_dev_name: "tap0".to_string(),
        guest_mac: Some("AA:FC:00:00:00:01".to_string()),
        ..Default::default()
    }).await?;

    // Start the microVM
    client.start_instance().await?;

    // Get instance info
    let info = client.describe_instance().await?;
    println!("Instance state: {:?}", info.state);

    Ok(())
}
```

## API Reference

### Instance Management

```rust
// Get instance information
let info = client.describe_instance().await?;

// Get Firecracker version
let version = client.get_version().await?;

// Start the microVM
client.start_instance().await?;

// Pause the microVM
client.pause_vm().await?;

// Resume the microVM
client.resume_vm().await?;
```

### Machine Configuration

```rust
// Full configuration
client.put_machine_config(MachineConfiguration {
    vcpu_count: 4,
    mem_size_mib: 2048,
    smt: Some(true),
    track_dirty_pages: Some(false),
    cpu_template: Some(CpuTemplate::T2),
    huge_pages: Some(HugePages::TwoMeg),
}).await?;

// Partial update
client.patch_machine_config(MachineConfiguration {
    vcpu_count: 8,
    ..Default::default()
}).await?;
```

### Block Devices (Drives)

```rust
// Add a drive
client.put_drive("data", Drive {
    drive_id: "data".to_string(),
    is_root_device: false,
    is_read_only: Some(true),
    path_on_host: Some("/path/to/data.img".to_string()),
    cache_type: Some(CacheType::Writeback),
    io_engine: Some(IoEngine::Async),
    ..Default::default()
}).await?;

// Update drive post-boot
client.patch_drive("data", PartialDrive {
    drive_id: "data".to_string(),
    path_on_host: Some("/path/to/new-data.img".to_string()),
    rate_limiter: None,
}).await?;
```

### Network Interfaces

```rust
// Add network interface with rate limiting
client.put_network_interface("eth0", NetworkInterface {
    iface_id: "eth0".to_string(),
    host_dev_name: "tap0".to_string(),
    guest_mac: Some("AA:FC:00:00:00:01".to_string()),
    rx_rate_limiter: Some(RateLimiter {
        bandwidth: Some(TokenBucket {
            size: 10_000_000, // 10 MB/s
            refill_time: 100,
            one_time_burst: None,
        }),
        ops: None,
    }),
    tx_rate_limiter: None,
}).await?;
```

### Balloon Device (Memory Management)

```rust
// Add balloon device
client.put_balloon(Balloon {
    amount_mib: 512,
    deflate_on_oom: true,
    stats_polling_interval_s: Some(10),
    free_page_hinting: Some(true),
    free_page_reporting: Some(false),
}).await?;

// Adjust balloon size
client.patch_balloon(BalloonUpdate {
    amount_mib: 1024,
}).await?;

// Get statistics
let stats = client.describe_balloon_stats().await?;
println!("Available memory: {} bytes", stats.available_memory.unwrap_or(0));
```

### Vsock Device

```rust
// Add vsock device
client.put_vsock(Vsock {
    guest_cid: 3,
    uds_path: "/tmp/vsock.sock".to_string(),
    vsock_id: None,
}).await?;
```

### Snapshots

```rust
// Pause before snapshot
client.pause_vm().await?;

// Create snapshot
client.create_snapshot(SnapshotCreateParams {
    mem_file_path: "/snapshots/vm-memory".to_string(),
    snapshot_path: "/snapshots/vm-state".to_string(),
    snapshot_type: Some(SnapshotType::Full),
}).await?;

// Load snapshot
client.load_snapshot(SnapshotLoadParams {
    snapshot_path: "/snapshots/vm-state".to_string(),
    mem_file_path: Some("/snapshots/vm-memory".to_string()),
    resume_vm: Some(true),
    ..Default::default()
}).await?;
```

### Memory Hotplug

```rust
// Configure hotplug memory
client.put_memory_hotplug(MemoryHotplugConfig {
    total_size_mib: Some(4096),
    slot_size_mib: Some(128),
    block_size_mib: Some(2),
}).await?;

// Adjust size
client.patch_memory_hotplug(MemoryHotplugSizeUpdate {
    requested_size_mib: 2048,
}).await?;

// Check status
let status = client.get_memory_hotplug().await?;
println!("Plugged size: {} MiB", status.plugged_size_mib.unwrap_or(0));
```

### MMDS (Microvm Metadata Service)

```rust
// Configure MMDS
client.put_mmds_config(MmdsConfig {
    network_interfaces: vec!["eth0".to_string()],
    version: Some(MmdsVersion::V2),
    ipv4_address: Some("169.254.169.254".to_string()),
    imds_compat: Some(true),
}).await?;

// Set metadata
client.put_mmds(serde_json::json!({
    "instance-id": "i-1234567890",
    "local-hostname": "my-vm",
})).await?;
```

## Feature Flags

- `default`: Standard features
- `experimental`: Enable experimental features (vsock, balloon, memory hotplug)

## Error Handling

```rust
use shellwego_firecracker::Error;

match client.start_instance().await {
    Ok(()) => println!("Instance started"),
    Err(e) => {
        if let Some(fc_error) = e.downcast_ref::<Error>() {
            eprintln!("Firecracker error: {}", fc_error.fault_message);
        } else {
            eprintln!("Other error: {}", e);
        }
    }
}
```

## Contributing

Contributions are welcome! Please see the [contributing guidelines](../../CONTRIBUTING.md).

## License

AGPL-3.0-or-later

## Related Projects

- [Firecracker](https://github.com/firecracker-microvm/firecracker) - The underlying VMM
- [ShellWeGo](https://github.com/shellwego/shellwego) - Cloud platform using Firecracker
