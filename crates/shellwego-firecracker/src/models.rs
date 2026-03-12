//! Firecracker API Models
//!
//! Generated from Firecracker API specification v1.16.0-dev
//! Latest stable release: v1.14.1

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

// ============================================================================
// Instance Info & State
// ============================================================================

/// Describes MicroVM instance information.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstanceInfo {
    /// Application name.
    pub app_name: String,
    /// MicroVM / instance ID.
    pub id: String,
    /// The current detailed state of the Firecracker instance.
    pub state: InstanceState,
    /// MicroVM hypervisor build version.
    pub vmm_version: String,
}

/// Instance state enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum InstanceState {
    NotStarted,
    Running,
    Paused,
}

impl Default for InstanceState {
    fn default() -> Self {
        Self::NotStarted
    }
}

/// Firecracker version information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirecrackerVersion {
    /// Firecracker build version.
    pub firecracker_version: String,
}

// ============================================================================
// Boot Source
// ============================================================================

/// Boot source descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootSource {
    /// Host level path to the kernel image used to boot the guest.
    pub kernel_image_path: String,
    /// Kernel boot arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_args: Option<String>,
    /// Host level path to the initrd image used to boot the guest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initrd_path: Option<String>,
}

// ============================================================================
// Machine Configuration
// ============================================================================

/// Machine configuration descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineConfiguration {
    /// Number of vCPUs (either 1 or an even number in range [1, 32]).
    pub vcpu_count: i64,
    /// Memory size of VM in MiB.
    pub mem_size_mib: i64,
    /// Flag for enabling/disabling simultaneous multithreading. Can be enabled only on x86.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smt: Option<bool>,
    /// Enable dirty page tracking for diff snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_dirty_pages: Option<bool>,
    /// CPU template for feature masking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_template: Option<CpuTemplate>,
    /// Huge pages configuration for backing guest memory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub huge_pages: Option<HugePages>,
}

impl Default for MachineConfiguration {
    fn default() -> Self {
        Self {
            vcpu_count: 1,
            mem_size_mib: 128,
            smt: Some(false),
            track_dirty_pages: Some(false),
            cpu_template: None,
            huge_pages: None,
        }
    }
}

/// CPU template enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuTemplate {
    C3,
    T2,
    T2S,
    T2CL,
    T2A,
    V1N1,
    #[serde(rename = "None")]
    NoneTemplate,
}

impl Default for CpuTemplate {
    fn default() -> Self {
        Self::NoneTemplate
    }
}

/// Huge pages configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HugePages {
    None,
    #[serde(rename = "2M")]
    TwoMeg,
}

impl Default for HugePages {
    fn default() -> Self {
        Self::None
    }
}

// ============================================================================
// CPU Configuration
// ============================================================================

/// CPU configuration with modifiers for flags.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CpuConfig {
    /// KVM capabilities to add or remove.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kvm_capabilities: Option<Vec<String>>,
    /// CPUID leaf modifiers (x86_64 only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpuid_modifiers: Option<Vec<CpuidLeafModifier>>,
    /// MSR modifiers (x86_64 only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msr_modifiers: Option<Vec<MsrModifier>>,
    /// Register modifiers (aarch64 only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reg_modifiers: Option<Vec<ArmRegisterModifier>>,
    /// vCPU feature modifiers (aarch64 only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vcpu_features: Option<Vec<VcpuFeatures>>,
}

/// CPUID leaf modifier (x86_64).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuidLeafModifier {
    /// CPUID leaf index.
    pub leaf: String,
    /// CPUID subleaf index.
    pub subleaf: String,
    /// KVM feature flags.
    pub flags: i32,
    /// Register modifiers.
    pub modifiers: Vec<CpuidRegisterModifier>,
}

/// CPUID register modifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuidRegisterModifier {
    /// Target register (eax, ebx, ecx, edx).
    pub register: CpuidRegister,
    /// 32-bit bitmap string.
    pub bitmap: String,
}

/// CPUID register enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuidRegister {
    Eax,
    Ebx,
    Ecx,
    Edx,
}

/// MSR modifier (x86_64).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsrModifier {
    /// MSR address.
    pub addr: String,
    /// 64-bit bitmap string.
    pub bitmap: String,
}

