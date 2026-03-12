//! Full VM configuration type

use serde::{Serialize, Deserialize};
use super::{
    Balloon, Drive, BootSource, CpuConfig, Logger, MachineConfiguration,
    Metrics, MemoryHotplugConfig, MmdsConfig, NetworkInterface, Pmem,
    Vsock, EntropyDevice,
};

/// Full VM configuration for export.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
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
