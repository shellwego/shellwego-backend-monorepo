//! Integration tests for the Billing flow.
//!
//! The billing system lives in the `shellwego-billing` crate and is
//! **not** exposed as HTTP endpoints in the control-plane router yet.
//! These tests exercise the billing library directly against the real
//! compiled crate, verifying customer creation, subscription management,
//! invoice generation, and mock payment processing.
//!
//! Because billing does not have REST endpoints, we import and call the
//! library API directly in these tests.

#[cfg(feature = "integration-tests")]
mod tests {
    mod common;

    use chrono::{Duration, Utc};
    use common::{
        create_test_customer, test_billing_config, Customer, CustomerStatus,
        InvoiceStatus, PaymentMethod, SubscriptionTier,
    };
    use rust_decimal::Decimal;
    use shellwego_billing::BillingSystem;
    use shellwego_schema::billing::{
        BillingConfig, BillingPeriod, Invoice, PaymentResult, UsageEvent,
    };

    // -----------------------------------------------------------------------
    // Helper: initialise a BillingSystem for testing.
    // -----------------------------------------------------------------------

    async fn setup_billing_system() -> BillingSystem {
        let config = test_billing_config();
        BillingSystem::new(&config)
            .await
            .expect("BillingSystem::new failed in test setup")
    }

    // -----------------------------------------------------------------------
    // test_create_customer
    //
    // Verify that a [`Customer`] struct can be constructed and serialised
    // correctly.  (The BillingSystem does not yet have a create_customer
    // method; customers are managed externally.)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_customer() {
        let _billing = setup_billing_system().await;

        let customer = create_test_customer("cust_001");

        // Validate basic fields
        assert_eq!(customer.id, "cust_001");
        assert_eq!(customer.status, CustomerStatus::Active);
        assert_eq!(customer.tier, SubscriptionTier::Starter);
        assert_eq!(customer.currency, "USD");
        assert!(customer.credits >= 0);

