//! The team's credit, alerts, invoices, orders, and subscription
//! (`/admin/v1/billing/*`).
//!
//! Almost everything here is read-only: buying credit, changing the plan,
//! and managing cards happen in the console. The two writes are the
//! low-balance alert and the buyer details printed on an invoice.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::{Client, Downloaded};
use crate::error::Result;

use super::path;
use super::types::{Page, push_each, push_opt, push_page_query};

/// Highest `percent_used` alert level.
pub const MAX_ALERT_PERCENT: i64 = 99;
/// Highest `tokens_remaining` alert level.
pub const MAX_ALERT_TOKENS: i64 = 1_000_000_000;
/// Most alert levels one configuration may hold.
pub const MAX_ALERT_THRESHOLDS: usize = 5;
/// Most extra alert recipients (the owner is always notified as well).
pub const MAX_ALERT_RECIPIENTS: usize = 5;

/// An amount of money.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    /// In the currency's smallest unit (cents for `usd`); negative for a
    /// credit.
    pub amount: i64,
    /// Lowercase ISO 4217 code.
    pub currency: String,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The team's balance in tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditBalance {
    /// One-time credit plus the unexpired subscription grant.
    pub available_tokens: i64,
    /// `true` when usage is recorded but not charged.
    pub unlimited: bool,
    /// `standard` or `metering_only`.
    pub billing_mode: String,
    /// The balance by source (`subscription`, `one_time`).
    #[serde(default)]
    pub breakdown: Option<Vec<Value>>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One alert level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertThreshold {
    /// `percent_used` or `tokens_remaining`.
    #[serde(rename = "type")]
    pub kind: String,
    /// 1–99 for `percent_used`; 1–1,000,000,000 for `tokens_remaining`.
    pub value: i64,
}

/// Where alerts go besides the team owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertChannel {
    /// `email`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Extra addresses.
    #[serde(default)]
    pub recipients: Option<Vec<String>>,
}

/// The low-balance alert configuration and its state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceAlert {
    /// Whether alerts are sent.
    pub enabled: bool,
    /// `true` when nothing is saved and the defaults apply.
    pub is_default: bool,
    /// Alert levels, all of one type.
    #[serde(default)]
    pub thresholds: Option<Vec<AlertThreshold>>,
    /// Extra recipients.
    pub notification_channel: AlertChannel,
    /// `false` when this deployment does not evaluate alerts at all.
    pub delivery_enabled: bool,
    /// Whether an alert is in effect (`state`: `ok` or `triggered`).
    pub status: Value,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Request body replacing the low-balance alert configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetBalanceAlertRequest {
    /// Whether alerts are sent.
    pub enabled: bool,
    /// One to five levels, all of one type; required even when disabled.
    pub thresholds: Vec<AlertThreshold>,
    /// Extra recipients; `None` notifies the team owner only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_channel: Option<AlertChannel>,
}

impl SetBalanceAlertRequest {
    /// The request that writes `alert` back with only `enabled` changed.
    pub fn toggled(alert: &BalanceAlert, enabled: bool) -> Self {
        let recipients = alert
            .notification_channel
            .recipients
            .clone()
            .unwrap_or_default();
        Self {
            enabled,
            thresholds: alert.thresholds.clone().unwrap_or_default(),
            notification_channel: (!recipients.is_empty()).then(|| AlertChannel {
                kind: alert.notification_channel.kind.clone(),
                recipients: Some(recipients),
            }),
        }
    }
}

