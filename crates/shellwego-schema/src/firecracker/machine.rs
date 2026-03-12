//! Machine configuration and CPU types

use serde::{Serialize, Deserialize};

/// Machine configuration descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum CpuTemplate {
    C3,
    T2,
    T2S,
    T2CL,
    T2A,
    V1N1,
    #[serde(rename = "None")]
    #[default]
    NoneTemplate,
}

/// Huge pages configuration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum HugePages {
    #[default]
    None,
    #[serde(rename = "2M")]
    TwoMeg,
}

/// CPU configuration with modifiers for flags.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct CpuidRegisterModifier {
    /// Target register (eax, ebx, ecx, edx).
    pub register: CpuidRegister,
    /// 32-bit bitmap string.
    pub bitmap: String,
}

/// CPUID register enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum CpuidRegister {
    Eax,
    Ebx,
    Ecx,
    Edx,
}

/// MSR modifier (x86_64).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MsrModifier {
    /// MSR address.
    pub addr: String,
    /// 64-bit bitmap string.
    pub bitmap: String,
}

/// ARM register modifier (aarch64).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct ArmRegisterModifier {
    /// Register address.
    pub addr: String,
    /// 128-bit bitmap string.
    pub bitmap: String,
}

/// vCPU features modifier (aarch64).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct VcpuFeatures {
    /// Index in kvm_vcpu_init.features array.
    pub index: i32,
    /// 32-bit bitmap string.
    pub bitmap: String,
}
