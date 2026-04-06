//! Paystack payment provider implementation.
//!
//! Supports redirect-based payments via Paystack's Transaction Initialize API,
//! refunds, webhook verification using HMAC-SHA512, and status polling.

use std::collections::HashMap;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha512;
use tracing::{debug, info, warn};

use super::*;

/// Paystack payment provider.
pub struct PaystackProvider {
    /// Paystack secret key
    secret_key: String,
    /// Reusable HTTP client.
    http_client: reqwest::Client,
}

impl PaystackProvider {
    /// Create a new Paystack provider.
    ///
    /// * `secret_key` – Paystack secret key (starts with `sk_test_` or `sk_live_`).
    pub fn new(secret_key: String) -> Self {
        Self {
            secret_key,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build Paystack HTTP client"),
        }
    }

    // ----------------------------------------------------------------
    // Helpers
    // ----------------------------------------------------------------

    /// Build an authenticated request with Bearer token.
    fn authenticated(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url)
            .bearer_auth(&self.secret_key)
    }

    /// Map a Paystack event string to a normalized [`WebhookEventType`].
    fn map_event_type(paystack_event: &str) -> WebhookEventType {
        match paystack_event {
            "charge.success" => WebhookEventType::PaymentSucceeded,
            "charge.failed" => WebhookEventType::PaymentFailed,
            "charge.refunded" => WebhookEventType::PaymentRefunded,
            "transfer.success" => WebhookEventType::PaymentSucceeded,
            "transfer.failed" => WebhookEventType::PaymentFailed,
            "subscription.create" => WebhookEventType::SubscriptionCreated,
            "subscription.disable" => WebhookEventType::SubscriptionCanceled,
            _ => WebhookEventType::Unknown,
        }
    }
}

#[async_trait]
impl PaymentProvider for PaystackProvider {
    fn provider_name(&self) -> &str {
        "paystack"
    }

