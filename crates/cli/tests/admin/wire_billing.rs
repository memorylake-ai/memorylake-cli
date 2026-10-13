//! Wire-level tests for `billing` and `audit-log`.

use std::fs;

use crate::common::stub::{exchange, exchange_sequence, request_body, request_line};
use crate::common::{assert_failure, assert_success};

const EMPTY_PAGE: &str = r#"{"success":true,"message":"Operation completed successfully","data":{"items":[],"total":0}}"#;
const ALERT: &str = r#"{"success":true,"data":{"enabled":true,"is_default":false,"thresholds":[{"type":"percent_used","value":50}],"notification_channel":{"type":"email","recipients":["ops@example.com"]},"delivery_enabled":true,"status":{"state":"ok"}}}"#;
const DETAILS: &str = r#"{"success":true,"data":{"invoice_id":"in_1","saved":true,"invoice_type":"business","name":"","company_name":"Acme","tax_id":"","address":"1 Main St","contact_email":""}}"#;

#[test]
fn every_billing_read_reaches_its_endpoint() {
    let balance = r#"{"success":true,"data":{"available_tokens":5,"unlimited":false,"billing_mode":"standard","breakdown":[]}}"#;
    let topup = r#"{"success":true,"data":{"enabled":false,"threshold_tokens":0,"amount":{"amount":1000,"currency":"usd"},"tokens":3200000}}"#;
    let order = r#"{"success":true,"data":{"order_id":"o-1","type":"onetime","status":"pending","amount":{"amount":10000,"currency":"usd"},"tokens_granted":32000000,"invoice_id":null,"receipt_url":null,"created_at":1775622625}}"#;
    let invoice = r#"{"success":true,"data":{"invoice_id":"in_1","number":"A-1","status":"paid","total":{"amount":100,"currency":"usd"},"subtotal":{"amount":100,"currency":"usd"},"tax":{"amount":0,"currency":"usd"},"created_at":1,"order_id":null,"invoice_pdf_url":null,"hosted_invoice_url":null}}"#;
    let subscription = r#"{"success":true,"data":{"status":"none","cancel_at_period_end":false}}"#;
    let preview = r#"{"success":true,"data":{"plan_id":2,"applicable":false,"reason":"no_paid_subscription","lines":[]}}"#;
    for (response, args, line) in [
        (
            balance,
            vec!["billing", "balance"],
            "GET /admin/v1/billing/credit-balance",
        ),
        (
            ALERT,
            vec!["billing", "alert", "get"],
            "GET /admin/v1/billing/credit-balance/alert",
        ),
        (
            EMPTY_PAGE,
            vec!["billing", "alert", "events", "--page-size", "5"],
            "GET /admin/v1/billing/credit-balance/alert/events?page_size=5",
        ),
        (
            topup,
            vec!["billing", "auto-topup", "get"],
            "GET /admin/v1/billing/auto-topup",
        ),
        (
            EMPTY_PAGE,
            vec![
                "billing",
                "auto-topup",
                "attempts",
                "--continuation-token",
                "t",
            ],
            "GET /admin/v1/billing/auto-topup/attempts?continuation_token=t",
        ),
        (
            EMPTY_PAGE,
            vec![
                "billing",
                "transactions",
                "--start-time",
                "100",
                "--end-time",
                "200",
            ],
            "GET /admin/v1/billing/credit-transactions?start_time=100&end_time=200",
        ),
        (
            EMPTY_PAGE,
            vec!["billing", "invoice", "list", "--status", "paid,open"],
            "GET /admin/v1/billing/invoices?status=paid&status=open",
        ),
        (
            invoice,
            vec!["billing", "invoice", "get", "in_1"],
            "GET /admin/v1/billing/invoices/in_1",
        ),
        (
            DETAILS,
            vec!["billing", "invoice", "details", "get", "in_1"],
            "GET /admin/v1/billing/invoices/in_1/billing-details",
        ),
        (
            EMPTY_PAGE,
            vec![
                "billing",
                "order",
                "list",
                "--type",
                "auto_topup",
                "--status",
                "failed",
            ],
            "GET /admin/v1/billing/orders?type=auto_topup&status=failed",
        ),
        (
            order,
            vec!["billing", "order", "get", "o-1"],
            "GET /admin/v1/billing/orders/o-1",
        ),
        (
            EMPTY_PAGE,
            vec!["billing", "payment-methods"],
            "GET /admin/v1/billing/payment-methods",
        ),
        (
            subscription,
            vec!["billing", "subscription", "get"],
            "GET /admin/v1/billing/subscription",
        ),
        (
            preview,
            vec!["billing", "subscription", "preview-change", "--plan", "2"],
            "GET /admin/v1/billing/subscription/plan-change-preview?plan_id=2",
        ),
    ] {
        let (request, output) = exchange(response, &args);
        assert_success(&output, &args);
        assert_eq!(
            request_line(&request),
            format!("{line} HTTP/1.1"),
            "{args:?}"
        );
    }
}

