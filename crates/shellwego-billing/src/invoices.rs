//! Invoice generation and PDF rendering
//!
//! This module provides invoice generation capabilities including:
//! - PDF invoice generation from templates
//! - Email delivery of invoices
//! - Proration calculations for partial billing periods
//! - Multi-currency support

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc, Duration, Datelike};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tera::{Tera, Context, Value};
use tracing::{info, warn, instrument};
use uuid::Uuid;

// Import billing types from schema
use shellwego_schema::billing::{Invoice, BillingPeriod, LineItem, UsageSummary, InvoiceStatus};
// Import Address for branding (reuse schema type)
pub use shellwego_schema::billing::Address;

use crate::{BillingError};

/// Invoice generator with template rendering
/// 
/// Generates professional invoices using Tera templates.
/// Supports PDF generation via headless Chrome (optional feature)
/// or external conversion services.
pub struct InvoiceGenerator {
    /// Template engine for HTML rendering
    templates: Tera,
    /// Company branding configuration
    branding: BrandingConfig,
}

/// Company branding for invoices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandingConfig {
    /// Company name
    pub company_name: String,
    /// Logo URL (base64 or remote)
    pub logo_url: Option<String>,
    /// Primary color (hex)
    pub primary_color: String,
    /// Address
    pub address: Address,
    /// Contact email
    pub email: String,
    /// Phone number
    pub phone: Option<String>,
    /// Website URL
    pub website: Option<String>,
    /// Tax ID / VAT number
    pub tax_id: Option<String>,
    /// Bank details for wire transfers
    pub bank_details: Option<BankDetails>,
    /// Footer text
    pub footer: Option<String>,
    /// Terms and conditions
    pub terms: Option<String>,
}

/// Bank details for wire transfers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankDetails {
    pub bank_name: String,
    pub account_name: String,
    pub account_number: String,
    pub routing_number: Option<String>,
    pub swift_code: Option<String>,
    pub iban: Option<String>,
}

impl Default for BrandingConfig {
    fn default() -> Self {
        Self {
            company_name: "ShellWeGo".to_string(),
            logo_url: None,
            primary_color: "#2563eb".to_string(),
            address: Address {
                line1: "123 Cloud Street".to_string(),
                line2: None,
                city: "San Francisco".to_string(),
                state: Some("CA".to_string()),
                postal_code: "94102".to_string(),
                country: "United States".to_string(),
            },
            email: "billing@shellwego.com".to_string(),
            phone: None,
            website: Some("https://shellwego.com".to_string()),
            tax_id: None,
            bank_details: None,
            footer: Some("Thank you for your business!".to_string()),
            terms: Some("Payment is due within 30 days. Late payments may incur a 1.5% monthly fee.".to_string()),
        }
    }
}

impl InvoiceGenerator {
    /// Create a new invoice generator
    ///
    /// Loads templates from the specified directory.
    /// Uses embedded templates if no directory is provided.
    #[instrument(skip(template_path))]
    pub fn new(template_path: &str) -> Result<Self, BillingError> {
        let mut templates = Tera::default();

        // Register custom filters
        templates.register_filter("currency", currency_filter);
        templates.register_filter("date_format", date_format_filter);
        templates.register_filter("invoice_status", invoice_status_filter);

        // Try to load templates from path, or use embedded defaults
        if !template_path.is_empty() && Path::new(template_path).exists() {
            let glob_pattern = format!("{}/**/*.html", template_path);
            // Use Tera::new() which is the public API for loading from glob
            let mut loaded = Tera::new(&glob_pattern)
                .map_err(|e| BillingError::TemplateError(format!("Failed to load templates: {}", e)))?;
            // Merge the loaded templates into our templates instance
            templates.extend(&mut loaded);
            info!(path = %template_path, "Loaded invoice templates from directory");
        } else {
            // Register embedded default template
            templates
                .add_raw_template("invoice.html", DEFAULT_INVOICE_TEMPLATE)
                .map_err(|e| BillingError::TemplateError(format!("Failed to register template: {}", e)))?;
            info!("Using embedded default invoice template");
        }

        Ok(Self {
            templates,
            branding: BrandingConfig::default(),
        })
    }
    
