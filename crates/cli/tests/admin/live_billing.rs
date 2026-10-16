//! Live `billing` and `audit-log` tests (require `MEMORYLAKE_API_KEY`).
//!
//! Billing is read-only apart from the low-balance alert, which these tests
//! change and then restore. Nothing here spends money: every command that
//! could is a read, and the plan-change preview is defined to change
//! nothing.

use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use super::live::{login_default, parse, str_field};
use crate::common::{assert_failure, assert_success, require_api_key, run, temp_home};

/// Run a command that must succeed and parse its JSON output.
fn run_json(home: &Path, args: &[&str]) -> serde_json::Value {
    let stdout = assert_success(&run(home, args), args);
    parse(&stdout, &args.join(" "))
}

#[test]
fn every_billing_read_answers_with_the_documented_shape() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let balance = run_json(&home, &["billing", "balance"]);
    assert!(balance["available_tokens"].is_i64(), "{balance}");
    assert!(["standard", "metering_only"].contains(&str_field(&balance, "billing_mode").as_str()));

    let topup = run_json(&home, &["billing", "auto-topup", "get"]);
    assert!(topup["enabled"].is_boolean() && topup["amount"]["currency"].is_string());
    for args in [
        ["billing", "auto-topup", "attempts", "--page-size", "1"].as_slice(),
        ["billing", "alert", "events", "--page-size", "1"].as_slice(),
        ["billing", "transactions", "--page-size", "1"].as_slice(),
        ["billing", "invoice", "list", "--page-size", "1"].as_slice(),
        ["billing", "order", "list", "--page-size", "1"].as_slice(),
    ] {
        let page = run_json(&home, args);
        assert!(page["items"].is_array(), "{args:?}: {page}");
    }
    let methods = run_json(&home, &["billing", "payment-methods"]);
    assert!(methods["items"].is_array());

    // Every order listed can be read back by id.
    let orders = run_json(&home, &["billing", "order", "list", "--page-size", "1"]);
    if let Some(order) = orders["items"].as_array().and_then(|o| o.first()) {
        let id = str_field(order, "order_id");
        let read = run_json(&home, &["billing", "order", "get", id.as_str()]);
        assert_eq!(read["order_id"], order["order_id"]);
    }

    let subscription = run_json(&home, &["billing", "subscription", "get"]);
    let status = str_field(&subscription, "status");
    assert!(["active", "past_due", "none"].contains(&status.as_str()));
    // Pricing a change is a pure read; whichever way it answers, it must
    // answer for the requested plan.
    let preview = run_json(
        &home,
        &["billing", "subscription", "preview-change", "--plan", "1"],
    );
    assert_eq!(preview["plan_id"], 1);
    assert!(preview["applicable"].is_boolean());

    // Unknown invoices and orders are not found, not errors of another kind.
    for args in [
        ["billing", "invoice", "get", "mlcli-no-such-invoice"],
        ["billing", "order", "get", "mlcli-no-such-order"],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains("NOT_FOUND"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn the_low_balance_alert_round_trips_and_resets() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let before = run_json(&home, &["billing", "alert", "get"]);
    let set = [
        "billing",
        "alert",
        "set",
        "--tokens-remaining",
        "123456,65432",
        "--recipient",
        "mlcli-live@example.com",
    ];
    let saved = run_json(&home, &set);
    assert_eq!(saved["is_default"], false);
    assert_eq!(saved["enabled"], true);
    // The server stores the levels in its own order.
    let mut values: Vec<i64> = saved["thresholds"]
        .as_array()
        .expect("thresholds")
        .iter()
        .map(|t| {
            assert_eq!(t["type"], "tokens_remaining");
            t["value"].as_i64().expect("value")
        })
        .collect();
    values.sort_unstable();
    assert_eq!(values, vec![65432, 123456]);
    assert_eq!(
        saved["notification_channel"]["recipients"][0],
        "mlcli-live@example.com"
    );

    // disable/enable keep everything but the switch.
    let disabled = run_json(&home, &["billing", "alert", "disable"]);
    assert_eq!(disabled["enabled"], false);
    assert_eq!(disabled["thresholds"], saved["thresholds"]);
    assert_eq!(
        disabled["notification_channel"],
        saved["notification_channel"]
    );
    let enabled = run_json(&home, &["billing", "alert", "enable"]);
    assert_eq!(enabled["enabled"], true);
    assert_eq!(enabled["thresholds"], saved["thresholds"]);

    // Reset restores the defaults — and what `get` showed before, if the
    // team was on them (any other saved configuration cannot be restored
    // by a test, which is why this checks is_default rather than equality).
    let reset = run_json(&home, &["billing", "alert", "reset"]);
    assert_eq!(reset["is_default"], true);
    assert_eq!(reset["enabled"], true);
    if before["is_default"] == true {
        // `status` is the live balance and the last trigger, which every
        // other test spending tokens moves; only the configuration is
        // restored.
        let config = |mut alert: serde_json::Value| {
            alert
                .as_object_mut()
                .expect("alert object")
                .remove("status");
            alert
        };
        assert_eq!(
            config(run_json(&home, &["billing", "alert", "get"])),
            config(before.clone())
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn the_audit_log_records_what_this_key_just_did() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    // Something audited, done by this key: create and delete a custom role.
    let role = run_json(&home, &["role", "create", "--label", "mlcli-audit"]);
    let key = str_field(&role, "key");
    let args = ["role", "delete", key.as_str()];
    assert_success(&run(&home, &args), &args);

    // The delete row can land a second after the listing's default window ends
    // (the server stamps rows and the window with whole seconds, possibly from
    // different instances), so poll briefly instead of reading once.
    let args = [
        "audit",
        "list",
        "--category",
        "management",
        "--target-id",
        key.as_str(),
        "--page-size",
        "10",
    ];
    let deadline = Instant::now() + Duration::from_secs(15);
    let page = loop {
        let page = run_json(&home, &args);
        let actions: Vec<String> = page["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter_map(|e| e["action"].as_str().map(str::to_owned))
            .collect();
        let complete = ["custom_role:create", "custom_role:delete"]
            .iter()
            .all(|want| actions.iter().any(|a| a == want));
        if complete {
            break page;
        }
        assert!(
            Instant::now() < deadline,
            "the role's lifecycle must be in the log: {actions:?}"
        );
        thread::sleep(Duration::from_secs(1));
    };
    let items = page["items"].as_array().expect("items");
    for entry in items {
        assert_eq!(entry["result"], "succeeded");
        // Done with an API key, so the actor is the key acting as its owner.
        assert_eq!(entry["actor"]["type"], "api_key");
        assert!(entry["actor"]["api_key_id"].is_string(), "{entry}");
    }
    // The log reports no total; the field must not be invented.
    assert!(page.get("total").is_none(), "{page}");

    // The window rule is enforced server-side too.
    let args = [
        "audit",
        "list",
        "--start-time",
        "1",
        "--end-time",
        "1791801121",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("92 days"), "{err}");
    let _ = fs::remove_dir_all(&home);
}