#[test]
fn alert_set_replaces_the_configuration() {
    let args = [
        "billing",
        "alert",
        "set",
        "--tokens-remaining",
        "1000000,200000",
        "--recipient",
        "ops@example.com",
        "--recipient",
        "cfo@example.com",
    ];
    let (request, output) = exchange(ALERT, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PUT /admin/v1/billing/credit-balance/alert HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"enabled":true,"thresholds":[{"type":"tokens_remaining","value":1000000},{"type":"tokens_remaining","value":200000}],"notification_channel":{"type":"email","recipients":["ops@example.com","cfo@example.com"]}}"#
    );
}

#[test]
fn alert_set_disabled_without_recipients_notifies_the_owner_only() {
    let args = [
        "billing",
        "alert",
        "set",
        "--percent-used",
        "90",
        "--disabled",
    ];
    let (request, output) = exchange(ALERT, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_body(&request),
        r#"{"enabled":false,"thresholds":[{"type":"percent_used","value":90}]}"#
    );
}

#[test]
fn alert_disable_writes_back_what_it_read_with_only_the_switch_changed() {
    let args = ["billing", "alert", "disable"];
    let (requests, output) = exchange_sequence(&[ALERT, ALERT], &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&requests[0]),
        "GET /admin/v1/billing/credit-balance/alert HTTP/1.1"
    );
    assert_eq!(
        request_line(&requests[1]),
        "PUT /admin/v1/billing/credit-balance/alert HTTP/1.1"
    );
    assert_eq!(
        request_body(&requests[1]),
        r#"{"enabled":false,"thresholds":[{"type":"percent_used","value":50}],"notification_channel":{"type":"email","recipients":["ops@example.com"]}}"#
    );
}

#[test]
fn alert_reset_deletes_the_saved_configuration() {
    let args = ["billing", "alert", "reset"];
    let (request, output) = exchange(ALERT, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "DELETE /admin/v1/billing/credit-balance/alert HTTP/1.1"
    );
}

#[test]
fn billing_details_set_sends_only_what_was_given() {
    let args = [
        "billing",
        "invoice",
        "details",
        "set",
        "in_1",
        "--type",
        "business",
        "--company",
        "Acme",
        "--address",
        "1 Main St",
    ];
    let (request, output) = exchange(DETAILS, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PUT /admin/v1/billing/invoices/in_1/billing-details HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"invoice_type":"business","address":"1 Main St","company_name":"Acme"}"#
    );
}

#[test]
fn an_invoice_pdf_is_written_under_a_safe_default_name() {
    // The stub answers JSON whatever is asked; the bytes are what matter.
    let body = r#"{"success":true,"data":{}}"#;
    let dir = std::env::temp_dir().join(format!("mlcli-pdf-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let out = dir.to_string_lossy().into_owned();
    let args = [
        "billing",
        "invoice",
        "pdf",
        "in_1",
        "--output",
        out.as_str(),
    ];
    let (request, output) = exchange(body, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/billing/invoices/in_1/pdf HTTP/1.1"
    );
    let written = dir.join("invoice-in_1.pdf");
    assert_eq!(fs::read_to_string(&written).unwrap(), body);
    assert!(stdout.contains("invoice-in_1.pdf"), "{stdout}");

    // A second download into the same place is refused without --force,
    // and nothing is left behind.
    let (_, output) = exchange(body, &args);
    let err = assert_failure(&output, &args);
    assert!(err.contains("--force"), "{err}");
    let entries: Vec<_> = fs::read_dir(&dir).unwrap().collect();
    assert_eq!(entries.len(), 1, "no temporary file may remain");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_log_filters_repeat_and_ride_the_query_string() {
    let page = r#"{"success":true,"data":{"items":[{"id":"9","created_at":1,"category":"management","action":"member:change_role","actor":{"type":"api_key","principal_id":"p-1","api_key_id":"42"},"result":"succeeded","reason":"why"}]}}"#;
    let args = [
        "audit",
        "list",
        "--category",
        "management",
        "--action",
        "member:change_role,member:remove",
        "--actor",
        "p-1",
        // The alias `usage records` users know.
        "--member",
        "p-2",
        "--target-type",
        "principal",
        "--target-id",
        "p-3",
        "--result",
        "failed",
        "--start-time",
        "100",
        "--end-time",
        "200",
        "--page-size",
        "100",
    ];
    let (request, output) = exchange(page, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/audit-logs?page_size=100&category=management&action=member%3Achange_role&action=member%3Aremove&actor_id=p-1&actor_id=p-2&target_type=principal&target_id=p-3&result=failed&start_time=100&end_time=200 HTTP/1.1"
    );
    let printed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // The log returns no total, and none is invented; unmodelled fields
    // survive.
    assert!(printed.get("total").is_none(), "{stdout}");
    assert_eq!(printed["items"][0]["reason"], "why");
}
