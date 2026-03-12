//! ShellWeGo Schema
//! 
//! The crate is the single source of truth for all type definitions
//! in the ShellWeGo platform. It contains pure data structures
//! with no business logic.
//!
//! ## Design Principles
//!
//! 1. **Pure Data**: No business logic,//! 2. **Wire Format**: Types define API contracts between services
//! 3. **Feature Flags**: Optional derives for ORM, OpenAPI
//! 4. **Zero Runtime Dependencies**: No IO, no state, no side effects
//!
//! ## Module Organization
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `entities` | Domain entities (App, Node, etc.) |
//! | `vmm` | Virtual machine manager types |
//! | `network` | Network configuration types |
//! | `api` | API request/response types |
//! | `agent` | Agent configuration types |
//! | `firecracker` | Firecracker microVM API types |

pub mod entities;
pub mod error;
pub mod prelude;
pub mod vmm;
pub mod network;
pub mod api;
pub mod agent;
pub mod firecracker;

// Re-export commonly used types at crate root
pub use entities::*;
pub use error::{CoreError, CoreResult};
pub use vmm::{VirtualizationMode, MicrovmConfig, MicrovmState, MicrovmSummary, DriveConfig, NetworkInterface, RateLimiterConfig, MicrovmMetrics, WasmConfig};
pub use network::{NetworkConfig, NetworkSetup, NetworkError, Message, QuicConfig, AgentConnection, ResourceLimits, ChannelPriority, DiscoveryError, ServiceInstance, generate_mac, parse_mac};
pub use api::{ListAppsQuery, ListNodesQuery, ScaleRequest, PaginatedResponse, ErrorResponse};
pub use agent::{AgentConfig, AgentConfigJson, Capabilities, NodeCapacity, DesiredState, DesiredApp, DesiredVolume, VolumeMount};