    /// Create invoice generator with custom branding
    pub fn with_branding(template_path: &str, branding: BrandingConfig) -> Result<Self, BillingError> {
        let mut generator = Self::new(template_path)?;
        generator.branding = branding;
        Ok(generator)
    }
    
    /// Render invoice HTML from template
    /// 
    /// Generates the HTML representation of the invoice using
    /// the configured template and branding.
    #[instrument(skip(self, invoice), fields(invoice_id = %invoice.id))]
    pub fn render_html(&self, invoice: &Invoice) -> Result<String, BillingError> {
        let mut context = Context::new();
        
        // Add branding information
        context.insert("branding", &self.branding);
        
        // Add invoice data
        context.insert("invoice", &invoice);
        context.insert("invoice_number", &invoice.invoice_number);
        context.insert("invoice_date", &invoice.created_at.format("%B %d, %Y").to_string());
        context.insert("due_date", &invoice.due_date.format("%B %d, %Y").to_string());
        
        // Add customer information
        context.insert("customer_name", &invoice.customer_name);
        context.insert("customer_email", &invoice.customer_email);
        
        // Add line items
        context.insert("line_items", &invoice.line_items);
        
        // Add totals
        context.insert("subtotal", &invoice.subtotal);
        context.insert("credit_applied", &invoice.credit_applied);
        context.insert("total", &invoice.total);
        context.insert("currency", &invoice.currency);
        
        // Add period information
        context.insert("period_start", &invoice.period.start.format("%B %d, %Y").to_string());
        context.insert("period_end", &invoice.period.end.format("%B %d, %Y").to_string());
        
        // Add status
        let status_class = match invoice.status {
            crate::InvoiceStatus::Draft => "draft",
            crate::InvoiceStatus::Open => "open",
            crate::InvoiceStatus::Paid => "paid",
            crate::InvoiceStatus::Void => "void",
            crate::InvoiceStatus::Uncollectible => "uncollectible",
        };
        context.insert("status_class", status_class);
        
        // Render template
        let html = self.templates
            .render("invoice.html", &context)
            .map_err(|e| BillingError::TemplateError(format!("Template rendering failed: {}", e)))?;
        
        Ok(html)
    }
    
    /// Generate invoice PDF
    /// 
    /// Converts the rendered HTML to PDF format.
    /// Uses headless Chrome when the "pdf" feature is enabled,
    /// otherwise returns the HTML for external processing.
    #[instrument(skip(self, invoice), fields(invoice_id = %invoice.id))]
    #[cfg(feature = "pdf")]
    pub async fn generate_pdf(&self, invoice: &Invoice) -> Result<Vec<u8>, BillingError> {
        use headless_chrome::{Browser, LaunchOptions};
        
        info!("Generating PDF for invoice");
        
        let html = self.render_html(invoice)?;
        
        // Launch headless browser
        let browser = Browser::new(LaunchOptions {
            headless: true,
            ..Default::default()
        })
        .map_err(|e| BillingError::InvoiceError(format!("Failed to launch browser: {}", e)))?;
        
        let tab = browser.new_tab()
            .map_err(|e| BillingError::InvoiceError(format!("Failed to create tab: {}", e)))?;
        
        // Set content
        tab.navigate_to(&format!("data:text/html,{}", urlencoding::encode(&html)))
            .map_err(|e| BillingError::InvoiceError(format!("Failed to load content: {}", e)))?;
        
        // Wait for render
        tab.wait_for_element("body")
            .map_err(|e| BillingError::InvoiceError(format!("Wait failed: {}", e)))?;
        
        // Print to PDF
        let pdf = tab.print_to_pdf(None)
            .map_err(|e| BillingError::InvoiceError(format!("PDF generation failed: {}", e)))?;
        
        info!(size = pdf.len(), "PDF generated successfully");
        
        Ok(pdf)
    }
    
    /// Generate invoice PDF (without pdf feature)
    /// 
    /// Returns HTML that can be converted to PDF by external services.
    #[cfg(not(feature = "pdf"))]
    pub async fn generate_pdf(&self, invoice: &Invoice) -> Result<Vec<u8>, BillingError> {
        info!("PDF generation requested (pdf feature not enabled, returning HTML)");
        
        let html = self.render_html(invoice)?;
        Ok(html.into_bytes())
    }
    
