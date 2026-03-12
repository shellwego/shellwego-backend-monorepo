//! Customer-related types for billing
//!
//! This module contains customer entity types that are shared across
//! the billing system and control plane.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Customer information for billing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Customer {
    /// Unique customer identifier
    pub id: String,
    /// Organization or user name
    pub name: String,
    /// Primary email for invoicing
    pub email: String,
    /// Billing address
    pub address: Option<Address>,
    /// Payment methods on file
    pub payment_methods: Vec<PaymentMethod>,
    /// Current subscription tier
    pub tier: SubscriptionTier,
    /// Account credits (in smallest currency unit, e.g., cents)
    pub credits: i64,
    /// Currency preference (ISO 4217 code)
    pub currency: String,
    /// Tax ID (VAT, GST, etc.)
    pub tax_id: Option<String>,
    /// Account creation timestamp
    pub created_at: DateTime<Utc>,
    /// Account status
    pub status: CustomerStatus,
}

/// Customer address
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Address {
    /// Street address line 1
    pub line1: String,
    /// Street address line 2 (optional)
    pub line2: Option<String>,
    /// City
    pub city: String,
    /// State or province
    pub state: Option<String>,
    /// Postal/ZIP code
    pub postal_code: String,
    /// Country (ISO 3166-1 alpha-2 code recommended)
    pub country: String,
}

/// Subscription tier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum SubscriptionTier {
    /// Free tier with limited resources
    Free,
    /// Starter tier for small projects
    Starter,
    /// Growth tier for scaling applications
    Growth,
    /// Enterprise tier with dedicated support
    Enterprise,
    /// Custom tier with negotiated terms
    Custom {
        /// Custom tier name
        name: String,
    },
}

/// Customer account status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum CustomerStatus {
    /// Account is active and in good standing
    Active,
    /// Account has past due invoices
    PastDue,
    /// Account is suspended due to payment issues
    Suspended,
    /// Account is closed
    Closed,
}

/// Payment method
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum PaymentMethod {
    /// Credit/debit card
    Card {
        /// Payment provider token
        token: String,
    },
    /// Bank account transfer
    BankTransfer {
        /// Bank account identifier
        account_id: String,
    },
    /// Digital wallet (PayPal, Apple Pay, etc.)
    Wallet {
        /// Wallet provider name
        provider: String,
        /// Provider token
        token: String,
    },
    /// Cryptocurrency
    Crypto {
        /// Currency type (BTC, ETH, USDC, etc.)
        currency: String,
        /// Wallet address
        address: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_customer_serialization() {
        let customer = Customer {
            id: "cust_123".to_string(),
            name: "Test Company".to_string(),
            email: "billing@test.com".to_string(),
            address: Some(Address {
                line1: "123 Main St".to_string(),
                line2: None,
                city: "San Francisco".to_string(),
                state: Some("CA".to_string()),
                postal_code: "94102".to_string(),
                country: "US".to_string(),
            }),
            payment_methods: vec![],
            tier: SubscriptionTier::Growth,
            credits: 1000,
            currency: "USD".to_string(),
            tax_id: None,
            created_at: Utc::now(),
            status: CustomerStatus::Active,
        };

        let json = serde_json::to_string(&customer).unwrap();
        let decoded: Customer = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, customer.id);
    }

    #[test]
    fn test_subscription_tier_custom() {
        let tier = SubscriptionTier::Custom {
            name: "Enterprise Plus".to_string(),
        };
        let json = serde_json::to_string(&tier).unwrap();
        let decoded: SubscriptionTier = serde_json::from_str(&json).unwrap();
        assert_eq!(tier, decoded);
    }

    #[test]
    fn test_payment_method_card() {
        let method = PaymentMethod::Card {
            token: "tok_visa_1234".to_string(),
        };
        let json = serde_json::to_string(&method).unwrap();
        assert!(json.contains("Card"));
    }
}