/// A fired alert level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceAlertEvent {
    /// Event id.
    pub id: i64,
    /// When it fired, Unix seconds.
    pub fired_at: i64,
    /// The level that fired.
    pub threshold: AlertThreshold,
    /// The balance then, in tokens.
    pub balance_tokens: i64,
    /// Whether the balance has since recovered above the level.
    pub recovered: bool,
    /// Emails sent.
    #[serde(default)]
    pub recipients: Option<Vec<Value>>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The automatic top-up configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoTopup {
    /// Whether the balance is topped up automatically.
    pub enabled: bool,
    /// Top up when the one-time balance falls to this many tokens.
    pub threshold_tokens: i64,
    /// Charged per top-up.
    pub amount: Money,
    /// Tokens each top-up adds.
    pub tokens: i64,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One automatic top-up attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoTopupAttempt {
    /// Attempt id.
    pub id: i64,
    /// `pending`, `succeeded`, `failed`, or `refunded`.
    pub status: String,
    /// Why it failed; `None` unless it did.
    #[serde(default)]
    pub failure_reason: Option<String>,
    /// `balance_check` or `manual`.
    pub triggered_by: String,
    /// The order created for the charge, when one was.
    #[serde(default)]
    pub order_id: Option<String>,
    /// Amount charged or attempted.
    pub amount: Money,
    /// Tokens it adds when it succeeds.
    pub tokens: i64,
    /// Unix seconds.
    pub created_at: i64,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Credit added to the balance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditTransaction {
    /// Transaction id.
    pub id: i64,
    /// Where it came from: `order`, `auto_topup`, `redemption`,
    /// `invite_code`, `subscription_grant`, or `other`.
    pub source: String,
    /// Tokens added.
    pub tokens: i64,
    /// The order behind it, when there is one.
    #[serde(default)]
    pub order_id: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An invoice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invoice {
    /// Invoice id.
    pub invoice_id: String,
    /// Number printed on the document.
    pub number: String,
    /// `draft`, `open`, `paid`, `uncollectible`, or `void`.
    pub status: String,
    /// Total.
    pub total: Money,
    /// Total before tax.
    pub subtotal: Money,
    /// Tax.
    pub tax: Money,
    /// Issue time, Unix seconds.
    pub created_at: i64,
    /// The order it belongs to, when there is one.
    #[serde(default)]
    pub order_id: Option<String>,
    /// The payment provider's PDF link, when it has one.
    #[serde(default)]
    pub invoice_pdf_url: Option<String>,
    /// The payment provider's hosted invoice page, when there is one.
    #[serde(default)]
    pub hosted_invoice_url: Option<String>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The buyer details printed on an invoice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingDetails {
    /// The invoice.
    pub invoice_id: String,
    /// `false` when nothing is saved: the fields are then a suggestion.
    pub saved: bool,
    /// `individual` or `business`.
    pub invoice_type: String,
    /// Person's name.
    pub name: String,
    /// Company name.
    pub company_name: String,
    /// Tax identification number.
    pub tax_id: String,
    /// Billing address.
    pub address: String,
    /// Contact email.
    pub contact_email: String,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Longest billing address.
pub const MAX_BILLING_ADDRESS: usize = 500;
/// Longest name, company name, or contact email.
pub const MAX_BILLING_FIELD: usize = 255;
/// Longest tax id.
pub const MAX_BILLING_TAX_ID: usize = 128;

/// Request body replacing an invoice's buyer details. Fields left out are
/// cleared.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SetBillingDetailsRequest {
    /// `individual` (needs `name`) or `business` (needs `company_name`).
    pub invoice_type: String,
    /// Billing address, 1–500 characters.
    pub address: String,
    /// Person's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Company name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_name: Option<String>,
    /// Tax identification number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_id: Option<String>,
    /// Contact email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_email: Option<String>,
}

/// A payment order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    /// Order number.
    pub order_id: String,
    /// `onetime`, `auto_topup`, or `subscription`.
    #[serde(rename = "type")]
    pub kind: String,
    /// `pending`, `succeeded`, `failed`, `closed`, `refunded`, or
    /// `disputed`.
    pub status: String,
    /// Amount paid or due.
    pub amount: Money,
    /// Tokens it adds once paid.
    pub tokens_granted: i64,
    /// The invoice, when one was issued.
    #[serde(default)]
    pub invoice_id: Option<String>,
    /// The payment provider's receipt page, when there is one.
    #[serde(default)]
    pub receipt_url: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A saved card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentMethod {
    /// e.g. `card`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Used for automatic top-ups and renewals.
    pub is_default: bool,
    /// Fields the CLI does not model yet (brand, last4, expiry).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The team's subscription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    /// `active`, `past_due`, or `none` (no subscription; the rest is then
    /// absent).
    pub status: String,
    /// Whether it ends at the period end instead of renewing.
    pub cancel_at_period_end: bool,
    /// The plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<Value>,
    /// Fields the CLI does not model yet (the current period).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// What switching the subscription to a plan would cost now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanChangePreview {
    /// The target plan, as requested.
    pub plan_id: i64,
    /// `false` when there is nothing to price; `reason` says why.
    pub applicable: bool,
    /// Line items behind the amount due.
    #[serde(default)]
    pub lines: Option<Vec<Value>>,
    /// Fields the CLI does not model yet (amount due, plans, dates).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Query parameters for listing credit transactions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListCreditTransactionsParams {
    /// Only entries at or after this time, Unix seconds.
    pub start_time: Option<i64>,
    /// Only entries before this time, Unix seconds.
    pub end_time: Option<i64>,
    /// Page size, 1–100.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

/// Query parameters for listing orders.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListOrdersParams {
    /// `onetime`, `auto_topup`, or `subscription`.
    pub kind: Option<String>,
    /// Only orders in this state.
    pub status: Option<String>,
    /// Page size, 1–100.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

/// Return the team's balance.
pub fn get_credit_balance(client: &Client) -> Result<CreditBalance> {
    client.get_data(path::CREDIT_BALANCE, &[])
}

/// Return the low-balance alert configuration: the saved one, or the
/// defaults with `is_default` true.
pub fn get_balance_alert(client: &Client) -> Result<BalanceAlert> {
    client.get_data(path::BALANCE_ALERT, &[])
}

/// Replace the low-balance alert configuration.
pub fn set_balance_alert(
    client: &Client,
    request: &SetBalanceAlertRequest,
) -> Result<BalanceAlert> {
    client.put_data_with_headers(path::BALANCE_ALERT, request, &[])
}

/// Restore the default alert configuration (enabled) and return it.
pub fn reset_balance_alert(client: &Client) -> Result<BalanceAlert> {
    client.delete_data(path::BALANCE_ALERT)
}

/// List fired alert levels, newest first.
pub fn list_balance_alert_events(
    client: &Client,
    page_size: Option<u32>,
    continuation_token: Option<&str>,
) -> Result<Page<BalanceAlertEvent>> {
    let mut query = Vec::new();
    push_page_query(&mut query, page_size, continuation_token);
    client.get_data(path::BALANCE_ALERT_EVENTS, &query)
}

/// Return the automatic top-up configuration.
pub fn get_auto_topup(client: &Client) -> Result<AutoTopup> {
    client.get_data(path::AUTO_TOPUP, &[])
}

/// List automatic top-up attempts, newest first, failed ones included.
pub fn list_auto_topup_attempts(
    client: &Client,
    page_size: Option<u32>,
    continuation_token: Option<&str>,
) -> Result<Page<AutoTopupAttempt>> {
    let mut query = Vec::new();
    push_page_query(&mut query, page_size, continuation_token);
    client.get_data(path::AUTO_TOPUP_ATTEMPTS, &query)
}

/// List credit added to the balance, newest first. There is no total.
pub fn list_credit_transactions(
    client: &Client,
    params: &ListCreditTransactionsParams,
) -> Result<Page<CreditTransaction>> {
    let mut query = Vec::new();
    push_opt(&mut query, "start_time", params.start_time.as_ref());
    push_opt(&mut query, "end_time", params.end_time.as_ref());
    push_page_query(
        &mut query,
        params.page_size,
        params.continuation_token.as_deref(),
    );
    client.get_data(path::CREDIT_TRANSACTIONS, &query)
}

/// List invoices, newest first, optionally only those in `statuses`.
pub fn list_invoices(
    client: &Client,
    statuses: &[String],
    page_size: Option<u32>,
    continuation_token: Option<&str>,
) -> Result<Page<Invoice>> {
    let mut query = Vec::new();
    push_each(&mut query, "status", statuses);
    push_page_query(&mut query, page_size, continuation_token);
    client.get_data(path::INVOICES, &query)
}

/// Return one invoice.
pub fn get_invoice(client: &Client, invoice_id: &str) -> Result<Invoice> {
    client.get_data(&path::invoice(invoice_id), &[])
}

/// Stream an invoice's PDF into `writer`. 409 when the payment provider has
/// not produced one yet.
pub fn download_invoice_pdf<W: std::io::Write>(
    client: &Client,
    invoice_id: &str,
    writer: &mut W,
) -> Result<Downloaded> {
    client.download_to(&path::invoice_pdf(invoice_id), writer)
}

/// Return the buyer details printed on an invoice.
pub fn get_billing_details(client: &Client, invoice_id: &str) -> Result<BillingDetails> {
    client.get_data(&path::invoice_billing_details(invoice_id), &[])
}

/// Replace the buyer details printed on a paid invoice.
pub fn set_billing_details(
    client: &Client,
    invoice_id: &str,
    request: &SetBillingDetailsRequest,
) -> Result<BillingDetails> {
    client.put_data_with_headers(&path::invoice_billing_details(invoice_id), request, &[])
}

/// List payment orders, newest first.
pub fn list_orders(client: &Client, params: &ListOrdersParams) -> Result<Page<Order>> {
    let mut query = Vec::new();
    push_opt(&mut query, "type", params.kind.as_ref());
    push_opt(&mut query, "status", params.status.as_ref());
    push_page_query(
        &mut query,
        params.page_size,
        params.continuation_token.as_deref(),
    );
    client.get_data(path::ORDERS, &query)
}

/// Return one order.
pub fn get_order(client: &Client, order_id: &str) -> Result<Order> {
    client.get_data(&path::order(order_id), &[])
}

/// List the saved cards.
pub fn list_payment_methods(client: &Client) -> Result<Page<PaymentMethod>> {
    client.get_data(path::PAYMENT_METHODS, &[])
}

/// Return the team's subscription.
pub fn get_subscription(client: &Client) -> Result<Subscription> {
    client.get_data(path::SUBSCRIPTION, &[])
}

/// Price a switch to `plan_id` without changing anything.
pub fn preview_plan_change(client: &Client, plan_id: i64) -> Result<PlanChangePreview> {
    client.get_data(
        path::PLAN_CHANGE_PREVIEW,
        &[("plan_id", plan_id.to_string())],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_keeps_the_levels_and_recipients() {
        let alert: BalanceAlert = serde_json::from_str(
            r#"{"enabled":true,"is_default":false,"thresholds":[{"type":"percent_used","value":50}],
                "notification_channel":{"type":"email","recipients":["a@example.com"]},
                "delivery_enabled":true,"status":{"state":"ok"}}"#,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&SetBalanceAlertRequest::toggled(&alert, false)).unwrap(),
            r#"{"enabled":false,"thresholds":[{"type":"percent_used","value":50}],"notification_channel":{"type":"email","recipients":["a@example.com"]}}"#
        );
    }

    #[test]
    fn toggling_without_recipients_sends_no_channel() {
        let alert: BalanceAlert = serde_json::from_str(
            r#"{"enabled":false,"is_default":true,"thresholds":[{"type":"percent_used","value":80}],
                "notification_channel":{"type":"email","recipients":[]},
                "delivery_enabled":true,"status":{"state":"ok"}}"#,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&SetBalanceAlertRequest::toggled(&alert, true)).unwrap(),
            r#"{"enabled":true,"thresholds":[{"type":"percent_used","value":80}]}"#
        );
    }
}