/// ARM register modifier (aarch64).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmRegisterModifier {
    /// Register address.
    pub addr: String,
    /// 128-bit bitmap string.
    pub bitmap: String,
}

/// vCPU features modifier (aarch64).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcpuFeatures {
    /// Index in kvm_vcpu_init.features array.
    pub index: i32,
    /// 32-bit bitmap string.
    pub bitmap: String,
}

// ============================================================================
// Drive (Block Device)
// ============================================================================

/// Drive descriptor for virtio-block or vhost-user-block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drive {
    /// Unique drive identifier.
    pub drive_id: String,
    /// Whether this is the root device.
    pub is_root_device: bool,
    /// Partition UUID for boot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partuuid: Option<String>,
    /// Caching strategy for the block device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_type: Option<CacheType>,

    // Virtio-block specific parameters
    /// Is block read only (virtio-block only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_read_only: Option<bool>,
    /// Host level path for the guest drive (virtio-block only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_on_host: Option<String>,
    /// Rate limiter configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiter: Option<RateLimiter>,
    /// IO engine type (virtio-block only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_engine: Option<IoEngine>,

    // Vhost-user-block specific parameters
    /// Path to vhost-user-block backend socket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket: Option<String>,
}

/// Cache type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheType {
    Unsafe,
    Writeback,
}

impl Default for CacheType {
    fn default() -> Self {
        Self::Unsafe
    }
}

/// IO engine type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IoEngine {
    Sync,
    Async,
}

impl Default for IoEngine {
    fn default() -> Self {
        Self::Sync
    }
}

/// Partial drive for PATCH operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialDrive {
    /// Drive identifier.
    pub drive_id: String,
    /// Host level path for the guest drive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_on_host: Option<String>,
    /// Rate limiter configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiter: Option<RateLimiter>,
}

// ============================================================================
// Persistent Memory (PMEM)
// ============================================================================

/// Persistent memory device descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pmem {
    /// Device identifier.
    pub id: String,
    /// Host level path for the backing file.
    pub path_on_host: String,
    /// Make this device the root device for boot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_device: Option<bool>,
    /// Map backing file in read-only mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
}

// ============================================================================
// Network Interface
// ============================================================================

/// Network interface descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    /// Network interface identifier.
    pub iface_id: String,
    /// Host level path for the guest network interface (TAP device).
    pub host_dev_name: String,
    /// Guest MAC address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_mac: Option<String>,
    /// RX rate limiter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_rate_limiter: Option<RateLimiter>,
    /// TX rate limiter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_rate_limiter: Option<RateLimiter>,
}

/// Partial network interface for PATCH operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialNetworkInterface {
    /// Network interface identifier.
    pub iface_id: String,
    /// RX rate limiter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rx_rate_limiter: Option<RateLimiter>,
    /// TX rate limiter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_rate_limiter: Option<RateLimiter>,
}

// ============================================================================
// Rate Limiter
// ============================================================================

/// IO rate limiter with independent bytes/s and ops/s limits.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimiter {
    /// Token bucket with bytes as tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<TokenBucket>,
    /// Token bucket with operations as tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ops: Option<TokenBucket>,
}

/// Token bucket for rate limiting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBucket {
    /// The total number of tokens this bucket can hold.
    pub size: i64,
    /// The initial size of a token bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_time_burst: Option<i64>,
    /// The amount of milliseconds it takes for the bucket to refill.
    pub refill_time: i64,
}

// ============================================================================
// Balloon Device
// ============================================================================

/// Balloon device descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balloon {
    /// Target balloon size in MiB.
    pub amount_mib: i64,
    /// Whether the balloon should deflate on OOM.
    pub deflate_on_oom: bool,
    /// Interval in seconds between refreshing statistics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats_polling_interval_s: Option<i64>,
    /// Enable free page hinting feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_page_hinting: Option<bool>,
    /// Enable free page reporting feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_page_reporting: Option<bool>,
}

/// Balloon update for PATCH operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalloonUpdate {
    /// Target balloon size in MiB.
    pub amount_mib: i64,
}

