//! Usage-related types for billing
//!
//! This module contains usage tracking types that are shared across
//! the billing system, agent, and control plane.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::LineItem;

/// Usage event from resource consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct UsageEvent {
    /// Customer identifier
    pub customer_id: String,
    /// Type of resource (cpu_hours, memory_gb_hours, storage_gb, etc.)
    pub resource_type: String,
    /// Quantity consumed
    pub quantity: f64,
    /// When the usage occurred
    pub timestamp: DateTime<Utc>,
    /// Additional metadata (region, app_id, etc.)
    pub metadata: HashMap<String, String>,
}

impl UsageEvent {
    /// Create a new usage event
    pub fn new(customer_id: impl Into<String>, resource_type: impl Into<String>, quantity: f64) -> Self {
        Self {
            customer_id: customer_id.into(),
            resource_type: resource_type.into(),
            quantity,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the event
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Usage summary for a billing period
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct UsageSummary {
    /// Customer identifier
    pub customer_id: String,
    /// Period start
    pub period_start: DateTime<Utc>,
    /// Period end
    pub period_end: DateTime<Utc>,
    /// Line items for each resource type
    pub line_items: Vec<LineItem>,
    /// Subtotal before credits
    #[cfg_attr(feature = "openapi", schemars(skip))]
    pub subtotal: Decimal,
    /// Currency (ISO 4217 code)
    pub currency: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_event_creation() {
        let event = UsageEvent::new("cust_123", "cpu_hours", 5.5).with_metadata("region", "us-east-1");

        assert_eq!(event.customer_id, "cust_123");
        assert_eq!(event.resource_type, "cpu_hours");
        assert_eq!(event.quantity, 5.5);
        assert_eq!(event.metadata.get("region"), Some(&"us-east-1".to_string()));
    }

    #[test]
    fn test_usage_event_serialization() {
        let event = UsageEvent::new("cust_456", "memory_gb_hours", 10.0);

        let json = serde_json::to_string(&event).unwrap();
        let decoded: UsageEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.customer_id, event.customer_id);
        assert_eq!(decoded.resource_type, event.resource_type);
    }

    #[test]
    fn test_usage_summary() {
        let summary = UsageSummary {
            customer_id: "cust_123".to_string(),
            period_start: Utc::now(),
            period_end: Utc::now() + chrono::Duration::days(30),
            line_items: vec![],
            subtotal: Decimal::new(100, 0),
            currency: "USD".to_string(),
        };

        let json = serde_json::to_string(&summary).unwrap();
        let decoded: UsageSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.customer_id, summary.customer_id);
    }
}
