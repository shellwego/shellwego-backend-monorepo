//! Mercado Pago payment provider implementation.
//!
//! Supports payments across Latin America via Mercado Pago's API,
//! including PIX (instant QR payments in Brazil), card payments,
//! refunds, webhook verification (x-signature header), and status polling.

use std::collections::HashMap;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{debug, info, warn};

use super::*;
use shellwego_schema::billing::MercadoPagoConfig;

/// Mercado Pago base URL.
const MERCADOPAGO_BASE_URL: &str = "https://api.mercadopago.com/v1";

/// Maximum allowed age for a webhook timestamp (5 minutes).
const WEBHOOK_TOLERANCE_SECS: i64 = 300;

/// Mercado Pago payment provider.
pub struct MercadoPagoProvider {
    /// Mercado Pago configuration.
    config: MercadoPagoConfig,
    /// Reusable HTTP client.
    http_client: reqwest::Client,
}

impl MercadoPagoProvider {
    /// Create a new Mercado Pago provider.
    pub fn new(config: MercadoPagoConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build Mercado Pago HTTP client"),
        }
    }

    // ----------------------------------------------------------------
    // Helpers
    // ----------------------------------------------------------------

    /// Build an authenticated request with Bearer token.
    fn authenticated(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url)
            .bearer_auth(&self.config.access_token)
    }

    /// Parse the Mercado Pago `x-signature` header into `(timestamp, v1_signature)`.
    fn parse_x_signature(header: &str) -> Result<(i64, String), BillingError> {
        let mut timestamp: Option<i64> = None;
        let mut v1: Option<String> = None;

        for part in header.split(',') {
            let part = part.trim();
            if let Some(ts) = part.strip_prefix("ts=") {
                timestamp = Some(ts.parse::<i64>().map_err(|_| {
                    BillingError::WebhookVerificationError(
                        "Invalid timestamp in Mercado Pago x-signature".to_string(),
                    )
                })?);
            } else if let Some(sig) = part.strip_prefix("v1=") {
                v1 = Some(sig.to_string());
            }
        }

        let timestamp =
            timestamp.ok_or_else(|| BillingError::WebhookVerificationError(
                "Missing ts= in Mercado Pago x-signature".to_string(),
            ))?;
        let v1 = v1.ok_or_else(|| BillingError::WebhookVerificationError(
            "Missing v1= in Mercado Pago x-signature".to_string(),
        ))?;

        Ok((timestamp, v1))
    }

    /// Map a Mercado Pago payment status to a normalized [`PaymentStatus`].
    fn map_mp_status(status: &str) -> PaymentStatus {
        match status {
            "approved" => PaymentStatus::Succeeded,
            "pending" | "in_process" | "in_mediation" => PaymentStatus::Pending,
            "rejected" | "cancelled" => PaymentStatus::Failed,
            "refunded" => PaymentStatus::Refunded,
            "partially_refunded" => PaymentStatus::PartiallyRefunded,
            "charged_back" => PaymentStatus::Disputed,
            _ => PaymentStatus::Unknown(status.to_string()),
        }
    }
}

#[async_trait]
impl PaymentProvider for MercadoPagoProvider {
    fn provider_name(&self) -> &str {
        "mercadopago"
    }

