//! Invoice-related types for billing
//!
//! This module contains invoice entity types that are shared across
//! the billing system, control plane, and external integrations.

use chrono::{DateTime, Datelike, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Billing period
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct BillingPeriod {
    /// Period start date/time
    pub start: DateTime<Utc>,
    /// Period end date/time
    pub end: DateTime<Utc>,
}

impl BillingPeriod {
    /// Create a new billing period
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self { start, end }
    }

    /// Create a monthly billing period from a reference date
    pub fn monthly_from(date: DateTime<Utc>) -> Self {
        let start = date
            .date_naive()
            .with_day(1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
            .unwrap();

        let end = (start + chrono::Duration::days(30))
            .date_naive()
            .and_hms_opt(23, 59, 59)
            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
            .unwrap();

        Self { start, end }
    }
}

/// Invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct Invoice {
    /// Unique invoice identifier
    pub id: String,
    /// Human-readable invoice number (INV-YYYYMM-NNNN)
    pub invoice_number: String,
    /// Customer identifier
    pub customer_id: String,
    /// Customer name
    pub customer_name: String,
    /// Customer email
    pub customer_email: String,
    /// Billing period
    pub period: BillingPeriod,
    /// Line items
    pub line_items: Vec<LineItem>,
    /// Subtotal before credits
    #[cfg_attr(feature = "openapi", schemars(skip))]
    pub subtotal: Decimal,
    /// Credits applied
    #[cfg_attr(feature = "openapi", schemars(skip))]
    pub credit_applied: Decimal,
    /// Total amount due
    #[cfg_attr(feature = "openapi", schemars(skip))]
    pub total: Decimal,
    /// Currency (ISO 4217 code)
    pub currency: String,
    /// Invoice status
    pub status: InvoiceStatus,
    /// Payment due date
    pub due_date: DateTime<Utc>,
    /// When the invoice was created
    pub created_at: DateTime<Utc>,
    /// When the invoice was paid (if applicable)
    pub paid_at: Option<DateTime<Utc>>,
}

/// Invoice status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub enum InvoiceStatus {
    /// Invoice is being prepared
    Draft,
    /// Invoice is ready for payment
    Open,
    /// Invoice has been paid
    Paid,
    /// Invoice has been voided
    Void,
    /// Invoice cannot be collected
    Uncollectible,
}

/// Line item on invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct LineItem {
    /// Resource type identifier
    pub resource_type: String,
    /// Human-readable description
    pub description: String,
    /// Quantity consumed
    pub quantity: f64,
    /// Unit of measurement
    pub unit: String,
    /// Price per unit
    pub unit_price: f64,
    /// Total amount for this line item
    #[cfg_attr(feature = "openapi", schemars(skip))]
    pub amount: Decimal,
}

/// Payment result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema, schemars::JsonSchema))]
pub struct PaymentResult {
    /// Whether the payment succeeded
    pub success: bool,
    /// Transaction ID from payment provider
    pub transaction_id: Option<String>,
    /// Status message
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_billing_period_monthly() {
        let period = BillingPeriod::monthly_from(Utc::now());
        assert!(period.start < period.end);
    }

    #[test]
    fn test_invoice_serialization() {
        let invoice = Invoice {
            id: "inv_123".to_string(),
            invoice_number: "INV-202401-0001".to_string(),
            customer_id: "cust_123".to_string(),
            customer_name: "Test Customer".to_string(),
            customer_email: "test@example.com".to_string(),
            period: BillingPeriod::monthly_from(Utc::now()),
            line_items: vec![LineItem {
                resource_type: "cpu_hours".to_string(),
                description: "CPU Hours".to_string(),
                quantity: 100.0,
                unit: "hours".to_string(),
                unit_price: 0.025,
                amount: Decimal::new(250, 2),
            }],
            subtotal: Decimal::new(250, 2),
            credit_applied: Decimal::ZERO,
            total: Decimal::new(250, 2),
            currency: "USD".to_string(),
            status: InvoiceStatus::Open,
            due_date: Utc::now() + chrono::Duration::days(30),
            created_at: Utc::now(),
            paid_at: None,
        };

        let json = serde_json::to_string(&invoice).unwrap();
        let decoded: Invoice = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, invoice.id);
        assert_eq!(decoded.line_items.len(), 1);
    }

    #[test]
    fn test_payment_result() {
        let result = PaymentResult {
            success: true,
            transaction_id: Some("txn_123".to_string()),
            message: "Payment processed".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: PaymentResult = serde_json::from_str(&json).unwrap();
        assert!(decoded.success);
    }
}