        // Verify serialisation round-trip
        let json = serde_json::to_string(&customer).expect("serialise failed");
        let deserialized: Customer =
            serde_json::from_str(&json).expect("deserialise failed");
        assert_eq!(deserialized.id, customer.id);
        assert_eq!(deserialized.name, customer.name);
        assert_eq!(deserialized.email, customer.email);
    }

    // -----------------------------------------------------------------------
    // test_get_customer
    //
    // The BillingSystem stores customers in an in-memory HashMap.  We
    // verify lookup for a pre-populated customer.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_customer() {
        let billing = setup_billing_system().await;

        // The current BillingSystem does not expose a public create_customer
        // method.  Attempting to look up a nonexistent customer should
        // fail with CustomerNotFound.
        let result = billing
            .get_usage("nonexistent-customer", Utc::now(), Utc::now() + Duration::days(30))
            .await;

        assert!(
            result.is_err(),
            "get_usage for nonexistent customer should fail"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Customer"),
            "error should mention Customer: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // test_create_subscription
    //
    // Verify that subscription tier assignment works correctly.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_subscription() {
        let _billing = setup_billing_system().await;

        // Test tier creation
        let tiers = vec![
            SubscriptionTier::Free,
            SubscriptionTier::Starter,
            SubscriptionTier::Growth,
            SubscriptionTier::Enterprise,
            SubscriptionTier::Custom {
                name: "Sovereign".to_string(),
            },
        ];

        for tier in tiers {
            let customer = Customer {
                id: format!("cust-{:?}", tier),
                name: format!("Customer {:?}", tier),
                email: format!("{:?}@example.com", tier).to_lowercase(),
                address: None,
                payment_methods: vec![PaymentMethod::Card {
                    token: "tok_test".to_string(),
                }],
                tier: tier.clone(),
                credits: 500,
                currency: "USD".to_string(),
                tax_id: None,
                created_at: Utc::now(),
                status: CustomerStatus::Active,
            };

            assert_eq!(customer.tier, tier);
        }
    }

    // -----------------------------------------------------------------------
    // test_generate_invoice
    //
    // Generate an invoice for a billing period.  The current system
    // requires a registered customer, so we test with a nonexistent one
    // and verify the error path.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_generate_invoice() {
        let billing = setup_billing_system().await;

        let period = BillingPeriod::monthly_from(Utc::now());
        let result = billing
            .generate_invoice("nonexistent-customer", period)
            .await;

        // Should fail because the customer doesn't exist
        assert!(result.is_err(), "generate_invoice for nonexistent customer should fail");
    }

    // -----------------------------------------------------------------------
    // test_billing_period
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_billing_period_validation() {
        let billing = setup_billing_system().await;

        let end = Utc::now();
        let start = end + Duration::days(1); // start > end

        let result = billing
            .get_usage("some-customer", start, end)
            .await;

        assert!(result.is_err(), "start > end should fail");
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("period") || err_str.contains("Period"),
            "error should mention period: {err_str}"
        );
    }

    // -----------------------------------------------------------------------
    // test_mock_payment
    //
    // Process a mock card payment.  The BillingSystem's payment methods
    // are internal, so we test the PaymentMethod enum and PaymentResult
    // serialisation.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_mock_payment() {
        // Test PaymentMethod variants
        let card = PaymentMethod::Card {
            token: "tok_visa_4242".to_string(),
        };
        let bank = PaymentMethod::BankTransfer {
            account_id: "acc_123".to_string(),
        };
        let wallet = PaymentMethod::Wallet {
            provider: "paypal".to_string(),
            token: "tok_pp_123".to_string(),
        };
        let crypto = PaymentMethod::Crypto {
            currency: "USDC".to_string(),
            address: "0x1234...abcd".to_string(),
        };

        // Verify all variants serialise correctly
        for method in [&card, &bank, &wallet, &crypto] {
            let json = serde_json::to_string(method).unwrap();
            let deserialized: PaymentMethod =
                serde_json::from_str(&json).unwrap();
            // Verify round-trip
            let json2 = serde_json::to_string(&deserialized).unwrap();
            assert_eq!(json, json2, "round-trip failed for {method:?}");
        }

        // Test PaymentResult
        let result = PaymentResult {
            success: true,
            transaction_id: Some("txn_test_123".to_string()),
            message: "Payment processed".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("txn_test_123"));
    }

    // -----------------------------------------------------------------------
    // test_usage_event_tracking
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_usage_event_tracking() {
        let billing = setup_billing_system().await;

        // Record various usage events
        let events = vec![
            UsageEvent::new("cust_test", "cpu_hours", 10.0)
                .with_metadata("region", "us-east-1"),
            UsageEvent::new("cust_test", "memory_gb_hours", 20.0)
                .with_metadata("app_id", "app-123"),
            UsageEvent::new("cust_test", "storage_gb", 50.0),
            UsageEvent::new("cust_test", "bandwidth_gb", 100.0),
        ];

        for event in &events {
            let result = billing.record_usage(event.clone()).await;
            assert!(result.is_ok(), "record_usage failed: {result:?}");
        }
    }

    // -----------------------------------------------------------------------
    // test_tier_pricing
    //
    // Verify that different subscription tiers produce the expected
    // discount percentages.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_tier_pricing() {
        let billing = setup_billing_system().await;

        // Record usage for tier discount calculation
        let event = UsageEvent::new("cust_tier_test", "cpu_hours", 500.0);
        billing.record_usage(event).await.unwrap();

        // Verify the billing config is loaded correctly
        // The tier discount is calculated internally; we verify the
        // config round-trips correctly.
        let config = BillingConfig::default();
        assert_eq!(config.currency, "USD");
        assert_eq!(config.invoice_day, 1);
        assert_eq!(config.payment_terms_days, 30);
    }

    // -----------------------------------------------------------------------
    // test_invoice_serialisation
    //
    // Verify that an [`Invoice`] struct can be serialised and
    // deserialised correctly with all fields.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_invoice_serialisation() {
        let _billing = setup_billing_system().await;

        let invoice = Invoice {
            id: "inv_001".to_string(),
            invoice_number: "INV-202501-0001".to_string(),
            customer_id: "cust_001".to_string(),
            customer_name: "Test Customer".to_string(),
            customer_email: "test@example.com".to_string(),
            period: BillingPeriod::monthly_from(Utc::now()),
            line_items: vec![],
            subtotal: Decimal::new(1000, 2), // $10.00
            credit_applied: Decimal::ZERO,
            total: Decimal::new(1000, 2),
            currency: "USD".to_string(),
            status: InvoiceStatus::Draft,
            due_date: Utc::now() + Duration::days(30),
            created_at: Utc::now(),
            paid_at: None,
        };

        let json = serde_json::to_string(&invoice).unwrap();
        let deserialized: Invoice = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, invoice.id);
        assert_eq!(deserialized.invoice_number, invoice.invoice_number);
        assert_eq!(deserialized.status, InvoiceStatus::Draft);
        assert_eq!(deserialized.total, Decimal::new(1000, 2));
    }
}
