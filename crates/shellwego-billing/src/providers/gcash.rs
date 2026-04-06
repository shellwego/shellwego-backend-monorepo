//! GCash payment provider implementation via PayMongo.
//!
//! Supports GCash payments through PayMongo's source/payment flow,
//! refunds, webhook verification using HMAC-SHA256, and status polling.

use std::collections::HashMap;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{debug, info, warn};

use super::*;
use shellwego_schema::billing::GcashConfig;

/// PayMongo base URL.
const PAYMONGO_BASE_URL: &str = "https://api.paymongo.com/v1";

/// GCash payment provider using PayMongo API.
pub struct GcashProvider {
    /// PayMongo configuration.
    config: GcashConfig,
    /// Reusable HTTP client.
    http_client: reqwest::Client,
}

impl GcashProvider {
    /// Create a new GCash provider via PayMongo.
    pub fn new(config: GcashConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build GCash/PayMongo HTTP client"),
        }
    }

    // ----------------------------------------------------------------
    // Helpers
    // ----------------------------------------------------------------

    /// Build an authenticated request with PayMongo basic auth.
    fn authenticated(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url)
            .basic_auth(
                &self.config.paymongo_public_key,
                Some(&self.config.paymongo_secret_key),
            )
    }
}

#[async_trait]
impl PaymentProvider for GcashProvider {
    fn provider_name(&self) -> &str {
        "gcash"
    }