/// Balloon device statistics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BalloonStats {
    /// Target number of pages.
    pub target_pages: i64,
    /// Actual number of pages.
    pub actual_pages: i64,
    /// Target memory in MiB.
    pub target_mib: i64,
    /// Actual memory in MiB.
    pub actual_mib: i64,
    /// Memory swapped in (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_in: Option<i64>,
    /// Memory swapped out (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_out: Option<i64>,
    /// Major page faults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub major_faults: Option<i64>,
    /// Minor page faults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor_faults: Option<i64>,
    /// Free memory (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_memory: Option<i64>,
    /// Total memory (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_memory: Option<i64>,
    /// Available memory (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_memory: Option<i64>,
    /// Disk caches (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_caches: Option<i64>,
    /// Successful hugetlb allocations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hugetlb_allocations: Option<i64>,
    /// Failed hugetlb allocations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hugetlb_failures: Option<i64>,
    /// OOM killer invocations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oom_kill: Option<i64>,
}

/// Balloon statistics update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalloonStatsUpdate {
    /// Statistics polling interval in seconds.
    pub stats_polling_interval_s: i64,
}

/// Command to start free page hinting.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BalloonStartCmd {
    /// Auto-acknowledge when guest submits done cmd.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledge_on_stop: Option<bool>,
}

/// Free page hinting status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalloonHintingStatus {
    /// Last command from host.
    pub host_cmd: i64,
    /// Last command from guest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_cmd: Option<i64>,
}

// ============================================================================
// Vsock Device
// ============================================================================

/// Vsock device descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vsock {
    /// Guest Vsock CID (must be >= 3).
    pub guest_cid: i64,
    /// Path to UNIX domain socket for proxying connections.
    pub uds_path: String,
    /// Deprecated vsock ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vsock_id: Option<String>,
}

// ============================================================================
// Entropy Device
// ============================================================================

/// Entropy device descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntropyDevice {
    /// Rate limiter configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiter: Option<RateLimiter>,
}

// ============================================================================
// Serial Device
// ============================================================================

/// Serial console configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SerialDevice {
    /// Path to file or named pipe for serial output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_out_path: Option<String>,
}

// ============================================================================
// Logger & Metrics
// ============================================================================

/// Logger configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Logger {
    /// Path to the named pipe or file for log output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    /// Log level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<LogLevel>,
    /// Whether to output the level in logs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_level: Option<bool>,
    /// Whether to include file path and line number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_log_origin: Option<bool>,
    /// Module path to filter log messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
}

/// Log level enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LogLevel {
    Error,
    Warning,
    Info,
    Debug,
    Trace,
    Off,
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Info
    }
}

/// Metrics configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    /// Path to the named pipe or file for metrics output.
    pub metrics_path: String,
}

/// Firecracker metrics data (emitted to FIFO).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FirecrackerMetrics {
    /// UTC timestamp in milliseconds.
    pub utc_time_ms: u64,
    /// API server metrics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_server: Option<serde_json::Value>,
    /// VMM metrics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vmm: Option<VmmMetrics>,
    /// Network metrics per interface.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<HashMap<String, NetMetrics>>,
    /// Block metrics per drive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<HashMap<String, BlockMetrics>>,
}

/// VMM metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmmMetrics {
    /// RX bytes.
    #[serde(default)]
    pub rx_bytes: u64,
    /// TX bytes.
    #[serde(default)]
    pub tx_bytes: u64,
}

/// Network interface metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetMetrics {
    /// RX bytes count.
    pub rx_bytes_count: u64,
    /// TX bytes count.
    pub tx_bytes_count: u64,
    /// RX packets count.
    pub rx_packets_count: u64,
    /// TX packets count.
    pub tx_packets_count: u64,
    /// RX drops count.
    pub rx_drops_count: u64,
    /// TX drops count.
    pub tx_drops_count: u64,
}

/// Block device metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlockMetrics {
    /// Bytes read.
    pub read_bytes: u64,
    /// Bytes written.
    pub write_bytes: u64,
    /// Read operation count.
    pub read_count: u64,
    /// Write operation count.
    pub write_count: u64,
    /// Flush operation count.
    pub flush_count: u64,
}

// ============================================================================
// Actions
// ============================================================================

/// Instance action information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceActionInfo {
    /// Action type.
    pub action_type: ActionType,
}

/// Action type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ActionType {
    FlushMetrics,
    InstanceStart,
    SendCtrlAltDel,
}

// ============================================================================
// VM State
// ============================================================================

