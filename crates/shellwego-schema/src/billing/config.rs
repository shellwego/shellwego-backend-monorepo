//! Configuration types for billing
//!
//! This module contains billing configuration types that are shared
//! across the billing system and control plane.

use serde::{Deserialize, Serialize};

/// Billing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BillingConfig {
    /// Default currency (ISO 4217 code)
    pub currency: String,
    /// Timezone for billing operations
    pub timezone: String,
    /// Day of month for invoice generation (1-28)
    pub invoice_day: u8,
    /// Payment terms in days (net N)
    pub payment_terms_days: u8,
    /// Stripe API key
    pub stripe_api_key: Option<String>,
    /// Paystack secret key
    pub paystack_secret_key: Option<String>,
    /// Path to invoice templates
    pub template_path: String,
    /// Metrics database DSN
    pub metrics_dsn: String,
    /// Retention period for usage data in days
    pub metering_retention_days: u32,
    /// Dunning configuration
    pub dunning: DunningConfig,
}

impl Default for BillingConfig {
    fn default() -> Self {
        Self {
            currency: "USD".to_string(),
            timezone: "UTC".to_string(),
            invoice_day: 1,
            payment_terms_days: 30,
            stripe_api_key: None,
            paystack_secret_key: None,
            template_path: "./templates".to_string(),
            metrics_dsn: "sqlite://billing.db".to_string(),
            metering_retention_days: 365,
            dunning: DunningConfig::default(),
        }
    }
}

/// Dunning (payment recovery) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct DunningConfig {
    /// Maximum retry attempts
    pub max_retries: u8,
    /// Days between retries
    pub retry_intervals_days: Vec<u8>,
    /// Email templates for each stage
    pub email_templates: Vec<String>,
    /// Suspend account after failed retries
    pub suspend_after_max_retries: bool,
}

impl Default for DunningConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_intervals_days: vec![3, 7, 14],
            email_templates: vec![
                "payment_reminder_1".to_string(),
                "payment_reminder_2".to_string(),
                "final_notice".to_string(),
            ],
            suspend_after_max_retries: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_billing_config_default() {
        let config = BillingConfig::default();
        assert_eq!(config.currency, "USD");
        assert_eq!(config.invoice_day, 1);
        assert_eq!(config.payment_terms_days, 30);
    }

    #[test]
    fn test_dunning_config_default() {
        let dunning = DunningConfig::default();
        assert_eq!(dunning.max_retries, 3);
        assert_eq!(dunning.retry_intervals_days.len(), 3);
        assert!(dunning.suspend_after_max_retries);
    }

    #[test]
    fn test_billing_config_serialization() {
        let config = BillingConfig {
            currency: "EUR".to_string(),
            timezone: "Europe/Berlin".to_string(),
            invoice_day: 15,
            payment_terms_days: 14,
            stripe_api_key: Some("sk_test_123".to_string()),
            paystack_secret_key: None,
            template_path: "/templates".to_string(),
            metrics_dsn: "postgres://localhost/billing".to_string(),
            metering_retention_days: 180,
            dunning: DunningConfig::default(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: BillingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.currency, "EUR");
        assert_eq!(decoded.invoice_day, 15);
    }
}
