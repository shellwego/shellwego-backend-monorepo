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
    /// Stripe webhook signing secret (whsec_...)
    pub stripe_webhook_secret: Option<String>,
    /// M-Pesa (Safaricom Daraja) configuration
    pub mpesa_config: Option<MpesaConfig>,
    /// GCash configuration (via PayMongo)
    pub gcash_config: Option<GcashConfig>,
    /// UPI configuration (via Razorpay)
    pub upi_config: Option<UpiConfig>,
    /// Mercado Pago configuration
    pub mercadopago_config: Option<MercadoPagoConfig>,
    /// Crypto payment configuration
    pub crypto_config: Option<CryptoConfig>,
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
            stripe_webhook_secret: None,
            mpesa_config: None,
            gcash_config: None,
            upi_config: None,
            mercadopago_config: None,
            crypto_config: None,
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

/// M-Pesa (Safaricom Daraja) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MpesaConfig {
    /// M-Pesa consumer key
    pub consumer_key: String,
    /// M-Pesa consumer secret
    pub consumer_secret: String,
    /// M-Pesa passkey (Lipa Na M-Pesa Online)
    pub passkey: String,
    /// Business short code
    pub business_short_code: String,
    /// M-Pesa environment (sandbox or production)
    pub environment: MpesaEnvironment,
    /// Callback URL for payment notifications
    pub callback_url: String,
}

/// M-Pesa API environment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum MpesaEnvironment {
    /// Sandbox environment for testing
    Sandbox,
    /// Production environment
    Production,
}

/// GCash payment configuration (via PayMongo)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct GcashConfig {
    /// PayMongo public API key
    pub paymongo_public_key: String,
    /// PayMongo secret API key
    pub paymongo_secret_key: String,
    /// Webhook signing secret
    pub webhook_secret: String,
}

/// UPI payment configuration (via Razorpay)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct UpiConfig {
    /// Razorpay key ID
    pub razorpay_key_id: String,
    /// Razorpay key secret
    pub razorpay_key_secret: String,
    /// Webhook signing secret
    pub webhook_secret: String,
}

/// Mercado Pago configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct MercadoPagoConfig {
    /// Mercado Pago access token
    pub access_token: String,
    /// Webhook signing secret
    pub webhook_secret: String,
}

/// Cryptocurrency payment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct CryptoConfig {
    /// Blockchain exploration API URL (e.g., "https://mempool.space/api")
    pub mempool_api_url: String,
    /// Number of required blockchain confirmations
    pub required_confirmations: u32,
    /// Conversion rate provider URL
    pub rate_api_url: String,
    /// Supported cryptocurrencies
    pub supported_currencies: Vec<CryptoCurrencyConfig>,
}

/// Individual cryptocurrency configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct CryptoCurrencyConfig {
    /// Currency code (e.g., "BTC", "ETH", "USDC")
    pub code: String,
    /// Network identifier (e.g., "bitcoin", "ethereum", "polygon")
    pub network: String,
    /// Decimal places for this currency
    pub decimals: u8,
    /// Confirmation timeout in minutes
    pub confirmation_timeout_minutes: u32,
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
            stripe_webhook_secret: Some("whsec_test_123".to_string()),
            mpesa_config: None,
            gcash_config: None,
            upi_config: None,
            mercadopago_config: None,
            crypto_config: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let decoded: BillingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.currency, "EUR");
        assert_eq!(decoded.invoice_day, 15);
    }
}
