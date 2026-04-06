//! Payment provider implementations
//!
//! Each provider implements the `PaymentProvider` trait for charge
//! creation, refund processing, webhook verification, and status polling.

pub mod stripe;
pub mod paystack;
pub mod mpesa;
pub mod gcash;
pub mod upi;
pub mod mercadopago;
pub mod crypto;
#[cfg(test)]
pub mod mock;

use crate::BillingError;
use crate::PaymentResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result of a refund operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundResult {
    /// Refund identifier from the provider
    pub refund_id: String,
    /// Current refund status
    pub status: RefundStatus,
    /// Amount refunded in cents
    pub amount_cents_refunded: i64,
    /// Status message
    pub message: String,
}

/// Refund status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RefundStatus {
    /// Refund is being processed
    Pending,
    /// Refund completed successfully
    Succeeded,
    /// Refund failed
    Failed,
}

/// A parsed webhook event from any provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedWebhookEvent {
    /// Provider-agnostic event type
    pub event_type: WebhookEventType,
    /// Raw provider event type string
    pub provider_event_type: String,
    /// Invoice ID (if extractable from the event)
    pub invoice_id: Option<String>,
    /// Transaction/payment ID from the provider
    pub provider_payment_id: Option<String>,
    /// Transaction ID in our system
    pub transaction_id: Option<String>,
    /// Amount in cents
    pub amount_cents: Option<i64>,
    /// Currency code
    pub currency: Option<String>,
    /// Failure reason (if applicable)
    pub failure_reason: Option<String>,
    /// Customer ID
    pub customer_id: Option<String>,
    /// Metadata key-value pairs
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// Normalized webhook event types across providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WebhookEventType {
    PaymentSucceeded,
    PaymentFailed,
    PaymentPending,
    PaymentRefunded,
    PaymentPartiallyRefunded,
    SubscriptionCreated,
    SubscriptionUpdated,
    SubscriptionCanceled,
    SubscriptionRenewed,
    DisputeCreated,
    DisputeWon,
    DisputeLost,
    Unknown,
}

/// Common payment charge request.
#[derive(Debug, Clone)]
pub struct ChargeRequest {
    pub invoice_id: String,
    pub customer_id: String,
    pub amount_cents: i64,
    pub currency: String,
    pub payment_token: String,
    pub description: String,
    pub metadata: std::collections::HashMap<String, String>,
    pub idempotency_key: Option<String>,
}

/// Common refund request.
#[derive(Debug, Clone)]
pub struct RefundRequest {
    pub original_transaction_id: String,
    pub amount_cents: Option<i64>,
    pub reason: String,
    pub idempotency_key: Option<String>,
}

/// Trait that all payment providers must implement.
#[async_trait]
pub trait PaymentProvider: Send + Sync {
    /// Provider identifier (e.g., "stripe", "paystack", "mpesa")
    fn provider_name(&self) -> &str;

    /// Create a payment charge.
    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError>;

    /// Process a refund.
    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError>;

    /// Verify a webhook signature.
    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<bool, BillingError>;

    /// Parse a provider-specific webhook payload into a normalized event.
    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError>;

    /// Poll the status of a payment (for async providers like M-Pesa, crypto).
    async fn check_payment_status(&self, provider_payment_id: &str)
        -> Result<PaymentStatus, BillingError>;
}

/// Payment status from provider polling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaymentStatus {
    Pending,
    Succeeded,
    Failed,
    Refunded,
    PartiallyRefunded,
    Disputed,
    Unknown(String),
}

/// Constant-time byte comparison to prevent timing attacks.
pub fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