    // ----------------------------------------------------------------
    // Charge (Initialize Transaction)
    // ----------------------------------------------------------------

    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError> {
        info!(
            invoice_id = %request.invoice_id,
            amount = request.amount_cents,
            "Initializing Paystack transaction"
        );

        // Paystack expects amount in smallest currency unit (kobo for NGN, pesewas for GHS)
        let amount = request.amount_cents;

        // Extract email from metadata or use a default
        let email = request
            .metadata
            .get("email")
            .cloned()
            .unwrap_or_else(|| "customer@shellwego.com".to_string());

        let mut body = serde_json::json!({
            "email": email,
            "amount": amount,
            "currency": request.currency,
        });

        // Add metadata
        let mut meta = serde_json::Map::new();
        meta.insert("invoice_id".to_string(), serde_json::Value::String(request.invoice_id.clone()));
        meta.insert("customer_id".to_string(), serde_json::Value::String(request.customer_id.clone()));
        meta.insert("description".to_string(), serde_json::Value::String(request.description.clone()));
        for (k, v) in &request.metadata {
            meta.insert(k.clone(), serde_json::Value::String(v.clone()));
        }
        body["metadata"] = serde_json::Value::Object(meta);

        let resp = self
            .authenticated(reqwest::Method::POST, "https://api.paystack.com/v1/transaction/initialize")
            .json(&body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Paystack charge request failed: {}",
                e
            )))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse Paystack response: {}",
                e
            )))?;

        if !status.is_success() || resp_body["status"].as_bool() == Some(false) {
            let message = resp_body["message"]
                .as_str()
                .unwrap_or("Unknown Paystack error")
                .to_string();
            warn!(error = %message, "Paystack charge failed");
            return Ok(PaymentResult {
                success: false,
                transaction_id: resp_body["data"]["reference"]
                    .as_str()
                    .map(String::from),
                message,
            });
        }

        let reference = resp_body["data"]["reference"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let authorization_url = resp_body["data"]["authorization_url"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let access_code = resp_body["data"]["access_code"]
            .as_str()
            .unwrap_or("")
            .to_string();

        debug!(
            reference = %reference,
            access_code = %access_code,
            "Paystack transaction initialized"
        );

        // Paystack is redirect-based: the customer must visit the authorization_url.
        // We return success=true for the initialization, and the redirect URL in message.
        Ok(PaymentResult {
            success: true,
            transaction_id: Some(reference),
            message: format!("Redirect customer to: {}", authorization_url),
        })
    }

    // ----------------------------------------------------------------
    // Refund
    // ----------------------------------------------------------------

    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError> {
        info!(
            transaction_id = %request.original_transaction_id,
            "Creating Paystack refund"
        );

        let mut body = serde_json::json!({
            "transaction": request.original_transaction_id,
        });

        if let Some(amount) = request.amount_cents {
            body["amount"] = serde_json::json!(amount);
        }

        let resp = self
            .authenticated(reqwest::Method::POST, "https://api.paystack.com/v1/refund")
            .json(&body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Paystack refund request failed: {}",
                e
            )))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse Paystack refund response: {}",
                e
            )))?;

        if !status.is_success() || resp_body["status"].as_bool() == Some(false) {
            let message = resp_body["message"]
                .as_str()
                .unwrap_or("Unknown Paystack refund error")
                .to_string();
            warn!(error = %message, "Paystack refund failed");
            return Ok(RefundResult {
                refund_id: String::new(),
                status: RefundStatus::Failed,
                amount_cents_refunded: 0,
                message,
            });
        }

        let refund_data = &resp_body["data"];
        let refund_id = refund_data["id"]
            .as_i64()
            .map(|id| id.to_string())
            .unwrap_or_default();
        let amount = refund_data["amount"].as_i64().unwrap_or(0);
        let refund_status = refund_data["status"]
            .as_str()
            .unwrap_or("failed");

        let mapped_status = match refund_status {
            "pending" => RefundStatus::Pending,
            "processed" | "completed" => RefundStatus::Succeeded,
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
    // Webhook verification (HMAC-SHA512)
    // ----------------------------------------------------------------

    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<bool, BillingError> {
        // Paystack sends the raw hex-encoded HMAC-SHA512 of the raw body
        // using the secret key as the HMAC key.
        let mut mac =
            Hmac::<Sha512>::new_from_slice(self.secret_key.as_bytes()).map_err(|e| {
                BillingError::WebhookVerificationError(format!(
                    "Failed to create HMAC: {}",
                    e
                ))
            })?;
        mac.update(payload);
        let result = mac.finalize();
        let expected_hex = hex::encode(result.into_bytes());

        let matches = super::constant_time_compare(
            expected_hex.as_bytes(),
            signature.trim().as_bytes(),
        );

        debug!(verified = matches, "Paystack webhook verification");
        Ok(matches)
    }

    // ----------------------------------------------------------------
    // Webhook parsing
    // ----------------------------------------------------------------

    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError> {
        let body: serde_json::Value = serde_json::from_slice(payload)?;

        let paystack_event = body["event"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let event_type = Self::map_event_type(&paystack_event);

        let data = &body["data"];

        let provider_payment_id = data["id"]
            .as_i64()
            .map(|id| id.to_string())
            .or_else(|| data["reference"].as_str().map(String::from));
        let amount_cents = data["amount"].as_i64();
        let currency = data["currency"].as_str().map(String::from);
        let customer_id = data["customer"]["email"]
            .as_str()
            .or_else(|| data["metadata"]["customer_id"].as_str())
            .map(String::from);
        let invoice_id = data["metadata"]["invoice_id"].as_str().map(String::from);
        let gateway_response = data["gateway_response"]
            .as_str()
            .unwrap_or("");
        let failure_reason = if !event_type.eq(&WebhookEventType::PaymentSucceeded) {
            Some(gateway_response.to_string())
        } else {
            None
        };

        let metadata = data["metadata"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();

        debug!(
            event_type = ?event_type,
            provider_event_type = %paystack_event,
            "Parsed Paystack webhook event"
        );

        Ok(ParsedWebhookEvent {
            event_type,
            provider_event_type: paystack_event,
            invoice_id,
            provider_payment_id,
            transaction_id: None,
            amount_cents,
            currency,
            failure_reason,
            customer_id,
            metadata,
        })
    }

    // ----------------------------------------------------------------
    // Payment status polling (Verify Transaction)
    // ----------------------------------------------------------------

    async fn check_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<PaymentStatus, BillingError> {
        let url = format!(
            "https://api.paystack.com/v1/transaction/{}/verify",
            provider_payment_id
        );

        let resp = self
            .authenticated(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Paystack status check failed: {}",
                e
            )))?;

        let status_code = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse Paystack verify response: {}",
                e
            )))?;

        if !status_code.is_success() || body["status"].as_bool() == Some(false) {
            return Ok(PaymentStatus::Unknown(format!("HTTP {}", status_code)));
        }

        let paystack_status = body["data"]["status"]
            .as_str()
            .unwrap_or("unknown");

        let mapped = match paystack_status {
            "success" => PaymentStatus::Succeeded,
            "failed" => PaymentStatus::Failed,
            "pending" | "abandoned" => PaymentStatus::Pending,
            "reversed" => PaymentStatus::Refunded,
            _ => PaymentStatus::Unknown(paystack_status.to_string()),
        };

        debug!(
            reference = %provider_payment_id,
            status = ?mapped,
            "Paystack transaction status"
        );

        Ok(mapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paystack_provider_name() {
        let provider = PaystackProvider::new("sk_test_key".into());
        assert_eq!(provider.provider_name(), "paystack");
    }

    #[test]
    fn test_map_event_type() {
        assert_eq!(
            PaystackProvider::map_event_type("charge.success"),
            WebhookEventType::PaymentSucceeded
        );
        assert_eq!(
            PaystackProvider::map_event_type("charge.failed"),
            WebhookEventType::PaymentFailed
        );
        assert_eq!(
            PaystackProvider::map_event_type("charge.refunded"),
            WebhookEventType::PaymentRefunded
        );
        assert_eq!(
            PaystackProvider::map_event_type("subscription.create"),
            WebhookEventType::SubscriptionCreated
        );
        assert_eq!(
            PaystackProvider::map_event_type("subscription.disable"),
            WebhookEventType::SubscriptionCanceled
        );
        assert_eq!(
            PaystackProvider::map_event_type("custom.event"),
            WebhookEventType::Unknown
        );
    }

    #[test]
    fn test_parse_webhook_event() {
        let provider = PaystackProvider::new("sk_test".into());
        let payload = r#"{
            "event": "charge.success",
            "data": {
                "id": 123456789,
                "reference": "ref_abc123",
                "amount": 50000,
                "currency": "NGN",
                "status": "success",
                "metadata": {
                    "invoice_id": "inv_001"
                }
            }
        }"#;

        let event = provider.parse_webhook_event(payload.as_bytes()).unwrap();
        assert_eq!(event.event_type, WebhookEventType::PaymentSucceeded);
        assert_eq!(event.provider_payment_id, Some("123456789".to_string()));
        assert_eq!(event.amount_cents, Some(50000));
        assert_eq!(event.invoice_id, Some("inv_001".to_string()));
    }
}
