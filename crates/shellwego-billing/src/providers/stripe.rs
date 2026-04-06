//! Stripe payment provider implementation.
//!
//! Implements charge creation via Payment Intents, refunds, webhook verification
//! using HMAC-SHA256 with timestamp tolerance, and payment status polling.

use std::collections::HashMap;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{debug, info, warn};

use super::*;

/// Maximum allowed age for a webhook timestamp (5 minutes).
const WEBHOOK_TOLERANCE_SECS: i64 = 300;

/// Stripe payment provider.
pub struct StripeProvider {
    /// Stripe secret API key (sk_live_... or sk_test_...)
    api_key: String,
    /// Webhook signing secret (whsec_...). Optional – webhooks will fail
    /// verification when this is `None`.
    webhook_secret: Option<String>,
    /// Reusable HTTP client.
    http_client: reqwest::Client,
}

impl StripeProvider {
    /// Create a new Stripe provider.
    ///
    /// * `api_key` – Stripe secret key (starts with `sk_`).
    /// * `webhook_secret` – Optional webhook endpoint secret (starts with `whsec_`).
    pub fn new(api_key: String, webhook_secret: Option<String>) -> Self {
        Self {
            api_key,
            webhook_secret,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build Stripe HTTP client"),
        }
    }

    // ----------------------------------------------------------------
    // Helpers
    // ----------------------------------------------------------------

    /// Build an authenticated request with Stripe basic auth.
    fn authenticated(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url)
            .basic_auth(&self.api_key, None::<&str>)
    }

    /// Parse the Stripe `Stripe-Signature` header into `(timestamp, v1_signature)`.
    fn parse_signature_header(header: &str) -> Result<(i64, String), BillingError> {
        let mut timestamp: Option<i64> = None;
        let mut v1: Option<String> = None;

        for part in header.split(',') {
            let part = part.trim();
            if let Some(ts) = part.strip_prefix("t=") {
                timestamp = Some(ts.parse::<i64>().map_err(|_| {
                    BillingError::WebhookVerificationError(
                        "Invalid timestamp in Stripe signature header".to_string(),
                    )
                })?);
            } else if let Some(sig) = part.strip_prefix("v1=") {
                v1 = Some(sig.to_string());
            }
        }

        let timestamp =
            timestamp.ok_or_else(|| BillingError::WebhookVerificationError(
                "Missing t= in Stripe signature header".to_string(),
            ))?;
        let v1 = v1.ok_or_else(|| BillingError::WebhookVerificationError(
            "Missing v1= in Stripe signature header".to_string(),
        ))?;

        Ok((timestamp, v1))
    }

    /// Map a Stripe event type string to a normalized [`WebhookEventType`].
    fn map_event_type(stripe_type: &str) -> WebhookEventType {
        match stripe_type {
            "payment_intent.succeeded" => WebhookEventType::PaymentSucceeded,
            "payment_intent.payment_failed" => WebhookEventType::PaymentFailed,
            "payment_intent.processing" => WebhookEventType::PaymentPending,
            "charge.refunded" => WebhookEventType::PaymentRefunded,
            "charge.refund.updated" => WebhookEventType::PaymentRefunded,
            "charge.dispute.created" => WebhookEventType::DisputeCreated,
            "charge.dispute.won" => WebhookEventType::DisputeWon,
            "charge.dispute.closed" | "charge.dispute.lost" => WebhookEventType::DisputeLost,
            "customer.subscription.created" => WebhookEventType::SubscriptionCreated,
            "customer.subscription.updated" => WebhookEventType::SubscriptionUpdated,
            "customer.subscription.deleted" => WebhookEventType::SubscriptionCanceled,
            "invoice.payment_succeeded" => WebhookEventType::SubscriptionRenewed,
            _ => WebhookEventType::Unknown,
        }
    }
}

#[async_trait]
impl PaymentProvider for StripeProvider {
    fn provider_name(&self) -> &str {
        "stripe"
    }