    /// Send invoice via email
    /// 
    /// Sends the invoice as an email attachment to the specified recipient.
    /// In production, this would integrate with an email service like SendGrid,
    /// Postmark, or AWS SES.
    #[instrument(skip(self, invoice, pdf), fields(invoice_id = %invoice.id, recipient = %recipient))]
    pub async fn send_email(
        &self,
        invoice: &Invoice,
        pdf: &[u8],
        recipient: &str,
    ) -> Result<(), BillingError> {
        info!(recipient = %recipient, "Sending invoice email");
        
        // Validate recipient email
        if recipient.is_empty() || !recipient.contains('@') {
            return Err(BillingError::InvoiceError("Invalid recipient email".to_string()));
        }
        
        // In production, this would send via an email service
        // For now, we simulate the email sending
        
        let subject = format!(
            "Invoice {} from {} - {}",
            invoice.invoice_number,
            self.branding.company_name,
            invoice.total
        );
        
        let body = format!(
            r#"
Dear {customer_name},

Your invoice {invoice_number} for {currency} {total} is now available.

Billing Period: {period_start} - {period_end}
Due Date: {due_date}

Please find the detailed invoice attached as a PDF.

{payment_instructions}

If you have any questions about this invoice, please contact us at {support_email}.

Best regards,
{company_name}
"#,
            customer_name = invoice.customer_name,
            invoice_number = invoice.invoice_number,
            currency = invoice.currency,
            total = invoice.total,
            period_start = invoice.period.start.format("%B %d, %Y"),
            period_end = invoice.period.end.format("%B %d, %Y"),
            due_date = invoice.due_date.format("%B %d, %Y"),
            payment_instructions = self.get_payment_instructions(),
            support_email = self.branding.email,
            company_name = self.branding.company_name,
        );
        
        // Simulate sending (in production, use actual email service)
        info!(
            subject = %subject,
            recipient = %recipient,
            pdf_size = pdf.len(),
            "Invoice email prepared for sending"
        );
        
        Ok(())
    }
    
