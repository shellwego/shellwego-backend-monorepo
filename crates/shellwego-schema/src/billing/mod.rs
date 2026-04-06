//! Billing types for the ShellWeGo platform
//!
//! This module contains all billing-related domain entities that are
//! shared across the billing system, control plane, and external integrations.
//!
//! ## Module Organization
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `customer` | Customer entities (Customer, Address, SubscriptionTier) |
//! | `invoice` | Invoice entities (Invoice, LineItem, BillingPeriod) |
//! | `usage` | Usage tracking types (UsageEvent, UsageSummary) |
//! | `config` | Configuration types (BillingConfig, DunningConfig) |
//!
//! ## Design Principles
//!
//! 1. **Pure Data**: No business logic, only data structures
//! 2. **Wire Format**: Types define API contracts between services
//! 3. **Feature Flags**: Optional derives for OpenAPI schemas
//! 4. **Serializable**: All types implement Serialize/Deserialize

pub mod config;
pub mod customer;
pub mod invoice;
pub mod usage;

// Re-export all public types at module level
pub use config::{
    BillingConfig, CryptoConfig, CryptoCurrencyConfig, DunningConfig, GcashConfig,
    MpesaConfig, MpesaEnvironment, MercadoPagoConfig, UpiConfig,
};
pub use customer::{Address, Customer, CustomerStatus, PaymentMethod, SubscriptionTier};
pub use invoice::{BillingPeriod, Invoice, InvoiceStatus, LineItem, PaymentResult};
pub use usage::{UsageEvent, UsageSummary};
