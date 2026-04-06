//! Mock payment provider for testing

use super::*;
use std::sync::{Arc, RwLock};

/// Records all calls made to the mock provider for test assertions.
#[derive(Debug, Default)]
pub struct MockCalls {
    pub charges: Vec<ChargeRequest>,
    pub refunds: Vec<RefundRequest>,
    pub webhook_verifications: usize,
    pub status_checks: Vec<String>,
}

/// Mock payment provider that returns configurable results.
#[derive(Debug, Clone)]
pub struct MockProvider {
    pub charge_should_succeed: bool,
    pub charge_delay_ms: u64,
    pub transaction_id_prefix: String,
    pub calls: Arc<RwLock<MockCalls>>,
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            charge_should_succeed: true,
            charge_delay_ms: 0,
            transaction_id_prefix: "mock_txn_".to_string(),
            calls: Arc::new(RwLock::new(MockCalls::default())),
        }
    }

    pub fn failing() -> Self {
        let mut mock = Self::new();
        mock.charge_should_succeed = false;
        mock
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PaymentProvider for MockProvider {
    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError> {
        if self.charge_delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.charge_delay_ms)).await;
        }
        self.calls.write().unwrap().charges.push(request);
        Ok(PaymentResult {
            success: self.charge_should_succeed,
            transaction_id: Some(format!("{}{}", self.transaction_id_prefix, uuid::Uuid::new_v4())),
            message: if self.charge_should_succeed {
                "Mock: payment succeeded".to_string()
            } else {
                "Mock: card declined".to_string()
            },
        })
    }

    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError> {
        self.calls.write().unwrap().refunds.push(request);
        Ok(RefundResult {
            refund_id: format!("mock_ref_{}", uuid::Uuid::new_v4()),
            status: RefundStatus::Succeeded,
            amount_cents_refunded: 0,
            message: "Mock: refund succeeded".to_string(),
        })
    }

    fn verify_webhook(&self, _payload: &[u8], _signature: &str) -> Result<bool, BillingError> {
        self.calls.write().unwrap().webhook_verifications += 1;
        Ok(true)
    }

    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError> {
        let body: serde_json::Value = serde_json::from_slice(payload)?;
        Ok(ParsedWebhookEvent {
            event_type: WebhookEventType::PaymentSucceeded,
            provider_event_type: body
                .get("event_type")
                .and_then(|v| v.as_str())
                .unwrap_or("mock.event")
                .to_string(),
            invoice_id: body.get("invoice_id").and_then(|v| v.as_str()).map(String::from),
            provider_payment_id: body
                .get("transaction_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            transaction_id: None,
            amount_cents: body.get("amount_cents").and_then(|v| v.as_i64()),
            currency: body.get("currency").and_then(|v| v.as_str()).map(String::from),
            failure_reason: None,
            customer_id: None,
            metadata: Default::default(),
        })
    }

    async fn check_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<PaymentStatus, BillingError> {
        self.calls.write().unwrap().status_checks.push(provider_payment_id.to_string());
        Ok(PaymentStatus::Succeeded)
    }
}