    // ----------------------------------------------------------------
    // Charge (Create Payment)
    // ----------------------------------------------------------------

    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError> {
        info!(
            invoice_id = %request.invoice_id,
            amount = request.amount_cents,
            currency = %request.currency,
            "Creating Mercado Pago payment"
        );

        // Mercado Pago amounts are in cents for most currencies,
        // but CLP and some others use whole units.
        // We convert cents to the appropriate format.
        let amount = request.amount_cents as f64 / 100.0;

        let mut metadata_map = serde_json::Map::new();
        metadata_map.insert(
            "invoice_id".to_string(),
            serde_json::Value::String(request.invoice_id.clone()),
        );
        metadata_map.insert(
            "customer_id".to_string(),
            serde_json::Value::String(request.customer_id.clone()),
        );
        for (k, v) in &request.metadata {
            metadata_map.insert(k.clone(), serde_json::Value::String(v.clone()));
        }

        let mut body = serde_json::json!({
            "transaction_amount": amount,
            "description": request.description,
            "external_reference": request.invoice_id,
            "metadata": metadata_map,
        });

        // Payment method: if PIX, use specific fields
        let payment_method_id = request
            .metadata
            .get("payment_method_id")
            .cloned()
            .unwrap_or_else(|| "pix".to_string());
        body["payment_method_id"] = serde_json::json!(payment_method_id);

        // Payer info
        if let Some(email) = request.metadata.get("payer_email") {
            body["payer"] = serde_json::json!({
                "email": email,
            });
        } else if let Some(email) = request.metadata.get("email") {
            body["payer"] = serde_json::json!({
                "email": email,
            });
        }

        // Set payer identification for Brazilian PIX
        if let Some(doc_number) = request.metadata.get("payer_doc_number") {
            let doc_type = request
                .metadata
                .get("payer_doc_type")
                .cloned()
                .unwrap_or_else(|| "CPF".to_string());
            body["payer"]["identification"] = serde_json::json!({
                "type": doc_type,
                "number": doc_number,
            });
        }

        let resp = self
            .authenticated(
                reqwest::Method::POST,
                &format!("{}/payments", MERCADOPAGO_BASE_URL),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Mercado Pago charge request failed: {}",
                e
            )))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse Mercado Pago response: {}",
                e
            )))?;

        let mp_status = resp_body["status"]
            .as_str()
            .unwrap_or("unknown");

        if !status.is_success() || mp_status == "rejected" || mp_status == "error" {
            let message = resp_body["message"]
                .as_str()
                .or_else(|| resp_body["error"].as_str())
                .or_else(|| resp_body["status_details"]
                    .as_str()
                    .filter(|_| mp_status == "rejected"))
                .unwrap_or("Unknown Mercado Pago error");
            warn!(
                error = message,
                mp_status = mp_status,
                "Mercado Pago charge failed"
            );
            return Ok(PaymentResult {
                success: false,
                transaction_id: resp_body["id"]
                    .as_i64()
                    .map(|id| id.to_string()),
                message: message.to_string(),
            });
        }

        let payment_id = resp_body["id"]
            .as_i64()
            .map(|id| id.to_string())
            .unwrap_or_default();

        let mut message = format!("Payment status: {}", mp_status);

        // Include PIX QR code data if available
        if let Some(point_of_interaction) = resp_body["point_of_interaction"].as_object() {
            if let Some(transaction_data) =
                point_of_interaction.get("transaction_data")
            {
                if let Some(qr_code) = transaction_data["qr_code"].as_str() {
                    message = format!(
                        "PIX QR code generated. Payment ID: {}. QR: {}",
                        payment_id, qr_code
                    );
                } else if let Some(ticket_url) = transaction_data["ticket_url"].as_str() {
                    message = format!(
                        "Payment link generated. ID: {}. URL: {}",
                        payment_id, ticket_url
                    );
                }
            }
        }

        debug!(
            payment_id = %payment_id,
            status = mp_status,
            "Mercado Pago payment created"
        );

        // Mercado Pago payments may be pending (e.g., PIX QR code generated)
        // We return the payment ID for status polling
        let success = mp_status == "approved";
        Ok(PaymentResult {
            success,
            transaction_id: Some(payment_id),
            message,
        })
    }

    // ----------------------------------------------------------------
    // Refund
    // ----------------------------------------------------------------

    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError> {
        info!(
            transaction_id = %request.original_transaction_id,
            "Creating Mercado Pago refund"
        );

        let url = format!(
            "{}/payments/{}/refunds",
            MERCADOPAGO_BASE_URL, request.original_transaction_id
        );

        let mut body = serde_json::json!({});

        if let Some(amount) = request.amount_cents {
            // Convert cents to whole currency
            body["amount"] = serde_json::json!(amount as f64 / 100.0);
        }

        let resp = self
            .authenticated(reqwest::Method::POST, &url)
            .json(&body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Mercado Pago refund request failed: {}",
                e
            )))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse Mercado Pago refund response: {}",
                e
            )))?;

        if !status.is_success() {
            let message = resp_body["message"]
                .as_str()
                .unwrap_or("Unknown Mercado Pago refund error");
            warn!(error = message, "Mercado Pago refund failed");
            return Ok(RefundResult {
                refund_id: String::new(),
                status: RefundStatus::Failed,
                amount_cents_refunded: 0,
                message: message.to_string(),
            });
        }

        let refund_id = resp_body["id"]
            .as_i64()
            .map(|id| id.to_string())
            .unwrap_or_default();
        let amount = resp_body["amount"]
            .as_f64()
            .map(|a| (a * 100.0) as i64)
            .unwrap_or(0);
        let refund_status = resp_body["status"]
            .as_str()
            .unwrap_or("failed");

        let mapped_status = match refund_status {
            "approved" => RefundStatus::Succeeded,
            "pending" | "in_process" => RefundStatus::Pending,
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
    // Webhook verification (x-signature header)
    // ----------------------------------------------------------------

    fn verify_webhook(&self, payload: &[u8], signature: &str) -> Result<bool, BillingError> {
        // Mercado Pago x-signature format: "ts=...,v1=..."
        // HMAC-SHA256(secret, "id={data.id};ts={timestamp};...")
        // But the simplified version uses: HMAC-SHA256(secret, "ts={ts}.jsonBody={payload}")

        let (timestamp, v1_signature) = Self::parse_x_signature(signature)?;

        // Check timestamp tolerance
        let now = chrono::Utc::now().timestamp();
        if (now - timestamp).abs() > WEBHOOK_TOLERANCE_SECS {
            return Err(BillingError::WebhookVerificationError(format!(
                "Mercado Pago webhook timestamp {} is outside tolerance",
                timestamp
            )));
        }

        // Extract data.id from payload for the signed string
        let body: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::json!({}));
        let data_id = body["data"]["id"]
            .as_i64()
            .map(|id| id.to_string())
            .or_else(|| body["data"]["id"].as_str().map(String::from))
            .unwrap_or_default();

        // Build the signed string
        let signed_string = format!(
            "id={};ts={}",
            data_id, timestamp
        );

        let mut mac = Hmac::<Sha256>::new_from_slice(
            self.config.webhook_secret.as_bytes(),
        )
        .map_err(|e| BillingError::WebhookVerificationError(format!(
            "Failed to create HMAC: {}",
            e
        )))?;
        mac.update(signed_string.as_bytes());
        let result = mac.finalize();
        let expected = hex::encode(result.into_bytes());

        let matches = super::constant_time_compare(
            expected.as_bytes(),
            v1_signature.as_bytes(),
        );

        debug!(verified = matches, "Mercado Pago webhook verification");
        Ok(matches)
    }

    // ----------------------------------------------------------------
    // Webhook parsing
    // ----------------------------------------------------------------

    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError> {
        let body: serde_json::Value = serde_json::from_slice(payload)?;

        let action = body["action"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let data_id = body["data"]["id"]
            .as_i64()
            .map(|id| id.to_string())
            .or_else(|| body["data"]["id"].as_str().map(String::from));

        // Mercado Pago webhooks mostly send "payment.updated" or "payment.created"
        // We need to inspect the payment status from the data payload.
        // The webhook includes a partial payment object.
        let mp_status = body["data"]["status"]
            .as_str()
            .unwrap_or("unknown");

        let event_type = match (action.as_str(), mp_status) {
            ("payment.updated", "approved") => WebhookEventType::PaymentSucceeded,
            ("payment.updated", "rejected") | ("payment.updated", "cancelled") => {
                WebhookEventType::PaymentFailed
            }
            ("payment.updated", "refunded") => WebhookEventType::PaymentRefunded,
            ("payment.updated", "partially_refunded") => WebhookEventType::PaymentPartiallyRefunded,
            ("payment.created", _) => WebhookEventType::PaymentPending,
            ("chargebacks.created", _) => WebhookEventType::DisputeCreated,
            ("chargebacks.won", _) => WebhookEventType::DisputeWon,
            ("chargebacks.lost", _) => WebhookEventType::DisputeLost,
            _ => WebhookEventType::Unknown,
        };

        let amount_cents = body["data"]["transaction_amount"]
            .as_f64()
            .map(|a| (a * 100.0) as i64);
        let currency = body["data"]["currency_id"]
            .as_str()
            .map(String::from);
        let external_reference = body["data"]["external_reference"]
            .as_str()
            .map(String::from); // This is our invoice_id
        let failure_reason = body["data"]["status_detail"]
            .as_str()
            .map(String::from);

        let metadata = body["data"]["metadata"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();

        debug!(
            event_type = ?event_type,
            action = %action,
            mp_status = mp_status,
            "Parsed Mercado Pago webhook event"
        );

        Ok(ParsedWebhookEvent {
            event_type,
            provider_event_type: format!("mercadopago.{}", action),
            invoice_id: external_reference,
            provider_payment_id: data_id,
            transaction_id: None,
            amount_cents,
            currency,
            failure_reason,
            customer_id: None,
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
            "{}/payments/{}",
            MERCADOPAGO_BASE_URL, provider_payment_id
        );

        let resp = self
            .authenticated(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Mercado Pago status check failed: {}",
                e
            )))?;

        let status_code = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse Mercado Pago status response: {}",
                e
            )))?;

        if !status_code.is_success() {
            return Ok(PaymentStatus::Unknown(format!("HTTP {}", status_code)));
        }

        let mp_status = body["status"].as_str().unwrap_or("unknown");
        let mapped = Self::map_mp_status(mp_status);

        debug!(
            payment_id = %provider_payment_id,
            mp_status = mp_status,
            status = ?mapped,
            "Mercado Pago payment status"
        );

        Ok(mapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> MercadoPagoConfig {
        MercadoPagoConfig {
            access_token: "test_access_token".to_string(),
            webhook_secret: "test_webhook_secret".to_string(),
        }
    }

    #[test]
    fn test_mercadopago_provider_name() {
        let provider = MercadoPagoProvider::new(make_config());
        assert_eq!(provider.provider_name(), "mercadopago");
    }

    #[test]
    fn test_parse_x_signature() {
        let header = "ts=1617932400,v1=abc123def456";
        let result = MercadoPagoProvider::parse_x_signature(header);
        assert!(result.is_ok());
        let (ts, v1) = result.unwrap();
        assert_eq!(ts, 1_617_932_400);
        assert_eq!(v1, "abc123def456");
    }

    #[test]
    fn test_parse_x_signature_missing_ts() {
        let header = "v1=abc123";
        let result = MercadoPagoProvider::parse_x_signature(header);
        assert!(result.is_err());
    }

    #[test]
    fn test_map_mp_status() {
        assert_eq!(
            MercadoPagoProvider::map_mp_status("approved"),
            PaymentStatus::Succeeded
        );
        assert_eq!(
            MercadoPagoProvider::map_mp_status("pending"),
            PaymentStatus::Pending
        );
        assert_eq!(
            MercadoPagoProvider::map_mp_status("in_process"),
            PaymentStatus::Pending
        );
        assert_eq!(
            MercadoPagoProvider::map_mp_status("rejected"),
            PaymentStatus::Failed
        );
        assert_eq!(
            MercadoPagoProvider::map_mp_status("refunded"),
            PaymentStatus::Refunded
        );
        assert_eq!(
            MercadoPagoProvider::map_mp_status("partially_refunded"),
            PaymentStatus::PartiallyRefunded
        );
        assert_eq!(
            MercadoPagoProvider::map_mp_status("charged_back"),
            PaymentStatus::Disputed
        );
    }

    #[test]
    fn test_parse_webhook_event_approved() {
        let provider = MercadoPagoProvider::new(make_config());
        let payload = r#"{
            "action": "payment.updated",
            "data": {
                "id": "123456789",
                "status": "approved",
                "transaction_amount": 100.50,
                "currency_id": "BRL",
                "external_reference": "inv_001",
                "metadata": {
                    "customer_id": "cust_001"
                }
            }
        }"#;

        let event = provider.parse_webhook_event(payload.as_bytes()).unwrap();
        assert_eq!(event.event_type, WebhookEventType::PaymentSucceeded);
        assert_eq!(event.amount_cents, Some(10050));
        assert_eq!(event.invoice_id, Some("inv_001".to_string()));
    }

    #[test]
    fn test_parse_webhook_event_rejected() {
        let provider = MercadoPagoProvider::new(make_config());
        let payload = r#"{
            "action": "payment.updated",
            "data": {
                "id": "123456789",
                "status": "rejected",
                "status_detail": "cc_rejected_other_reason",
                "transaction_amount": 50.00,
                "currency_id": "BRL"
            }
        }"#;

        let event = provider.parse_webhook_event(payload.as_bytes()).unwrap();
        assert_eq!(event.event_type, WebhookEventType::PaymentFailed);
        assert_eq!(
            event.failure_reason,
            Some("cc_rejected_other_reason".to_string())
        );
    }
}
