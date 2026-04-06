//! M-Pesa (Safaricom Daraja) payment provider implementation.
//!
//! Supports STK Push (Lipa Na M-Pesa Online) for initiating payments,
//! callback parsing, and payment status polling. M-Pesa does not natively
//! support webhook signatures or refunds, so those methods return
//! constrained results.

use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Timelike;
use tracing::{debug, info, warn};

use super::*;
use shellwego_schema::billing::MpesaConfig;

/// Base URL for M-Pesa sandbox.
const MPESA_SANDBOX_URL: &str = "https://sandbox.safaricom.co.ke";
/// Base URL for M-Pesa production.
const MPESA_PRODUCTION_URL: &str = "https://api.safaricom.co.ke";

/// M-Pesa payment provider using Safaricom Daraja API.
pub struct MpesaProvider {
    /// M-Pesa configuration.
    config: MpesaConfig,
    /// Base URL (sandbox or production).
    base_url: String,
    /// Reusable HTTP client.
    http_client: reqwest::Client,
    /// Cached OAuth token. M-Pesa tokens are valid for 1 hour.
    cached_token: RwLock<Option<(String, i64)>>,
}

impl MpesaProvider {
    /// Create a new M-Pesa provider from configuration.
    pub fn new(config: MpesaConfig) -> Self {
        let base_url = match config.environment {
            shellwego_schema::billing::MpesaEnvironment::Sandbox => MPESA_SANDBOX_URL.to_string(),
            shellwego_schema::billing::MpesaEnvironment::Production => {
                MPESA_PRODUCTION_URL.to_string()
            }
        };

        Self {
            config,
            base_url,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build M-Pesa HTTP client"),
            cached_token: RwLock::new(None),
        }
    }

    // ----------------------------------------------------------------
    // OAuth
    // ----------------------------------------------------------------

    /// Obtain or return cached OAuth access token.
    ///
    /// M-Pesa tokens are valid for 3600 seconds. This method caches the
    /// token and re-uses it until it expires.
    async fn get_access_token(&self) -> Result<String, BillingError> {
        // Check cache first
        {
            let cache = self.cached_token.read().unwrap();
            if let Some((ref token, expires_at)) = *cache {
                let now = chrono::Utc::now().timestamp();
                if now < expires_at {
                    return Ok(token.clone());
                }
            }
        }

        // Fetch new token
        let url = format!(
            "{}/oauth/v1/generate?grant_type=client_credentials",
            self.base_url
        );

        let resp = self
            .http_client
            .get(&url)
            .basic_auth(&self.config.consumer_key, Some(&self.config.consumer_secret))
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "M-Pesa OAuth request failed: {}",
                e
            )))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse M-Pesa OAuth response: {}",
                e
            )))?;

        let token = body["access_token"]
            .as_str()
            .ok_or_else(|| {
                BillingError::ProviderError(
                    "No access_token in M-Pesa OAuth response".to_string(),
                )
            })?
            .to_string();

        // Cache for 55 minutes (safe margin under 1-hour expiry)
        let expires_at = chrono::Utc::now().timestamp() + 3300;
        {
            let mut cache = self.cached_token.write().unwrap();
            *cache = Some((token.clone(), expires_at));
        }

        debug!("M-Pesa OAuth token obtained");
        Ok(token)
    }

    // ----------------------------------------------------------------
    // STK Push helpers
    // ----------------------------------------------------------------

    /// Generate the Lipa Na M-Pesa password:
    /// `base64(ShortCode + Passkey + Timestamp)`
    fn generate_password(&self) -> (String, String) {
        let now = chrono::Utc::now();
        let timestamp = format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}",
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        );
        let raw = format!(
            "{}{}{}",
            self.config.business_short_code, self.config.passkey, timestamp
        );
        let password = STANDARD.encode(raw);
        (password, timestamp)
    }
}

#[async_trait]
impl PaymentProvider for MpesaProvider {
    fn provider_name(&self) -> &str {
        "mpesa"
    }

