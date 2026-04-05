# ShellWeGo Network
**The Data Plane.** Linux networking without the iptables bloat.

- **eBPF:** High-performance XDP firewall and TC egress rate limiting via `aya`.
- **IPAM:** Automatic IPv4 allocation for microVM subnets.
- **CNI:** Bridge + TAP management optimized for Firecracker.
- **Mesh:** WireGuard integration for encrypted node-to-node traffic.
- **QUIC:** Secure, multiplexed Control Plane ↔ Agent messaging via Quinn.

---

## Prerequisites

### Kernel Version

| Feature              | Minimum Kernel | Recommended |
|----------------------|----------------|-------------|
| XDP (SKB mode)       | 4.8            | 5.10+       |
| BPF_MAP_TYPE_PERCPU  | 4.6            | 5.10+       |
| TC BPF classifier    | 4.1            | 5.10+       |
| eBPF (full support)  | 5.4            | 6.1+        |

Run `uname -r` to check your current kernel.

### Required Kernel Modules

Load these modules before starting the network services:

```bash
# Bridge networking
sudo modprobe br_netfilter

# VXLAN overlay
sudo modprobe vxlan

# WireGuard (optional, for encrypted mesh)
sudo modprobe wireguard

# Persist across reboots (Debian/Ubuntu)
echo "br_netfilter" | sudo tee /etc/modules-load.d/shellwego.conf
echo "vxlan"       | sudo tee -a /etc/modules-load.d/shellwego.conf
echo "wireguard"   | sudo tee -a /etc/modules-load.d/shellwego.conf
```

### Required Capabilities

The ShellWeGo agent and network services require:

| Capability       | Purpose                                    |
|------------------|--------------------------------------------|
| `CAP_NET_ADMIN`  | Create bridges, TAP devices, attach eBPF   |
| `CAP_SYS_ADMIN`  | Load eBPF programs (older kernels)         |
| `CAP_NET_RAW`    | Raw socket access for packet inspection    |

When running in Docker:

```bash
docker run --cap-add=NET_ADMIN --cap-add=SYS_ADMIN ...
```

When running as a systemd service, add to the unit file:

```ini
[Service]
AmbientCapabilities=CAP_NET_ADMIN CAP_SYS_ADMIN
```

### Required sysctl Settings

```bash
# Enable IP forwarding (required for bridge-based CNI)
sudo sysctl -w net.ipv4.ip_forward=1
sudo sysctl -w net.ipv6.conf.all.forwarding=1

# Bridge netfilter (required for iptables rules to work on bridged traffic)
sudo sysctl -w net.bridge.bridge-nf-call-iptables=1
sudo sysctl -w net.bridge.bridge-nf-call-ip6tables=1

# Recommended DDoS hardening
sudo sysctl -w net.ipv4.tcp_syncookies=1
sudo sysctl -w net.ipv4.tcp_syn_retries=2
sudo sysctl -w net.ipv4.tcp_synack_retries=2
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=8192
sudo sysctl -w net.core.somaxconn=65535
sudo sysctl -w net.netfilter.nf_conntrack_max=131072
sudo sysctl -w net.netfilter.nf_conntrack_tcp_timeout_established=600
```

To persist these settings, add them to `/etc/sysctl.d/99-shellwego.conf` and run `sudo sysctl --system`.

---

## Building

### Rust Crate

```bash
# Default features (QUIC enabled, eBPF disabled)
cargo build -p shellwego-network

# With eBPF support (requires libbpf-dev / kernel headers)
cargo build -p shellwego-network --features ebpf

# All features
cargo build -p shellwego-network --all-features
```

### Compiling the eBPF Programs

The eBPF C source programs are located in `src/ebpf/programs/`:

| File                        | Type   | Description                         |
|-----------------------------|--------|-------------------------------------|
| `ingress_filter.bpf.c`      | XDP    | Packet filtering, IP blocklist, per-IP rate limiting |
| `tc_egress_limiter.bpf.c`   | TC     | Token-bucket egress bandwidth limiting |

**Prerequisites:**

- `clang` ≥ 14 with BPF target support
- `llvm` ≥ 14
- `libbpf-dev` (for headers like `bpf/bpf_helpers.h`)
- Linux kernel headers

**Ubuntu/Debian:**
```bash
sudo apt install clang llvm libbpf-dev linux-headers-$(uname -r)
```

**Compilation:**
```bash
make -C crates/shellwego-network ebpf
```

This produces `src/ebpf/bin/shellwego.bin` which is embedded into the Rust binary at compile time via `include_bytes!`.

If you cannot compile the eBPF programs (e.g., cross-compiling), the Rust code will automatically fall back to iptables/tc-based firewall and QoS.

---

## Architecture

### CNI Networking Flow

```
┌─────────────┐     ┌──────────────┐     ┌────────────────┐
│  CniNetwork  │────▶│    Bridge     │────▶│   TAP Device   │
│  (orchestr.) │     │   (br0)       │     │   (tap-<id>)   │
└──────┬───────┘     └──────────────┘     └───────┬────────┘
       │                                          │
       ▼                                          ▼
┌──────────────┐                          ┌──────────────┐
│     IPAM     │                          │   eBPF XDP   │
│  (IPv4 alloc)│                          │  (firewall)  │
└──────────────┘                          └──────────────┘
                                            ┌──────────────┐
                                            │  eBPF TC     │
                                            │  (QoS/rate)  │
                                            └──────────────┘
```

### Fallback Mode

When the eBPF binary is not available (0-byte placeholder or feature disabled), the manager operates in **fallback mode**:

- `attach_firewall()` → no-op (XdpFirewall module applies iptables instead)
- `apply_qos()` → no-op (EbpfQos module applies tc/htb instead)
- All in-memory state (blocklists, rate limits, statistics) still works

This means the system degrades gracefully and remains fully functional even without compiled eBPF programs.

### QUIC Message Bus

The Quinn-based QUIC layer provides:

- TLS 1.3 encrypted transport
- Bidirectional multiplexed streams
- ALPN-based protocol negotiation (`shellwego/1`)
- Native certificate roots via `webpki-roots` + `rustls-native-certs`
- Self-signed cert support for development

---

## Feature Flags

| Feature  | Dependencies                          | Description                          |
|----------|--------------------------------------|--------------------------------------|
| `quinn`  | quinn, rustls, webpki-roots, rcgen   | QUIC messaging (enabled by default)  |
| `ebpf`   | aya, aya-log                         | eBPF XDP/TC programs                 |

---

## Testing

```bash
# Unit tests (no privileges needed)
cargo test -p shellwego-network

# With eBPF feature
cargo test -p shellwego-network --features ebpf
```

Tests are designed to run without root privileges. eBPF integration tests require `CAP_BPF` / `CAP_NET_ADMIN`.