/// VM state descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vm {
    /// VM state.
    pub state: VmState,
}

/// VM state enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum VmState {
    Paused,
    Resumed,
}

// ============================================================================
// Snapshot
// ============================================================================

/// Snapshot creation parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotCreateParams {
    /// Path to the file for guest memory.
    pub mem_file_path: String,
    /// Path to the file for microVM state.
    pub snapshot_path: String,
    /// Type of snapshot (Full or Diff).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_type: Option<SnapshotType>,
}

/// Snapshot type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotType {
    Full,
    Diff,
}

impl Default for SnapshotType {
    fn default() -> Self {
        Self::Full
    }
}

/// Snapshot load parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotLoadParams {
    /// Path to the file containing microVM state.
    pub snapshot_path: String,
    /// Path to the file containing guest memory (deprecated, use mem_backend).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_file_path: Option<String>,
    /// Memory backend configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_backend: Option<MemoryBackend>,
    /// Enable dirty page tracking for diff snapshots (deprecated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_diff_snapshots: Option<bool>,
    /// Enable dirty page tracking for diff snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_dirty_pages: Option<bool>,
    /// Resume VM after loading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_vm: Option<bool>,
    /// Network device overrides for snapshot restore.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_overrides: Option<Vec<NetworkOverride>>,
}

/// Memory backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBackend {
    /// Backend type (File or Uffd).
    pub backend_type: MemoryBackendType,
    /// Path to file or UDS.
    pub backend_path: String,
}

/// Memory backend type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryBackendType {
    File,
    Uffd,
}

/// Network override for snapshot restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkOverride {
    /// Interface ID to modify.
    pub iface_id: String,
    /// New host device name.
    pub host_dev_name: String,
}

// ============================================================================
// Memory Hotplug
// ============================================================================

/// Memory hotplug configuration (virtio-mem).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHotplugConfig {
    /// Total size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size_mib: Option<i64>,
    /// Slot size in MiB (min: 128).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_size_mib: Option<i64>,
    /// Block size in MiB (min: 2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_size_mib: Option<i64>,
}

/// Memory hotplug size update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHotplugSizeUpdate {
    /// New target region size in MiB.
    pub requested_size_mib: i64,
}

/// Memory hotplug status.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryHotplugStatus {
    /// Total size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size_mib: Option<i64>,
    /// Slot size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_size_mib: Option<i64>,
    /// Block size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_size_mib: Option<i64>,
    /// Plugged size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugged_size_mib: Option<i64>,
    /// Requested size in MiB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_size_mib: Option<i64>,
}

// ============================================================================
// MMDS (Microvm Metadata Service)
// ============================================================================

/// MMDS configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmdsConfig {
    /// List of network interface IDs for MMDS.
    pub network_interfaces: Vec<String>,
    /// MMDS version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<MmdsVersion>,
    /// IPv4 address for MMDS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4_address: Option<String>,
    /// IMDS compatibility mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imds_compat: Option<bool>,
}

/// MMDS version enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MmdsVersion {
    V1,
    V2,
}

impl Default for MmdsVersion {
    fn default() -> Self {
        Self::V1
    }
}

/// MMDS contents (JSON object).
pub type MmdsContentsObject = serde_json::Value;

// ============================================================================
// Full VM Configuration
// ============================================================================

/// Full VM configuration for export.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FullVmConfiguration {
    /// Balloon device configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balloon: Option<Balloon>,
    /// Block device configurations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drives: Option<Vec<Drive>>,
    /// Boot source configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_source: Option<BootSource>,
    /// CPU configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_config: Option<CpuConfig>,
    /// Logger configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<Logger>,
    /// Machine configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_config: Option<MachineConfiguration>,
    /// Metrics configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Metrics>,
    /// Memory hotplug configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_hotplug: Option<MemoryHotplugConfig>,
    /// MMDS configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mmds_config: Option<MmdsConfig>,
    /// Network interface configurations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_interfaces: Option<Vec<NetworkInterface>>,
    /// PMEM device configurations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pmem: Option<Vec<Pmem>>,
    /// Vsock configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vsock: Option<Vsock>,
    /// Entropy device configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entropy: Option<EntropyDevice>,
}

// ============================================================================
// Error
// ============================================================================

/// Firecracker API error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    /// Error description.
    pub fault_message: String,
}