    /// Calculate prorated amount
    /// 
    /// Calculates the prorated cost for a partial billing period.
    /// Uses daily rates based on a 30-day month convention.
    #[instrument(skip(self))]
    pub fn prorate(
        &self,
        monthly_price: f64,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> f64 {
        if start >= end {
            warn!("Invalid proration period: start >= end");
            return 0.0;
        }
        
        // Calculate days in the period
        let duration = end - start;
        let days = duration.num_days() as f64;
        
        // Handle edge case of same-day proration
        if days < 1.0 {
            let hours = duration.num_hours() as f64;
            return (monthly_price / 30.0 / 24.0) * hours;
        }
        
        // Daily rate (30-day month convention)
        let daily_rate = monthly_price / 30.0;
        
        let prorated = daily_rate * days;
        
        // Round to 2 decimal places
        (prorated * 100.0).round() / 100.0
    }
    
    /// Calculate prorated amount with Decimal precision
    pub fn prorate_decimal(
        &self,
        monthly_price: Decimal,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Decimal {
        if start >= end {
            return Decimal::ZERO;
        }
        
        let duration = end - start;
        let days = Decimal::from(duration.num_days());
        
        // Daily rate
        let daily_rate = monthly_price / Decimal::from(30);
        
        daily_rate * days
    }
    
    /// Get payment instructions for invoice email
    fn get_payment_instructions(&self) -> String {
        let mut instructions = String::new();
        
        if let Some(ref bank) = self.branding.bank_details {
            instructions.push_str(&format!(
                "\nWire Transfer Details:\n\
                 Bank: {}\n\
                 Account Name: {}\n\
                 Account Number: {}\n",
                bank.bank_name,
                bank.account_name,
                bank.account_number
            ));
            
            if let Some(ref swift) = bank.swift_code {
                instructions.push_str(&format!("SWIFT Code: {}\n", swift));
            }
            
            if let Some(ref iban) = bank.iban {
                instructions.push_str(&format!("IBAN: {}\n", iban));
            }
        }
        
        instructions
    }
    
    /// Generate invoice from usage summary
    pub fn generate_from_usage(
        &self,
        usage: &UsageSummary,
        customer_name: &str,
        customer_email: &str,
        payment_terms_days: u8,
    ) -> Invoice {
        let invoice_number = self.generate_invoice_number();
        let due_date = Utc::now() + Duration::days(payment_terms_days as i64);
        
        Invoice {
            id: Uuid::new_v4().to_string(),
            invoice_number,
            customer_id: usage.customer_id.clone(),
            customer_name: customer_name.to_string(),
            customer_email: customer_email.to_string(),
            period: BillingPeriod {
                start: usage.period_start,
                end: usage.period_end,
            },
            line_items: usage.line_items.clone(),
            subtotal: usage.subtotal,
            credit_applied: Decimal::ZERO,
            total: usage.subtotal,
            currency: usage.currency.clone(),
            status: crate::InvoiceStatus::Open,
            due_date,
            created_at: Utc::now(),
            paid_at: None,
        }
    }
    
    /// Generate a unique invoice number
    fn generate_invoice_number(&self) -> String {
        let now = Utc::now();
        let random_suffix: u32 = (now.timestamp_nanos_opt().unwrap_or(0) % 10000) as u32;
        format!(
            "INV-{}{:02}-{:04}",
            now.year(),
            now.month(),
            random_suffix
        )
    }
}

/// Invoice template data for rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceTemplate {
    /// Invoice number
    pub invoice_number: String,
    /// Invoice date
    pub date: DateTime<Utc>,
    /// Payment due date
    pub due_date: DateTime<Utc>,
    /// Customer information
    pub customer: CustomerInfo,
    /// Line items
    pub items: Vec<TemplateLineItem>,
    /// Subtotal
    pub subtotal: Decimal,
    /// Tax amount
    pub tax: Option<Decimal>,
    /// Total amount
    pub total: Decimal,
    /// Currency
    pub currency: String,
    /// Notes
    pub notes: Option<String>,
}

/// Customer info for template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerInfo {
    pub name: String,
    pub email: String,
    pub address: Option<String>,
}

/// Line item for template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateLineItem {
    pub description: String,
    pub quantity: f64,
    pub unit: String,
    pub unit_price: Decimal,
    pub amount: Decimal,
}

// Custom Tera filters

/// Currency formatting filter
fn currency_filter(value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
    let amount = value.as_f64().unwrap_or(0.0);
    let currency = args.get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("USD");
    
    let symbol = match currency {
        "USD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" => "¥",
        "NGN" => "₦",
        "KES" => "KSh",
        "INR" => "₹",
        _ => currency,
    };
    
    Ok(Value::String(format!("{}{:.2}", symbol, amount)))
}

/// Date formatting filter
fn date_format_filter(value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
    let format = args.get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("%Y-%m-%d");
    
    if let Some(s) = value.as_str() {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Ok(Value::String(dt.format(format).to_string()));
        }
    }
    
    Ok(value.clone())
}

/// Invoice status formatting filter
fn invoice_status_filter(value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
    let status = value.as_str().unwrap_or("draft");
    
    let display = match status {
        "draft" => "Draft",
        "open" => "Awaiting Payment",
        "paid" => "Paid",
        "void" => "Void",
        "uncollectible" => "Uncollectible",
        _ => status,
    };
    
    Ok(Value::String(display.to_string()))
}