    // ----------------------------------------------------------------
    // Charge
    // ----------------------------------------------------------------

    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError> {
        info!(
            invoice_id = %request.invoice_id,
            amount = request.amount_cents,
            currency = %request.currency,
            "Creating Stripe payment intent"
        );

        let amount = request.amount_cents; // Stripe uses cents already

        let mut form_body = HashMap::new();
        form_body.insert("amount", amount.to_string());
        form_body.insert("currency", request.currency.to_lowercase());
        form_body.insert("confirm", "true".to_string());
        form_body.insert("payment_method_data[type]", "card".to_string());
        form_body.insert(
            "payment_method_data[card][token]",
            request.payment_token.clone(),
        );
        form_body.insert("description", request.description);
        form_body.insert("metadata[invoice_id]", request.invoice_id.clone());
        form_body.insert("metadata[customer_id]", request.customer_id.clone());
        if let Some(ref idem) = request.idempotency_key {
            form_body.insert("idempotency_key", idem.clone());
        }

        // Include any extra metadata from the request
        for (k, v) in &request.metadata {
            form_body.insert(format!("metadata[{}]", k), v.clone());
        }

        let resp = self
            .authenticated(reqwest::Method::POST, "https://api.stripe.com/v1/payment_intents")
            .form(&form_body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!("Stripe charge request failed: {}", e)))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!("Failed to parse Stripe response: {}", e)))?;

        if !status.is_success() {
            let message = body["error"]["message"]
                .as_str()
                .unwrap_or("Unknown Stripe error")
                .to_string();
            warn!(error = %message, "Stripe charge failed");
            return Ok(PaymentResult {
                success: false,
                transaction_id: body["id"].as_str().map(String::from),
                message,
            });
        }

        let succeeded = body["status"].as_str() == Some("succeeded");
        let txn_id = body["id"].as_str().unwrap_or("").to_string();

        debug!(
            payment_intent_id = %txn_id,
            succeeded,
            "Stripe charge response"
        );

        Ok(PaymentResult {
            success: succeeded,
            transaction_id: Some(txn_id),
            message: if succeeded {
                "Payment succeeded".to_string()
            } else {
                body["status"].as_str().unwrap_or("unknown").to_string()
            },
        })
    }

    // ----------------------------------------------------------------
    // Refund
    // ----------------------------------------------------------------

    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError> {
        info!(
            transaction_id = %request.original_transaction_id,
            "Creating Stripe refund"
        );

        let mut form_body = HashMap::new();
        form_body.insert("payment_intent", request.original_transaction_id.clone());
        if let Some(amount) = request.amount_cents {
            form_body.insert("amount", amount.to_string());
        }
        form_body.insert("reason", request.reason.clone());
        if let Some(ref idem) = request.idempotency_key {
            form_body.insert("idempotency_key", idem.clone());
        }

        let resp = self
            .authenticated(reqwest::Method::POST, "https://api.stripe.com/v1/refunds")
            .form(&form_body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!("Stripe refund request failed: {}", e)))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!("Failed to parse Stripe refund response: {}", e)))?;

        if !status.is_success() {
            let message = body["error"]["message"]
                .as_str()
                .unwrap_or("Unknown Stripe refund error")
                .to_string();
            warn!(error = %message, "Stripe refund failed");
            return Ok(RefundResult {
                refund_id: String::new(),
                status: RefundStatus::Failed,
                amount_cents_refunded: 0,
                message,
            });
        }

        let refund_id = body["id"].as_str().unwrap_or("").to_string();
        let refund_status = body["status"].as_str().unwrap_or("");
        let amount = body["amount"].as_i64().unwrap_or(0);

        let mapped_status = match refund_status {
            "pending" => RefundStatus::Pending,
            "succeeded" => RefundStatus::Succeeded,
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
    // Webhook verification
    // ----------------------------------------------------------------

    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<bool, BillingError> {
        let secret = match &self.webhook_secret {
            Some(s) => s,
            None => {
                return Err(BillingError::WebhookVerificationError(
                    "Stripe webhook secret not configured".to_string(),
                ));
            }
        };

        // 1. Parse signature header
        let (timestamp, v1_signature) = Self::parse_signature_header(signature)?;

        // 2. Check timestamp tolerance (reject events older than 5 minutes)
        let now = chrono::Utc::now().timestamp();
        if (now - timestamp).abs() > WEBHOOK_TOLERANCE_SECS {
            return Err(BillingError::WebhookVerificationError(format!(
                "Stripe webhook timestamp {} is outside tolerance window",
                timestamp
            )));
        }

        // 3. Compute HMAC-SHA256(secret, "{timestamp}.{payload}")
        let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|e| {
                BillingError::WebhookVerificationError(format!(
                    "Failed to create HMAC: {}",
                    e
                ))
            })?;
        mac.update(signed_payload.as_bytes());
        let result = mac.finalize();
        let expected = STANDARD.encode(result.into_bytes());

        // 4. Constant-time compare
        let matches = super::constant_time_compare(
            expected.as_bytes(),
            v1_signature.as_bytes(),
        );

        debug!(verified = matches, "Stripe webhook verification");
        Ok(matches)
    }

    // ----------------------------------------------------------------
    // Webhook parsing
    // ----------------------------------------------------------------

    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError> {
        let body: serde_json::Value = serde_json::from_slice(payload)?;

        let stripe_type = body["type"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let event_type = Self::map_event_type(&stripe_type);

        let obj = &body["data"]["object"];

        // Extract common fields from the object
        let provider_payment_id = obj["id"].as_str().map(String::from);
        let amount_cents = obj["amount"].as_i64();
        let currency = obj["currency"].as_str().map(String::from);
        let invoice_id = obj["metadata"]["invoice_id"].as_str().map(String::from);
        let transaction_id = obj["metadata"]["transaction_id"].as_str().map(String::from);
        let customer_id = obj["customer"].as_str().map(String::from);

        // Failure reason for payment failures
        let failure_reason = obj["last_payment_error"]["message"]
            .as_str()
            .map(String::from);

        // Collect all metadata into a generic map
        let metadata = obj["metadata"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();

        debug!(
            event_type = ?event_type,
            provider_event_type = %stripe_type,
            "Parsed Stripe webhook event"
        );

        Ok(ParsedWebhookEvent {
            event_type,
            provider_event_type: stripe_type,
            invoice_id,
            provider_payment_id,
            transaction_id,
            amount_cents,
            currency,
            failure_reason,
            customer_id,
            metadata,
        })
    }

    // ----------------------------------------------------------------
    // Payment status polling
    // ----------------------------------------------------------------

    async fn check_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<PaymentStatus, BillingError> {
        let url = format!(
            "https://api.stripe.com/v1/payment_intents/{}",
            provider_payment_id
        );

        let resp = self
            .authenticated(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Stripe status check failed: {}",
                e
            )))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse Stripe status response: {}",
                e
            )))?;

        if !status.is_success() {
            return Ok(PaymentStatus::Unknown(format!(
                "HTTP {}",
                status
            )));
        }

        let stripe_status = body["status"].as_str().unwrap_or("unknown");
        let mapped = match stripe_status {
            "succeeded" => PaymentStatus::Succeeded,
            "processing" => PaymentStatus::Pending,
            "requires_payment_method" | "requires_confirmation" | "requires_action"
            | "requires_capture" => PaymentStatus::Pending,
            "canceled" => PaymentStatus::Failed,
            _ => PaymentStatus::Unknown(stripe_status.to_string()),
        };

        debug!(
            payment_intent = %provider_payment_id,
            status = ?mapped,
            "Stripe payment status check"
        );

        Ok(mapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stripe_provider_name() {
        let provider = StripeProvider::new("sk_test_key".into(), None);
        assert_eq!(provider.provider_name(), "stripe");
    }

    #[test]
    fn test_parse_signature_header_valid() {
        let header = "t=1617932400,v1=abc123def456";
        let result = StripeProvider::parse_signature_header(header);
        assert!(result.is_ok());
        let (ts, v1) = result.unwrap();
        assert_eq!(ts, 1_617_932_400);
        assert_eq!(v1, "abc123def456");
    }

    #[test]
    fn test_parse_signature_header_missing_timestamp() {
        let header = "v1=abc123";
        let result = StripeProvider::parse_signature_header(header);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_signature_header_missing_v1() {
        let header = "t=1617932400";
        let result = StripeProvider::parse_signature_header(header);
        assert!(result.is_err());
    }

    #[test]
    fn test_map_event_type() {
        assert_eq!(
            StripeProvider::map_event_type("payment_intent.succeeded"),
            WebhookEventType::PaymentSucceeded
        );
        assert_eq!(
            StripeProvider::map_event_type("payment_intent.payment_failed"),
            WebhookEventType::PaymentFailed
        );
        assert_eq!(
            StripeProvider::map_event_type("payment_intent.processing"),
            WebhookEventType::PaymentPending
        );
        assert_eq!(
            StripeProvider::map_event_type("charge.refunded"),
            WebhookEventType::PaymentRefunded
        );
        assert_eq!(
            StripeProvider::map_event_type("charge.dispute.created"),
            WebhookEventType::DisputeCreated
        );
        assert_eq!(
            StripeProvider::map_event_type("charge.dispute.won"),
            WebhookEventType::DisputeWon
        );
        assert_eq!(
            StripeProvider::map_event_type("customer.subscription.created"),
            WebhookEventType::SubscriptionCreated
        );
        assert_eq!(
            StripeProvider::map_event_type("invoice.payment_succeeded"),
            WebhookEventType::SubscriptionRenewed
        );
        assert_eq!(
            StripeProvider::map_event_type("something.weird"),
            WebhookEventType::Unknown
        );
    }
}
