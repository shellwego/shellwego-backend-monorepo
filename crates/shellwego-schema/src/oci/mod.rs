//! OCI (Open Container Initiative) types for image distribution and storage
//!
//! This module provides the standard OCI types as defined in:
//! - OCI Image Spec: https://github.com/opencontainers/image-spec
//! - OCI Distribution Spec: https://github.com/opencontainers/distribution-spec
//!
//! ## Module Organization
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `manifest` | OCI image manifest and index types |
//! | `descriptor` | Content descriptors for manifests, configs, and layers |
//! | `platform` | Platform specification for multi-arch images |
//! | `config` | Image configuration (env, cmd, entrypoint, etc.) |
//! | `auth` | Registry authentication types |

pub mod manifest;
pub mod descriptor;
pub mod platform;
pub mod config;
pub mod auth;

// Re-export commonly used types at module level
pub use manifest::{Manifest, ManifestIndex};
pub use descriptor::{Descriptor, ConfigDescriptor, LayerDescriptor, ManifestDescriptor};
pub use platform::Platform;
pub use config::{ImageConfig, ContainerConfig, RootFs, HistoryEntry};
pub use auth::{RegistryAuth, AuthToken, OciConfig};
