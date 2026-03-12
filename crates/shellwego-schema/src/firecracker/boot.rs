//! Boot source configuration types

use serde::{Serialize, Deserialize};

/// Boot source descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
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