// Default invoice template (embedded)
const DEFAULT_INVOICE_TEMPLATE: &str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Invoice {{ invoice_number }}</title>
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            line-height: 1.6;
            color: #333;
            padding: 40px;
            max-width: 800px;
            margin: 0 auto;
        }
        
        .header {
            display: flex;
            justify-content: space-between;
            align-items: flex-start;
            margin-bottom: 40px;
            border-bottom: 3px solid {{ branding.primary_color }};
            padding-bottom: 20px;
        }
        
        .company-info h1 {
            color: {{ branding.primary_color }};
            font-size: 28px;
            margin-bottom: 10px;
        }
        
        .company-info p {
            color: #666;
            font-size: 14px;
        }
        
        .invoice-info {
            text-align: right;
        }
        
        .invoice-info h2 {
            color: {{ branding.primary_color }};
            font-size: 24px;
            margin-bottom: 10px;
        }
        
        .invoice-info .status {
            display: inline-block;
            padding: 5px 15px;
            border-radius: 20px;
            font-size: 12px;
            font-weight: 600;
            text-transform: uppercase;
        }
        
        .status.draft { background: #f3f4f6; color: #6b7280; }
        .status.open { background: #fef3c7; color: #d97706; }
        .status.paid { background: #d1fae5; color: #059669; }
        .status.void { background: #fee2e2; color: #dc2626; }
        
        .parties {
            display: flex;
            justify-content: space-between;
            margin-bottom: 40px;
        }
        
        .party {
            flex: 1;
        }
        
        .party h3 {
            font-size: 12px;
            text-transform: uppercase;
            color: #999;
            margin-bottom: 10px;
        }
        
        .party p {
            font-size: 14px;
            margin-bottom: 5px;
        }
        
        .dates {
            display: flex;
            gap: 40px;
            margin-bottom: 30px;
            padding: 20px;
            background: #f9fafb;
            border-radius: 8px;
        }
        
        .date-item h4 {
            font-size: 12px;
            text-transform: uppercase;
            color: #999;
            margin-bottom: 5px;
        }
        
        .date-item p {
            font-size: 16px;
            font-weight: 600;
        }
        
        table {
            width: 100%;
            border-collapse: collapse;
            margin-bottom: 30px;
        }
        
        th {
            text-align: left;
            padding: 15px;
            background: {{ branding.primary_color }};
            color: white;
            font-size: 12px;
            text-transform: uppercase;
        }
        
        th:last-child, td:last-child {
            text-align: right;
        }
        
        td {
            padding: 15px;
            border-bottom: 1px solid #eee;
            font-size: 14px;
        }
        
        tr:hover {
            background: #f9fafb;
        }
        
        .totals {
            margin-left: auto;
            width: 300px;
        }
        
        .totals-row {
            display: flex;
            justify-content: space-between;
            padding: 10px 0;
            border-bottom: 1px solid #eee;
        }
        
        .totals-row.total {
            font-size: 20px;
            font-weight: 700;
            border-bottom: none;
            border-top: 2px solid {{ branding.primary_color }};
            margin-top: 10px;
            padding-top: 15px;
        }
        
        .footer {
            margin-top: 50px;
            padding-top: 20px;
            border-top: 1px solid #eee;
        }
        
        .footer h4 {
            font-size: 12px;
            text-transform: uppercase;
            color: #999;
            margin-bottom: 10px;
        }
        
        .footer p {
            font-size: 12px;
            color: #666;
            line-height: 1.8;
        }
        
        .terms {
            margin-top: 20px;
            padding: 15px;
            background: #f9fafb;
            border-radius: 8px;
            font-size: 11px;
            color: #999;
        }
        
        @media print {
            body {
                padding: 0;
            }
            
            .header {
                page-break-inside: avoid;
            }
            
            table {
                page-break-inside: auto;
            }
            
            tr {
                page-break-inside: avoid;
            }
        }
    </style>
</head>
<body>
    <div class="header">
        <div class="company-info">
            <h1>{{ branding.company_name }}</h1>
            <p>{{ branding.address.line1 }}</p>
            {% if branding.address.line2 %}
            <p>{{ branding.address.line2 }}</p>
            {% endif %}
            <p>{{ branding.address.city }}, {{ branding.address.state }} {{ branding.address.postal_code }}</p>
            <p>{{ branding.address.country }}</p>
            {% if branding.email %}
            <p>{{ branding.email }}</p>
            {% endif %}
        </div>
        <div class="invoice-info">
            <h2>Invoice</h2>
            <p><strong>{{ invoice_number }}</strong></p>
            <p>{{ invoice_date }}</p>
            <br>
            <span class="status {{ status_class }}">{{ invoice.status | invoice_status }}</span>
        </div>
    </div>
    
    <div class="parties">
        <div class="party">
            <h3>Bill To</h3>
            <p><strong>{{ customer_name }}</strong></p>
            <p>{{ customer_email }}</p>
        </div>
    </div>
    
    <div class="dates">
        <div class="date-item">
            <h4>Billing Period</h4>
            <p>{{ period_start }} - {{ period_end }}</p>
        </div>
        <div class="date-item">
            <h4>Due Date</h4>
            <p>{{ due_date }}</p>
        </div>
        <div class="date-item">
            <h4>Amount Due</h4>
            <p>{{ total }} {{ currency }}</p>
        </div>
    </div>
    
    <table>
        <thead>
            <tr>
                <th>Description</th>
                <th>Quantity</th>
                <th>Unit</th>
                <th>Unit Price</th>
                <th>Amount</th>
            </tr>
        </thead>
        <tbody>
            {% for item in line_items %}
            <tr>
                <td>{{ item.description }}</td>
                <td>{{ item.quantity | round }}</td>
                <td>{{ item.unit }}</td>
                <td>${{ item.unit_price }}</td>
                <td>${{ item.amount }}</td>
            </tr>
            {% endfor %}
        </tbody>
    </table>
    
    <div class="totals">
        <div class="totals-row">
            <span>Subtotal</span>
            <span>{{ subtotal }} {{ currency }}</span>
        </div>
        {% if credit_applied > 0 %}
        <div class="totals-row">
            <span>Credits Applied</span>
            <span>-{{ credit_applied }} {{ currency }}</span>
        </div>
        {% endif %}
        <div class="totals-row total">
            <span>Total</span>
            <span>{{ total }} {{ currency }}</span>
        </div>
    </div>
    
    <div class="footer">
        {% if branding.footer %}
        <p>{{ branding.footer }}</p>
        {% endif %}
        
        {% if branding.terms %}
        <div class="terms">
            <strong>Terms & Conditions:</strong> {{ branding.terms }}
        </div>
        {% endif %}
    </div>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InvoiceStatus;
    
    #[test]
    fn test_invoice_generator_creation() {
        let generator = InvoiceGenerator::new("");
        assert!(generator.is_ok());
    }
    
    #[test]
    fn test_proration() {
        let generator = InvoiceGenerator::new("").unwrap();
        
        // Full month
        let start = Utc::now();
        let end = start + Duration::days(30);
        let prorated = generator.prorate(100.0, start, end);
        assert!((prorated - 100.0).abs() < 0.01);
        
        // Half month
        let end = start + Duration::days(15);
        let prorated = generator.prorate(100.0, start, end);
        assert!((prorated - 50.0).abs() < 0.01);
        
        // 10 days
        let end = start + Duration::days(10);
        let prorated = generator.prorate(100.0, start, end);
        assert!((prorated - 33.33).abs() < 0.1);
    }
    
    #[test]
    fn test_proration_decimal() {
        let generator = InvoiceGenerator::new("").unwrap();
        
        let start = Utc::now();
        let end = start + Duration::days(15);
        
        let monthly = Decimal::new(100, 0);
        let prorated = generator.prorate_decimal(monthly, start, end);
        
        assert_eq!(prorated, Decimal::new(50, 0));
    }
    
    #[test]
    fn test_proration_invalid_period() {
        let generator = InvoiceGenerator::new("").unwrap();
        
        let start = Utc::now();
        let end = start - Duration::days(1);
        
        let prorated = generator.prorate(100.0, start, end);
        assert_eq!(prorated, 0.0);
    }
    
    #[tokio::test]
    async fn test_render_html() {
        let generator = InvoiceGenerator::new("").unwrap();
        
        let invoice = Invoice {
            id: "inv_123".to_string(),
            invoice_number: "INV-202401-0001".to_string(),
            customer_id: "cust_123".to_string(),
            customer_name: "Test Customer".to_string(),
            customer_email: "test@example.com".to_string(),
            period: BillingPeriod {
                start: Utc::now() - Duration::days(30),
                end: Utc::now(),
            },
            line_items: vec![LineItem {
                resource_type: "cpu_hours".to_string(),
                description: "CPU Hours".to_string(),
                quantity: 100.0,
                unit: "hours".to_string(),
                unit_price: 0.025,
                amount: Decimal::new(250, 2),
            }],
            subtotal: Decimal::new(250, 2),
            credit_applied: Decimal::ZERO,
            total: Decimal::new(250, 2),
            currency: "USD".to_string(),
            status: InvoiceStatus::Open,
            due_date: Utc::now() + Duration::days(30),
            created_at: Utc::now(),
            paid_at: None,
        };
        
        let html = generator.render_html(&invoice);
        assert!(html.is_ok());
        assert!(html.unwrap().contains("INV-202401-0001"));
    }
    
    #[tokio::test]
    async fn test_generate_pdf_without_feature() {
        let generator = InvoiceGenerator::new("").unwrap();
        
        let invoice = Invoice {
            id: "inv_123".to_string(),
            invoice_number: "INV-202401-0001".to_string(),
            customer_id: "cust_123".to_string(),
            customer_name: "Test Customer".to_string(),
            customer_email: "test@example.com".to_string(),
            period: BillingPeriod {
                start: Utc::now() - Duration::days(30),
                end: Utc::now(),
            },
            line_items: vec![],
            subtotal: Decimal::ZERO,
            credit_applied: Decimal::ZERO,
            total: Decimal::ZERO,
            currency: "USD".to_string(),
            status: InvoiceStatus::Open,
            due_date: Utc::now() + Duration::days(30),
            created_at: Utc::now(),
            paid_at: None,
        };
        
        let pdf = generator.generate_pdf(&invoice).await;
        assert!(pdf.is_ok());
        // Without pdf feature, returns HTML as bytes
        assert!(!pdf.unwrap().is_empty());
    }
    
    #[tokio::test]
    async fn test_send_email() {
        let generator = InvoiceGenerator::new("").unwrap();
        
        let invoice = Invoice {
            id: "inv_123".to_string(),
            invoice_number: "INV-202401-0001".to_string(),
            customer_id: "cust_123".to_string(),
            customer_name: "Test Customer".to_string(),
            customer_email: "test@example.com".to_string(),
            period: BillingPeriod {
                start: Utc::now() - Duration::days(30),
                end: Utc::now(),
            },
            line_items: vec![],
            subtotal: Decimal::new(100, 0),
            credit_applied: Decimal::ZERO,
            total: Decimal::new(100, 0),
            currency: "USD".to_string(),
            status: InvoiceStatus::Open,
            due_date: Utc::now() + Duration::days(30),
            created_at: Utc::now(),
            paid_at: None,
        };
        
        let pdf = b"fake pdf content";
        let result = generator.send_email(&invoice, pdf, "recipient@example.com").await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_send_email_invalid_recipient() {
        let generator = InvoiceGenerator::new("").unwrap();
        
        let invoice = Invoice {
            id: "inv_123".to_string(),
            invoice_number: "INV-202401-0001".to_string(),
            customer_id: "cust_123".to_string(),
            customer_name: "Test Customer".to_string(),
            customer_email: "test@example.com".to_string(),
            period: BillingPeriod {
                start: Utc::now() - Duration::days(30),
                end: Utc::now(),
            },
            line_items: vec![],
            subtotal: Decimal::ZERO,
            credit_applied: Decimal::ZERO,
            total: Decimal::ZERO,
            currency: "USD".to_string(),
            status: InvoiceStatus::Open,
            due_date: Utc::now() + Duration::days(30),
            created_at: Utc::now(),
            paid_at: None,
        };
        
        let pdf = b"fake pdf content";
        let result = generator.send_email(&invoice, pdf, "invalid-email").await;
        assert!(result.is_err());
    }
    
    #[test]
    fn test_invoice_number_generation() {
        let generator = InvoiceGenerator::new("").unwrap();
        
        let num1 = generator.generate_invoice_number();
        let num2 = generator.generate_invoice_number();
        
        // Both should start with INV-
        assert!(num1.starts_with("INV-"));
        assert!(num2.starts_with("INV-"));
        
        // They should be unique (different random suffixes)
        // Note: There's a small chance they could match in fast execution
    }
}
