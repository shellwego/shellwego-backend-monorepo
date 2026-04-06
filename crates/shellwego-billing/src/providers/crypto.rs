//! Cryptocurrency payment provider implementation.
//!
//! Supports crypto payments by generating a payment address and expected
//! amount from fiat, then polling a blockchain explorer (e.g., mempool.space)
//! to detect incoming transactions with sufficient confirmations.
//!
//! Crypto payments are inherently asynchronous and do not support
//! instant refunds or webhook-based notifications.

use std::collections::HashMap;

use async_trait::async_trait;
use tracing::{debug, info, warn};

use super::*;
use shellwego_schema::billing::CryptoConfig;

/// Default crypto address for BTC when none is configured in metadata.
const DEFAULT_BTC_ADDRESS: &str = "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq";

/// Cryptocurrency payment provider.
pub struct CryptoProvider {
    /// Crypto payment configuration.
    config: CryptoConfig,
    /// Reusable HTTP client.
    http_client: reqwest::Client,
}

impl CryptoProvider {
    /// Create a new crypto provider.
    pub fn new(config: CryptoConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build crypto HTTP client"),
        }
    }

    // ----------------------------------------------------------------
    // Helpers
    // ----------------------------------------------------------------

    /// Fetch the exchange rate from the configured rate API.
    ///
    /// Expected response format:
    /// ```json
    /// { "rate": 65000.0 }
    /// ```
    /// or any JSON where a top-level float `rate` field exists.
    async fn fetch_exchange_rate(&self, crypto_code: &str) -> Result<f64, BillingError> {
        let url = format!("{}?from=USD&to={}", self.config.rate_api_url, crypto_code);

        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Exchange rate API request failed: {}",
                e
            )))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse exchange rate response: {}",
                e
            )))?;

        if !status.is_success() {
            return Err(BillingError::HttpError(format!(
                "Exchange rate API returned HTTP {}",
                status
            )));
        }

        // Try common field names for the rate
        let rate = body["rate"]
            .as_f64()
            .or_else(|| body["price"].as_f64())
            .or_else(|| body["data"]["rate"].as_f64())
            .or_else(|| body["data"]["price"].as_f64())
            .or_else(|| {
                // Try first value in the result if it's a simple mapping
                body.as_f64()
            })
            .ok_or_else(|| {
                BillingError::ProviderError(
                    "Could not extract exchange rate from API response".to_string(),
                )
            })?;

        if rate <= 0.0 {
            return Err(BillingError::ProviderError(format!(
                "Invalid exchange rate from API: {}",
                rate
            )));
        }

        debug!(crypto = crypto_code, rate = rate, "Fetched exchange rate");
        Ok(rate)
    }

    /// Convert a fiat amount in cents to crypto using the given exchange rate
    /// and decimal places.
    fn fiat_to_crypto(fiat_cents: i64, exchange_rate: f64, decimals: u8) -> f64 {
        let fiat_amount = fiat_cents as f64 / 100.0;
        fiat_amount / exchange_rate
    }

    /// Get the decimal places for a supported crypto currency.
    fn get_decimals(&self, crypto_code: &str) -> u8 {
        self.config
            .supported_currencies
            .iter()
            .find(|c| c.code.eq_ignore_ascii_case(crypto_code))
            .map(|c| c.decimals)
            .unwrap_or(8) // Default to 8 decimals for BTC-like
    }

    /// Fetch transactions for an address from mempool.space API.
    ///
    /// Expected response: array of transaction objects with `vin` and `vout`.
    async fn fetch_address_transactions(
        &self,
        address: &str,
    ) -> Result<Vec<serde_json::Value>, BillingError> {
        let url = format!(
            "{}/address/{}/txs",
            self.config.mempool_api_url, address
        );

        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| BillingError::HttpError(format!(
                "Mempool API request failed: {}",
                e
            )))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| BillingError::ProviderError(format!(
                "Failed to parse mempool API response: {}",
                e
            )))?;

        if !status.is_success() {
            return Err(BillingError::HttpError(format!(
                "Mempool API returned HTTP {}",
                status
            )));
        }

        let txs = body
            .as_array()
            .cloned()
            .unwrap_or_default();

        Ok(txs)
    }

    /// Check if any transaction sends the expected amount to the given address
    /// within a tolerance of 1%.
    fn find_matching_transaction(
        &self,
        txs: &[serde_json::Value],
        address: &str,
        expected_amount: f64,
    ) -> Option<(String, u32, f64)> {
        let tolerance = 0.01; // 1%

        for tx in txs {
            let txid = tx["txid"].as_str().unwrap_or("");
            let status = tx["status"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            let confirmations = status["confirmations"]
                .as_u64()
                .unwrap_or(0) as u32;

            // Check vout for payments to our address
            if let Some(vouts) = tx["vout"].as_array() {
                for vout in vouts {
                    let scriptpubkey_address = vout["scriptpubkey_address"]
                        .as_str()
                        .unwrap_or("");
                    if scriptpubkey_address == address {
                        let value = vout["value"].as_f64().unwrap_or(0.0);

                        // Check amount within tolerance
                        if expected_amount > 0.0 {
                            let diff = (value - expected_amount).abs() / expected_amount;
                            if diff <= tolerance {
                                return Some((
                                    txid.to_string(),
                                    confirmations,
                                    value,
                                ));
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

#[async_trait]
impl PaymentProvider for CryptoProvider {
    fn provider_name(&self) -> &str {
        "crypto"
    }

    // ----------------------------------------------------------------
    // Charge (Generate payment address + expected amount)
    // ----------------------------------------------------------------

    async fn charge(&self, request: ChargeRequest) -> Result<PaymentResult, BillingError> {
        info!(
            invoice_id = %request.invoice_id,
            amount_cents = request.amount_cents,
            "Generating crypto payment instructions"
        );

        // Determine which cryptocurrency to use
        let crypto_code = request
            .metadata
            .get("crypto_currency")
            .cloned()
            .unwrap_or_else(|| "BTC".to_string());

        // Get payment address from metadata, or use default
        let address = request
            .metadata
            .get("crypto_address")
            .cloned()
            .unwrap_or_else(|| DEFAULT_BTC_ADDRESS.to_string());

        // Fetch exchange rate and compute expected crypto amount
        let decimals = self.get_decimals(&crypto_code);
        let exchange_rate = self.fetch_exchange_rate(&crypto_code).await?;
        let crypto_amount = Self::fiat_to_crypto(request.amount_cents, exchange_rate, decimals);

        // Format the amount with appropriate decimal places
        let formatted_amount = format!("{:.1$}", crypto_amount, decimals as usize);

        debug!(
            crypto = %crypto_code,
            address = %address,
            fiat_cents = request.amount_cents,
            exchange_rate = exchange_rate,
            crypto_amount = %formatted_amount,
            "Crypto payment instructions generated"
        );

        // Return the address as the transaction_id for status polling
        Ok(PaymentResult {
            success: true,
            transaction_id: Some(address.clone()),
            message: format!(
                "Send {} {} to {}. Exchange rate: 1 {} = ${:.2}",
                formatted_amount, crypto_code, address, crypto_code, exchange_rate
            ),
        })
    }

    // ----------------------------------------------------------------
    // Refund – not supported
    // ----------------------------------------------------------------

    async fn refund(&self, request: RefundRequest) -> Result<RefundResult, BillingError> {
        warn!(
            transaction_id = %request.original_transaction_id,
            "Crypto payments do not support programmatic refunds"
        );
        Ok(RefundResult {
            refund_id: String::new(),
            status: RefundStatus::Failed,
            amount_cents_refunded: 0,
            message: "Cryptocurrency payments cannot be refunded programmatically. Process refunds manually from your wallet.".to_string(),
        })
    }

    // ----------------------------------------------------------------
    // Webhook verification – no webhooks
    // ----------------------------------------------------------------

    fn verify_webhook(&self, _payload: &[u8], _signature: &str) -> Result<bool, BillingError> {
        // Crypto payments use polling, not webhooks.
        Ok(true)
    }

    // ----------------------------------------------------------------
    // Webhook parsing – no webhooks
    // ----------------------------------------------------------------

    fn parse_webhook_event(&self, _payload: &[u8]) -> Result<ParsedWebhookEvent, BillingError> {
        Ok(ParsedWebhookEvent {
            event_type: WebhookEventType::Unknown,
            provider_event_type: "crypto.no_webhook".to_string(),
            invoice_id: None,
            provider_payment_id: None,
            transaction_id: None,
            amount_cents: None,
            currency: None,
            failure_reason: None,
            customer_id: None,
            metadata: HashMap::new(),
        })
    }

    // ----------------------------------------------------------------
    // Payment status polling (mempool.space)
    // ----------------------------------------------------------------

    async fn check_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<PaymentStatus, BillingError> {
        // provider_payment_id is the crypto address
        let address = provider_payment_id;

        debug!(
            address = %address,
            "Polling crypto payment status"
        );

        let txs = self.fetch_address_transactions(address).await?;

        // We need to find a transaction with the expected amount.
        // Since we don't store the expected amount here, we look for any
        // recent transaction to this address. In production, you would
        // store the expected amount in a database keyed by address.

        // For a basic implementation, we check if any transaction with
        // confirmations >= required_confirmations exists to this address.
        let required_confirms = self.config.required_confirmations;

        let mut best_match: Option<(String, u32, f64)> = None;

        for tx in &txs {
            let txid = tx["txid"].as_str().unwrap_or("");
            let confirmations = tx["status"]["confirmations"]
                .as_u64()
                .unwrap_or(0) as u32;

            if let Some(vouts) = tx["vout"].as_array() {
                for vout in vouts {
                    let scriptpubkey_address = vout["scriptpubkey_address"]
                        .as_str()
                        .unwrap_or("");
                    if scriptpubkey_address == address {
                        let value = vout["value"].as_f64().unwrap_or(0.0);

                        if best_match.is_none() || confirmations > best_match.as_ref().unwrap().1 {
                            best_match = Some((txid.to_string(), confirmations, value));
                        }
                    }
                }
            }
        }

        match best_match {
            Some((txid, confirmations, value)) => {
                if confirmations >= required_confirms {
                    info!(
                        txid = %txid,
                        confirmations = confirmations,
                        value = value,
                        "Crypto payment confirmed"
                    );
                    Ok(PaymentStatus::Succeeded)
                } else {
                    debug!(
                        txid = %txid,
                        confirmations = confirmations,
                        required = required_confirms,
                        "Crypto payment awaiting confirmations"
                    );
                    Ok(PaymentStatus::Pending)
                }
            }
            None => {
                debug!("No transactions found for crypto address");
                Ok(PaymentStatus::Pending)
            }
        }
    }

    /// Extended check that also validates the expected amount.
    ///
    /// This is a convenience method for callers that have the expected
    /// crypto amount available (e.g., from a database record).
    pub async fn check_payment_status_with_amount(
        &self,
        provider_payment_id: &str,
        expected_crypto_amount: f64,
    ) -> Result<PaymentStatus, BillingError> {
        let address = provider_payment_id;
        let txs = self.fetch_address_transactions(address).await?;
        let required_confirms = self.config.required_confirmations;

        match self.find_matching_transaction(&txs, address, expected_crypto_amount) {
            Some((txid, confirmations, value)) => {
                if confirmations >= required_confirms {
                    info!(
                        txid = %txid,
                        confirmations = confirmations,
                        value = value,
                        expected = expected_crypto_amount,
                        "Crypto payment confirmed with amount match"
                    );
                    Ok(PaymentStatus::Succeeded)
                } else {
                    debug!(
                        txid = %txid,
                        confirmations = confirmations,
                        required = required_confirms,
                        "Crypto payment found but awaiting confirmations"
                    );
                    Ok(PaymentStatus::Pending)
                }
            }
            None => {
                debug!(
                    address = %address,
                    expected = expected_crypto_amount,
                    "No matching crypto transaction found"
                );
                Ok(PaymentStatus::Pending)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shellwego_schema::billing::CryptoCurrencyConfig;

    fn make_config() -> CryptoConfig {
        CryptoConfig {
            mempool_api_url: "https://mempool.space/api".to_string(),
            required_confirmations: 3,
            rate_api_url: "https://api.example.com/rate".to_string(),
            supported_currencies: vec![CryptoCurrencyConfig {
                code: "BTC".to_string(),
                network: "bitcoin".to_string(),
                decimals: 8,
                confirmation_timeout_minutes: 60,
            }],
        }
    }

    #[test]
    fn test_crypto_provider_name() {
        let provider = CryptoProvider::new(make_config());
        assert_eq!(provider.provider_name(), "crypto");
    }

    #[test]
    fn test_fiat_to_crypto() {
        // $100 at $50,000/BTC = 0.002 BTC
        let result = CryptoProvider::fiat_to_crypto(10000, 50000.0, 8);
        assert!((result - 0.002).abs() < 0.0000001);
    }

    #[test]
    fn test_fiat_to_crypto_small() {
        // $10 at $50,000/BTC = 0.0002 BTC
        let result = CryptoProvider::fiat_to_crypto(1000, 50000.0, 8);
        assert!((result - 0.0002).abs() < 0.0000001);
    }

    #[test]
    fn test_get_decimals_known() {
        let config = make_config();
        let provider = CryptoProvider::new(config);
        assert_eq!(provider.get_decimals("BTC"), 8);
    }

    #[test]
    fn test_get_decimals_unknown() {
        let config = make_config();
        let provider = CryptoProvider::new(config);
        assert_eq!(provider.get_decimals("ETH"), 8); // Default
    }

    #[test]
    fn test_find_matching_transaction_exact() {
        let provider = CryptoProvider::new(make_config());
        let txs = vec![serde_json::json!({
            "txid": "abc123",
            "status": { "confirmations": 5 },
            "vout": [{
                "scriptpubkey_address": DEFAULT_BTC_ADDRESS,
                "value": 0.001
            }]
        })];

        let result = provider.find_matching_transaction(&txs, DEFAULT_BTC_ADDRESS, 0.001);
        assert!(result.is_some());
        let (txid, confs, value) = result.unwrap();
        assert_eq!(txid, "abc123");
        assert_eq!(confs, 5);
        assert!((value - 0.001).abs() < 0.0000001);
    }

    #[test]
    fn test_find_matching_transaction_within_tolerance() {
        let provider = CryptoProvider::new(make_config());
        // 0.5% deviation should be within 1% tolerance
        let txs = vec![serde_json::json!({
            "txid": "def456",
            "status": { "confirmations": 3 },
            "vout": [{
                "scriptpubkey_address": DEFAULT_BTC_ADDRESS,
                "value": 0.000995 // 0.5% less than 0.001
            }]
        })];

        let result = provider.find_matching_transaction(&txs, DEFAULT_BTC_ADDRESS, 0.001);
        assert!(result.is_some());
    }

    #[test]
    fn test_find_matching_transaction_outside_tolerance() {
        let provider = CryptoProvider::new(make_config());
        // 2% deviation should be outside 1% tolerance
        let txs = vec![serde_json::json!({
            "txid": "ghi789",
            "status": { "confirmations": 1 },
            "vout": [{
                "scriptpubkey_address": DEFAULT_BTC_ADDRESS,
                "value": 0.0008 // 20% less than 0.001
            }]
        })];

        let result = provider.find_matching_transaction(&txs, DEFAULT_BTC_ADDRESS, 0.001);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_matching_transaction_wrong_address() {
        let provider = CryptoProvider::new(make_config());
        let txs = vec![serde_json::json!({
            "txid": "jkl012",
            "status": { "confirmations": 10 },
            "vout": [{
                "scriptpubkey_address": "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",
                "value": 0.001
            }]
        })];

        let result = provider.find_matching_transaction(&txs, DEFAULT_BTC_ADDRESS, 0.001);
        assert!(result.is_none());
    }
}