    // ----------------------------------------------------------------
    // Charge (Create Source → Create Payment)
    // ----------------------------------------------------------------

    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError> {
        info!(
            invoice_id = %request.invoice_id,
            amount = request.amount_cents,
            "Creating PayMongo GCash source and payment"
        );

        // PayMongo amounts are in cents (centavos for PHP).

        // Step 1: Create a GCash source (redirect-based)
        let source_body = serde_json::json!({
            "data": {
                "attributes": {
                    "type": "gcash",
                    "amount": request.amount_cents,
                    "currency": request.currency,
                    "redirect": {
                        "success": request.metadata.get("redirect_success")
                            .map(|s| s.as_str())
                            .unwrap_or("https://example.com/success"),
                        "failed": request.metadata.get("redirect_failed")
                            .map(|s| s.as_str())
                            .unwrap_or("https://example.com/failed"),
                    },
                    "billing": {
                        "name": request.metadata.get("customer_name")
                            .map(|s| s.as_str())
                            .unwrap_or("Customer"),
                        "email": request.metadata.get("email")
                            .map(|s| s.as_str())
                            .unwrap_or("customer@shellwego.com"),
                        "phone": request.metadata.get("phone")
                            .map(|s| s.as_str())
                            .unwrap_or("+63 917 000 0000"),
                    },
                    "metadata": {
                        "invoice_id": request.invoice_id,
                        "customer_id": request.customer_id,
                    }
                }
            }
        });

        let source_resp = self
            .authenticated(reqwest::Method::POST, &format!("{}/sources", PAYMONGO_BASE_URL))
            .json(&source_body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "PayMongo source creation failed: {}",
                e
            )))?;

        let source_status = source_resp.status();
        let source_body: serde_json::Value = source_resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse PayMongo source response: {}",
                e
            )))?;

        if !source_status.is_success() {
            let errors = source_body["errors"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e["detail"].as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_else(|| "Unknown PayMongo error".to_string());
            warn!(error = %errors, "PayMongo source creation failed");
            return Ok(PaymentResult {
                success: false,
                transaction_id: None,
                message: errors,
            });
        }

        let source_id = source_body["data"]["id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let redirect_url = source_body["data"]["attributes"]["redirect"]["checkout_url"]
            .as_str()
            .unwrap_or("")
            .to_string();

        debug!(source_id = %source_id, "GCash source created");

        // Step 2: Create a payment from the source
        let payment_body = serde_json::json!({
            "data": {
                "attributes": {
                    "amount": request.amount_cents,
                    "currency": request.currency,
                    "description": request.description,
                    "source": {
                        "id": source_id,
                        "type": "source"
                    },
                    "metadata": {
                        "invoice_id": request.invoice_id,
                        "customer_id": request.customer_id,
                    },
                    "statement_descriptor": &request.description[..request.description.len().min(22)],
                }
            }
        });

        let pay_resp = self
            .authenticated(
                reqwest::Method::POST,
                &format!("{}/payments", PAYMONGO_BASE_URL),
            )
            .json(&payment_body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "PayMongo payment creation failed: {}",
                e
            )))?;

        let pay_status = pay_resp.status();
        let pay_body: serde_json::Value = pay_resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse PayMongo payment response: {}",
                e
            )))?;

        if !pay_status.is_success() {
            let errors = pay_body["errors"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e["detail"].as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_else(|| "Unknown PayMongo error".to_string());
            warn!(error = %errors, "PayMongo payment creation failed");
            return Ok(PaymentResult {
                success: false,
                transaction_id: None,
                message: errors,
            });
        }

        let payment_id = pay_body["data"]["id"]
            .as_str()
            .unwrap_or("")
            .to_string();

        debug!(payment_id = %payment_id, "GCash payment created");

        // GCash is redirect-based: customer must visit the redirect URL.
        Ok(PaymentResult {
            success: true,
            transaction_id: Some(payment_id),
            message: format!("Redirect customer to GCash: {}", redirect_url),
        })
    }

    // ----------------------------------------------------------------
    // Refund
    // ----------------------------------------------------------------

    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError> {
        info!(
            transaction_id = %request.original_transaction_id,
            "Creating PayMongo refund"
        );

        let mut body = serde_json::json!({
            "data": {
                "attributes": {
                    "amount": request.amount_cents.unwrap_or(0),
                    "reason": request.reason,
                    "metadata": {}
                }
            }
        });

        // PayMongo returns full amount if amount is 0
        if request.amount_cents.is_none() || request.amount_cents == Some(0) {
            body["data"]["attributes"]["amount"] = serde_json::json!(null);
            body["data"]["attributes"]["full"] = serde_json::json!(true);
        }

        let url = format!(
            "{}/payments/{}/refunds",
            PAYMONGO_BASE_URL, request.original_transaction_id
        );

        let resp = self
            .authenticated(reqwest::Method::POST, &url)
            .json(&body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "PayMongo refund request failed: {}",
                e
            )))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse PayMongo refund response: {}",
                e
            )))?;

        if !status.is_success() {
            let errors = resp_body["errors"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e["detail"].as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_else(|| "Unknown PayMongo error".to_string());
            return Ok(RefundResult {
                refund_id: String::new(),
                status: RefundStatus::Failed,
                amount_cents_refunded: 0,
                message: errors,
            });
        }

        let refund_id = resp_body["data"]["id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let amount = resp_body["data"]["attributes"]["amount"]
            .as_i64()
            .unwrap_or(0);
        let refund_status = resp_body["data"]["attributes"]["status"]
            .as_str()
            .unwrap_or("failed");

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
    // Webhook verification (HMAC-SHA256)
    // ----------------------------------------------------------------

    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<bool, BillingError> {
        // PayMongo signs webhooks with HMAC-SHA256(webhook_secret, raw_body)
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

        debug!(verified = matches, "PayMongo webhook verification");
        Ok(matches)
    }

    // ----------------------------------------------------------------
    // Webhook parsing
    // ----------------------------------------------------------------

    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError> {
        let body: serde_json::Value = serde_json::from_slice(payload)?;

        let paymongo_type = body["data"]["type"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let attributes = &body["data"]["attributes"];

        // Map PayMongo payment status
        let status = attributes["status"]
            .as_str()
            .unwrap_or("unknown");

        let event_type = match status {
            "paid" => WebhookEventType::PaymentSucceeded,
            "failed" => WebhookEventType::PaymentFailed,
            "refunded" => WebhookEventType::PaymentRefunded,
            "partially_refunded" => WebhookEventType::PaymentPartiallyRefunded,
            _ => WebhookEventType::Unknown,
        };

        let provider_payment_id = body["data"]["id"]
            .as_str()
            .map(String::from);
        let amount_cents = attributes["amount"].as_i64();
        let currency = attributes["currency"].as_str().map(String::from);
        let invoice_id = attributes["metadata"]["invoice_id"]
            .as_str()
            .map(String::from);
        let customer_id = attributes["metadata"]["customer_id"]
            .as_str()
            .map(String::from);
        let failure_reason = attributes["failure_message"]
            .as_str()
            .map(String::from);

        let metadata = attributes["metadata"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();

        debug!(
            event_type = ?event_type,
            paymongo_type = %paymongo_type,
            "Parsed PayMongo webhook event"
        );

        Ok(ParsedWebhookEvent {
            event_type,
            provider_event_type: format!("paymongo.{}", paymongo_type),
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
    // Payment status polling
    // ----------------------------------------------------------------

    async fn check_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<PaymentStatus, BillingError> {
        let url = format!("{}/payments/{}", PAYMONGO_BASE_URL, provider_payment_id);

        let resp = self
            .authenticated(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "PayMongo status check failed: {}",
                e
            )))?;

        let status_code = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse PayMongo status response: {}",
                e
            )))?;

        if !status_code.is_success() {
            return Ok(PaymentStatus::Unknown(format!("HTTP {}", status_code)));
        }

        let paymongo_status = body["data"]["attributes"]["status"]
            .as_str()
            .unwrap_or("unknown");

        let mapped = match paymongo_status {
            "paid" => PaymentStatus::Succeeded,
            "pending" => PaymentStatus::Pending,
            "failed" => PaymentStatus::Failed,
            "refunded" => PaymentStatus::Refunded,
            "partially_refunded" => PaymentStatus::PartiallyRefunded,
            _ => PaymentStatus::Unknown(paymongo_status.to_string()),
        };

        debug!(
            payment_id = %provider_payment_id,
            status = ?mapped,
            "PayMongo payment status"
        );

        Ok(mapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> GcashConfig {
        GcashConfig {
            paymongo_public_key: "pk_test_key".to_string(),
            paymongo_secret_key: "sk_test_key".to_string(),
            webhook_secret: "whsecret_test".to_string(),
        }
    }

    #[test]
    fn test_gcash_provider_name() {
        let provider = GcashProvider::new(make_config());
        assert_eq!(provider.provider_name(), "gcash");
    }

    #[test]
    fn test_parse_webhook_event_paid() {
        let provider = GcashProvider::new(make_config());
        let payload = r#"{
            "data": {
                "id": "pay_abc123",
                "type": "payment",
                "attributes": {
                    "status": "paid",
                    "amount": 50000,
                    "currency": "PHP",
                    "metadata": {
                        "invoice_id": "inv_001",
                        "customer_id": "cust_001"
                    }
                }
            }
        }"#;

        let event = provider.parse_webhook_event(payload.as_bytes()).unwrap();
        assert_eq!(event.event_type, WebhookEventType::PaymentSucceeded);
        assert_eq!(event.amount_cents, Some(50000));
        assert_eq!(event.invoice_id, Some("inv_001".to_string()));
    }

    #[test]
    fn test_parse_webhook_event_failed() {
        let provider = GcashProvider::new(make_config());
        let payload = r#"{
            "data": {
                "id": "pay_fail_123",
                "type": "payment",
                "attributes": {
                    "status": "failed",
                    "amount": 50000,
                    "currency": "PHP",
                    "failure_message": "Payment was refused"
                }
            }
        }"#;

        let event = provider.parse_webhook_event(payload.as_bytes()).unwrap();
        assert_eq!(event.event_type, WebhookEventType::PaymentFailed);
        assert_eq!(
            event.failure_reason,
            Some("Payment was refused".to_string())
        );
    }
}
