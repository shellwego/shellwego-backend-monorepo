//! UPI payment provider implementation via Razorpay.
//!
//! Supports UPI payments through Razorpay's order-based flow,
//! refunds, webhook verification using HMAC-SHA256, and status polling.

use std::collections::HashMap;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{debug, info, warn};

use super::*;
use shellwego_schema::billing::UpiConfig;

/// Razorpay base URL.
const RAZORPAY_BASE_URL: &str = "https://api.razorpay.com/v1";

/// UPI payment provider using Razorpay API.
pub struct UpiProvider {
    /// Razorpay configuration.
    config: UpiConfig,
    /// Reusable HTTP client.
    http_client: reqwest::Client,
}

impl UpiProvider {
    /// Create a new UPI provider via Razorpay.
    pub fn new(config: UpiConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build Razorpay HTTP client"),
        }
    }

    // ----------------------------------------------------------------
    // Helpers
    // ----------------------------------------------------------------

    /// Build an authenticated request with Razorpay basic auth.
    fn authenticated(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url)
            .basic_auth(&self.config.razorpay_key_id, Some(&self.config.razorpay_key_secret))
    }
}

#[async_trait]
impl PaymentProvider for UpiProvider {
    fn provider_name(&self) -> &str {
        "upi"
    }

    // ----------------------------------------------------------------
    // Charge (Create Order)
    // ----------------------------------------------------------------

    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError> {
        info!(
            invoice_id = %request.invoice_id,
            amount = request.amount_cents,
            "Creating Razorpay order"
        );

        // Razorpay amounts are in paise (cents for INR).
        let amount = request.amount_cents;

        let mut notes = serde_json::Map::new();
        notes.insert("invoice_id".to_string(), serde_json::Value::String(request.invoice_id.clone()));
        notes.insert("customer_id".to_string(), serde_json::Value::String(request.customer_id.clone()));
        notes.insert("description".to_string(), serde_json::Value::String(request.description.clone()));
        for (k, v) in &request.metadata {
            notes.insert(k.clone(), serde_json::Value::String(v.clone()));
        }

        let body = serde_json::json!({
            "amount": amount,
            "currency": request.currency,
            "receipt": request.invoice_id,
            "notes": notes,
            "payment_capture": 1,  // Auto-capture
        });

        let resp = self
            .authenticated(reqwest::Method::POST, &format!("{}/orders", RAZORPAY_BASE_URL))
            .json(&body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Razorpay order creation failed: {}",
                e
            )))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse Razorpay order response: {}",
                e
            )))?;

        if !status.is_success() {
            let error_field = resp_body["error"]["description"]
                .as_str()
                .unwrap_or("Unknown Razorpay error");
            warn!(error = error_field, "Razorpay order creation failed");
            return Ok(PaymentResult {
                success: false,
                transaction_id: None,
                message: error_field.to_string(),
            });
        }

        let order_id = resp_body["id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let order_status = resp_body["status"]
            .as_str()
            .unwrap_or("created");

        debug!(
            order_id = %order_id,
            status = order_status,
            "Razorpay order created"
        );

        // Razorpay orders need to be paid via the Razorpay frontend SDK.
        // The order_id is returned for later capture and status polling.
        Ok(PaymentResult {
            success: true,
            transaction_id: Some(order_id.clone()),
            message: format!(
                "Order created. Use Razorpay SDK with order_id={} to capture UPI payment.",
                order_id
            ),
        })
    }

    // ----------------------------------------------------------------
    // Refund
    // ----------------------------------------------------------------

    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError> {
        info!(
            transaction_id = %request.original_transaction_id,
            "Creating Razorpay refund"
        );

        let mut body = serde_json::json!({
            "payment_id": request.original_transaction_id,
            "notes": {
                "reason": request.reason,
            }
        });

        if let Some(amount) = request.amount_cents {
            body["amount"] = serde_json::json!(amount);
        }

        let resp = self
            .authenticated(reqwest::Method::POST, &format!("{}/refunds", RAZORPAY_BASE_URL))
            .json(&body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Razorpay refund request failed: {}",
                e
            )))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse Razorpay refund response: {}",
                e
            )))?;

        if !status.is_success() {
            let error_msg = resp_body["error"]["description"]
                .as_str()
                .unwrap_or("Unknown Razorpay refund error");
            warn!(error = error_msg, "Razorpay refund failed");
            return Ok(RefundResult {
                refund_id: String::new(),
                status: RefundStatus::Failed,
                amount_cents_refunded: 0,
                message: error_msg.to_string(),
            });
        }

        // Razorpay returns an array of refunds; take the first.
        let refund = resp_body
            .as_array()
            .and_then(|arr| arr.first())
            .unwrap_or(&resp_body);

        let refund_id = refund["id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let amount = refund["amount"].as_i64().unwrap_or(0);
        let refund_status = refund["status"]
            .as_str()
            .unwrap_or("failed");

        let mapped_status = match refund_status {
            "pending" => RefundStatus::Pending,
            "processed" => RefundStatus::Succeeded,
            _ => RefundStatus::Failed,
        };

        Ok(RefundResult {
            refund_id,
            status: mapped_status,
            amount_cents_refunded: amount,
            message: format!("Refund {}", refund_status),
        })
    }

    // ----------------------------------------------------------------
    // Webhook verification (HMAC-SHA256)
    // ----------------------------------------------------------------

    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<bool, BillingError> {
        // Razorpay signs webhooks with HMAC-SHA256(webhook_secret, raw_body)
        // The signature is hex-encoded.
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.config.webhook_secret.as_bytes()).map_err(|e| {
                BillingError::WebhookVerificationError(format!(
                    "Failed to create HMAC: {}",
                    e
                ))
            })?;
        mac.update(payload);
        let result = mac.finalize();
        let expected = hex::encode(result.into_bytes());

        let matches =
            super::constant_time_compare(expected.as_bytes(), signature.trim().as_bytes());

        debug!(verified = matches, "Razorpay webhook verification");
        Ok(matches)
    }

    // ----------------------------------------------------------------
    // Webhook parsing
    // ----------------------------------------------------------------

    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError> {
        let body: serde_json::Value = serde_json::from_slice(payload)?;

        let razorpay_event = body["event"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let event_type = match razorpay_event.as_str() {
            "payment.captured" => WebhookEventType::PaymentSucceeded,
            "payment.authorized" => WebhookEventType::PaymentPending,
            "payment.failed" => WebhookEventType::PaymentFailed,
            "payment.refunded" => WebhookEventType::PaymentRefunded,
            "refund.created" => WebhookEventType::PaymentRefunded,
            "refund.processed" => WebhookEventType::PaymentRefunded,
            "subscription.created" => WebhookEventType::SubscriptionCreated,
            "subscription.updated" => WebhookEventType::SubscriptionUpdated,
            "subscription.halted" | "subscription.cancelled" => {
                WebhookEventType::SubscriptionCanceled
            }
            "subscription.charged" => WebhookEventType::SubscriptionRenewed,
            "dispute.created" => WebhookEventType::DisputeCreated,
            "dispute.won" => WebhookEventType::DisputeWon,
            "dispute.lost" => WebhookEventType::DisputeLost,
            _ => WebhookEventType::Unknown,
        };

        // Extract payload entity (contains payment/order/refund details)
        let payload_entity = &body["payload"]["payment"]
            .or_else(|| body["payload"].as_object().and_then(|obj| {
                obj.values().next()
            }));

        let provider_payment_id = payload_entity
            .and_then(|p| p["id"].as_str())
            .map(String::from);
        let amount_cents = payload_entity.and_then(|p| p["amount"].as_i64());
        let currency = payload_entity.and_then(|p| p["currency"].as_str()).map(String::from);
        let order_id = payload_entity.and_then(|p| p["order_id"].as_str()).map(String::from);
        let notes = payload_entity
            .and_then(|p| p["notes"].as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<HashMap<String, serde_json::Value>>()
            })
            .unwrap_or_default();

        let invoice_id = notes
            .get("invoice_id")
            .and_then(|v| v.as_str())
            .map(String::from);
        let customer_id = notes
            .get("customer_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        let failure_reason = payload_entity
            .and_then(|p| {
                p["error_description"]
                    .as_str()
                    .or_else(|| p["error_code"].as_str())
            })
            .map(String::from);

        // If payload contains order info, set transaction_id from order
        let transaction_id = body["payload"]["order"]["id"]
            .as_str()
            .map(String::from)
            .or(order_id);

        debug!(
            event_type = ?event_type,
            razorpay_event = %razorpay_event,
            "Parsed Razorpay webhook event"
        );

        Ok(ParsedWebhookEvent {
            event_type,
            provider_event_type: razorpay_event,
            invoice_id,
            provider_payment_id,
            transaction_id,
            amount_cents,
            currency,
            failure_reason,
            customer_id,
            metadata: notes,
        })
    }

    // ----------------------------------------------------------------
    // Payment status polling
    // ----------------------------------------------------------------

    async fn check_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<PaymentStatus, BillingError> {
        // If the ID looks like an order (starts with "order_"), fetch order status
        // Otherwise, fetch payment status
        let url = if provider_payment_id.starts_with("order_") {
            format!("{}/orders/{}", RAZORPAY_BASE_URL, provider_payment_id)
        } else {
            format!("{}/payments/{}", RAZORPAY_BASE_URL, provider_payment_id)
        };

        let resp = self
            .authenticated(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Razorpay status check failed: {}",
                e
            )))?;

        let status_code = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse Razorpay status response: {}",
                e
            )))?;

        if !status_code.is_success() {
            return Ok(PaymentStatus::Unknown(format!("HTTP {}", status_code)));
        }

        // Check payments array in order response, or entity directly
        let razorpay_status = body["status"]
            .as_str()
            .or_else(|| {
                body["payments"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|p| p["status"].as_str())
            })
            .or_else(|| body["entity"].as_str())
            .unwrap_or("unknown");

        let mapped = match razorpay_status {
            "captured" | "paid" => PaymentStatus::Succeeded,
            "authorized" => PaymentStatus::Pending,
            "created" => PaymentStatus::Pending,
            "attempted" => PaymentStatus::Pending,
            "failed" => PaymentStatus::Failed,
            "refunded" => PaymentStatus::Refunded,
            "partially_refunded" => PaymentStatus::PartiallyRefunded,
            "disputed" => PaymentStatus::Disputed,
            _ => PaymentStatus::Unknown(razorpay_status.to_string()),
        };

        debug!(
            payment_id = %provider_payment_id,
            status = ?mapped,
            "Razorpay payment status"
        );

        Ok(mapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> UpiConfig {
        UpiConfig {
            razorpay_key_id: "rzp_test_key".to_string(),
            razorpay_key_secret: "secret_key".to_string(),
            webhook_secret: "whsecret_test".to_string(),
        }
    }

    #[test]
    fn test_upi_provider_name() {
        let provider = UpiProvider::new(make_config());
        assert_eq!(provider.provider_name(), "upi");
    }

    #[test]
    fn test_parse_webhook_event_captured() {
        let provider = UpiProvider::new(make_config());
        let payload = r#"{
            "event": "payment.captured",
            "payload": {
                "payment": {
                    "entity": {
                        "id": "pay_abc123",
                        "order_id": "order_abc",
                        "amount": 50000,
                        "currency": "INR",
                        "status": "captured",
                        "notes": {
                            "invoice_id": "inv_001",
                            "customer_id": "cust_001"
                        }
                    }
                }
            }
        }"#;

        let event = provider.parse_webhook_event(payload.as_bytes()).unwrap();
        assert_eq!(event.event_type, WebhookEventType::PaymentSucceeded);
        assert_eq!(event.provider_payment_id, Some("pay_abc123".to_string()));
        assert_eq!(event.amount_cents, Some(50000));
    }

    #[test]
    fn test_parse_webhook_event_failed() {
        let provider = UpiProvider::new(make_config());
        let payload = r#"{
            "event": "payment.failed",
            "payload": {
                "payment": {
                    "entity": {
                        "id": "pay_fail",
                        "amount": 50000,
                        "currency": "INR",
                        "error_description": "Payment authorization failed"
                    }
                }
            }
        }"#;

        let event = provider.parse_webhook_event(payload.as_bytes()).unwrap();
        assert_eq!(event.event_type, WebhookEventType::PaymentFailed);
        assert_eq!(
            event.failure_reason,
            Some("Payment authorization failed".to_string())
        );
    }
}