    // ----------------------------------------------------------------
    // Charge (STK Push)
    // ----------------------------------------------------------------

    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError> {
        info!(
            invoice_id = %request.invoice_id,
            amount = request.amount_cents,
            "Initiating M-Pesa STK push"
        );

        let token = self.get_access_token().await?;
        let (password, timestamp) = self.generate_password();

        let url = format!("{}/mpesa/stkpush/v1/processrequest", self.base_url);

        // M-Pesa amount is in whole currency units (KES), but we receive cents.
        let amount = request.amount_cents / 100;

        // Phone number: extract from metadata["phone"] or use the payment_token
        let phone_number = request
            .metadata
            .get("phone")
            .cloned()
            .unwrap_or_else(|| {
                // Default: prepend 254 (Kenya) if not already prefixed
                let mut phone = request.payment_token.clone();
                if phone.starts_with("0") {
                    phone = format!("254{}", &phone[1..]);
                } else if !phone.starts_with("254") {
                    phone = format!("254{}", phone);
                }
                phone
            });

        let body = serde_json::json!({
            "BusinessShortCode": self.config.business_short_code,
            "Password": password,
            "Timestamp": timestamp,
            "TransactionType": "CustomerPayBillOnline",
            "Amount": amount,
            "PartyA": phone_number,
            "PartyB": self.config.business_short_code,
            "PhoneNumber": phone_number,
            "CallBackURL": self.config.callback_url,
            "AccountReference": &request.invoice_id[..request.invoice_id.len().min(12)],
            "TransactionDesc": &request.description[..request.description.len().min(13)],
        });

        let resp = self
            .http_client
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "M-Pesa STK push failed: {}",
                e
            )))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse M-Pesa STK push response: {}",
                e
            )))?;

        let response_code = body["ResponseCode"].as_str().unwrap_or("");
        let checkout_request_id = body["CheckoutRequestID"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let response_description = body["ResponseDescription"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();
        let merchant_request_id = body["MerchantRequestID"]
            .as_str()
            .unwrap_or("")
            .to_string();

        if response_code != "0" {
            warn!(
                code = response_code,
                description = %response_description,
                "M-Pesa STK push failed"
            );
            return Ok(PaymentResult {
                success: false,
                transaction_id: Some(checkout_request_id),
                message: format!("M-Pesa error {}: {}", response_code, response_description),
            });
        }

        debug!(
            checkout_request_id = %checkout_request_id,
            merchant_request_id = %merchant_request_id,
            "M-Pesa STK push initiated successfully"
        );

        // STK push is async – the customer will get a prompt on their phone.
        // We return the CheckoutRequestID as the transaction_id for later polling.
        Ok(PaymentResult {
            success: true,
            transaction_id: Some(checkout_request_id),
            message: "STK push sent. Waiting for customer to confirm on phone.".to_string(),
        })
    }

    // ----------------------------------------------------------------
    // Refund – not supported
    // ----------------------------------------------------------------

    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError> {
        warn!(
            transaction_id = %request.original_transaction_id,
            "M-Pesa does not support programmatic refunds"
        );
        Ok(RefundResult {
            refund_id: String::new(),
            status: RefundStatus::Failed,
            amount_cents_refunded: 0,
            message: "M-Pesa does not support programmatic refunds. Use the M-Pesa business portal or reverse via the Disbursement API (B2C).".to_string(),
        })
    }

    // ----------------------------------------------------------------
    // Webhook verification – no signatures
    // ----------------------------------------------------------------

    fn verify_webhook(&self, _payload: &[u8], _signature: &str) -> Result<bool, BillingError> {
        // M-Pesa C2B callbacks do not include cryptographic signatures.
        // Validation is done by ensuring the callback originates from
        // Safaricom IPs at the infrastructure level.
        Ok(true)
    }

    // ----------------------------------------------------------------
    // Webhook parsing
    // ----------------------------------------------------------------

    fn parse_webhook_event(&self, payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError> {
        let body: serde_json::Value = serde_json::from_slice(payload)?;

        let stk_callback = &body["Body"]["stkCallback"];
        let checkout_request_id = stk_callback["CheckoutRequestID"]
            .as_str()
            .map(String::from);
        let merchant_request_id = stk_callback["MerchantRequestID"]
            .as_str()
            .map(String::from);
        let result_code = stk_callback["ResultCode"].as_i64().unwrap_or(-1);
        let result_desc = stk_callback["ResultDesc"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();

        let event_type = match result_code {
            0 => {
                // Extract callback metadata
                let items = &stk_callback["CallbackMetadata"]["Item"];
                let amount = items
                    .as_array()
                    .and_then(|arr| {
                        arr.iter()
                            .find(|item| item["Name"].as_str() == Some("Amount"))
                    })
                    .and_then(|item| item["Value"].as_i64());
                let mpesa_receipt = items
                    .as_array()
                    .and_then(|arr| {
                        arr.iter()
                            .find(|item| item["Name"].as_str() == Some("MpesaReceiptNumber"))
                    })
                    .and_then(|item| item["Value"].as_str())
                    .map(String::from);
                let phone_number = items
                    .as_array()
                    .and_then(|arr| {
                        arr.iter()
                            .find(|item| item["Name"].as_str() == Some("PhoneNumber"))
                    })
                    .and_then(|item| item["Value"].as_i64())
                    .map(|n| n.to_string());

                debug!(
                    receipt = ?mpesa_receipt,
                    amount = ?amount,
                    phone = ?phone_number,
                    "M-Pesa payment succeeded"
                );

                WebhookEventType::PaymentSucceeded
            }
            1032 => {
                debug!("M-Pesa payment cancelled by user");
                WebhookEventType::PaymentFailed
            }
            1037 => {
                debug!("M-Pesa payment timeout");
                WebhookEventType::PaymentFailed
            }
            1 => {
                // Insufficient balance
                debug!("M-Pesa insufficient balance");
                WebhookEventType::PaymentFailed
            }
            _ => {
                warn!(code = result_code, "Unknown M-Pesa result code");
                WebhookEventType::Unknown
            }
        };

        let failure_reason = if result_code != 0 {
            Some(result_desc)
        } else {
            None
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "result_code".to_string(),
            serde_json::json!(result_code),
        );
        metadata.insert(
            "result_desc".to_string(),
            serde_json::json!(result_desc),
        );
        if let Some(ref merchant_id) = merchant_request_id {
            metadata.insert(
                "merchant_request_id".to_string(),
                serde_json::json!(merchant_id),
            );
        }

        Ok(ParsedWebhookEvent {
            event_type,
            provider_event_type: format!("stkCallback.result_{}", result_code),
            invoice_id: None,
            provider_payment_id: checkout_request_id,
            transaction_id: merchant_request_id,
            amount_cents: None,
            currency: Some("KES".to_string()),
            failure_reason,
            customer_id: None,
            metadata,
        })
    }

    // ----------------------------------------------------------------
    // Payment status polling (STK Query)
    // ----------------------------------------------------------------

    async fn check_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<PaymentStatus, BillingError> {
        let token = self.get_access_token().await?;
        let (password, timestamp) = self.generate_password();

        let url = format!("{}/mpesa/stkpushquery/v1/query", self.base_url);

        let body = serde_json::json!({
            "BusinessShortCode": self.config.business_short_code,
            "Password": password,
            "Timestamp": timestamp,
            "CheckoutRequestID": provider_payment_id,
        });

        let resp = self
            .http_client
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "M-Pesa STK query failed: {}",
                e
            )))?;

        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse M-Pesa STK query response: {}",
                e
            )))?;

        let result_code = resp_body["ResultCode"].as_i64().unwrap_or(-1);

        let status = match result_code {
            0 => PaymentStatus::Succeeded,
            1032 => PaymentStatus::Failed,   // cancelled
            1037 => PaymentStatus::Pending,   // timeout (still might complete)
            1 => PaymentStatus::Failed,        // insufficient balance
            _ => PaymentStatus::Unknown(format!("result_code {}", result_code)),
        };

        debug!(
            checkout_request_id = %provider_payment_id,
            result_code = result_code,
            status = ?status,
            "M-Pesa STK query result"
        );

        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shellwego_schema::billing::MpesaEnvironment;

    fn make_config() -> MpesaConfig {
        MpesaConfig {
            consumer_key: "test_key".to_string(),
            consumer_secret: "test_secret".to_string(),
            passkey: "test_passkey".to_string(),
            business_short_code: "174379".to_string(),
            environment: MpesaEnvironment::Sandbox,
            callback_url: "https://example.com/callback".to_string(),
        }
    }

    #[test]
    fn test_mpesa_provider_name() {
        let provider = MpesaProvider::new(make_config());
        assert_eq!(provider.provider_name(), "mpesa");
    }

    #[test]
    fn test_mpesa_sandbox_url() {
        let provider = MpesaProvider::new(make_config());
        assert_eq!(provider.base_url, MPESA_SANDBOX_URL);
    }

    #[test]
    fn test_mpesa_production_url() {
        let mut config = make_config();
        config.environment = MpesaEnvironment::Production;
        let provider = MpesaProvider::new(config);
        assert_eq!(provider.base_url, MPESA_PRODUCTION_URL);
    }

    #[test]
    fn test_parse_stk_callback_success() {
        let provider = MpesaProvider::new(make_config());
        let payload = r#"{
            "Body": {
                "stkCallback": {
                    "MerchantRequestID": "29115-34620561-1",
                    "CheckoutRequestID": "ws_CO_191220191020363925",
                    "ResultCode": 0,
                    "ResultDesc": "The service request is processed successfully.",
                    "CallbackMetadata": {
                        "Item": [
                            { "Name": "Amount", "Value": 100 },
                            { "Name": "MpesaReceiptNumber", "Value": "SJF4VQ3P6V" },
                            { "Name": "PhoneNumber", "Value": 254708374149 }
                        ]
                    }
                }
            }
        }"#;

        let event = provider.parse_webhook_event(payload.as_bytes()).unwrap();
        assert_eq!(event.event_type, WebhookEventType::PaymentSucceeded);
        assert_eq!(
            event.provider_payment_id,
            Some("ws_CO_191220191020363925".to_string())
        );
    }

    #[test]
    fn test_parse_stk_callback_cancelled() {
        let provider = MpesaProvider::new(make_config());
        let payload = r#"{
            "Body": {
                "stkCallback": {
                    "MerchantRequestID": "29115-34620561-1",
                    "CheckoutRequestID": "ws_CO_191220191020363925",
                    "ResultCode": 1032,
                    "ResultDesc": "Request cancelled by user"
                }
            }
        }"#;

        let event = provider.parse_webhook_event(payload.as_bytes()).unwrap();
        assert_eq!(event.event_type, WebhookEventType::PaymentFailed);
        assert!(event.failure_reason.is_some());
    }
}
