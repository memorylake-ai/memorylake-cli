//! Live team-management tests (require `MEMORYLAKE_API_KEY`).
//!
//! The key must belong to a team OWNER on an org team: `team rename` is
//! owner-only, and virtual members cannot exist on a personal team.
//!
//! These assert SEMANTICS, not just exit codes: a minted key must actually
//! authenticate, a rotated-away key must actually stop working, a role change
//! must be visible on the roster, and a virtual member's key must act with
//! that member's identity and limits.
//!
//! Invitation writes are OPT-IN: creating one sends a real email and spends
//! the team's daily invitation cap, so that test only runs when
//! MEMORYLAKE_INVITE_EMAIL supplies an inbox (env or .env, never committed)
//! and skips silently otherwise.
//!
//! Every object these tests create carries a unique `mlcli-` name and is
//! removed before the test ends; an assertion failure mid-test can leave at
//! most one scratch key or virtual member behind, named clearly enough to
//! delete by hand.

use std::fs;
use std::path::Path;
use std::process::Output;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run, temp_home,
    unique_name,
};

/// Log `api_key` in under `profile`, returning the raw process output.
///
/// `auth login` VALIDATES the key against the API before storing it, which is
/// exactly what makes it the probe for "does this key still authenticate?".
pub(super) fn login(home: &Path, api_key: &str, profile: &str) -> Output {
    let base_url = live_base_url();
    let args = login_args(api_key, profile, base_url.as_deref());
    run(home, &args)
}

pub(super) fn login_default(home: &Path, api_key: &str) {
    assert_success(&login(home, api_key, "default"), &["auth", "login"]);
}

/// Assert that `api_key` authenticates, and return the team it sees.
///
/// ⚠️ Runs in a THROWAWAY home. `auth login` switches the active profile to
/// the profile it just wrote, so probing a freshly minted key inside the
/// test's main home would silently re-identify every later bare command as
/// that key — which is how an owner-side `member remove` once became the
/// member removing itself.
fn assert_key_authenticates(api_key: &str) -> serde_json::Value {
    let probe_home = temp_home();
    assert_success(&login(&probe_home, api_key, "probe"), &["login probe"]);
    let args = ["team", "get"];
    let stdout = assert_success(&run(&probe_home, &args), &args);
    let team = parse(&stdout, "team via probed key");
    let _ = fs::remove_dir_all(&probe_home);
    team
}

/// Assert that `api_key` no longer authenticates (login validates the key).
fn assert_key_rejected(api_key: &str) {
    let probe_home = temp_home();
    assert_failure(&login(&probe_home, api_key, "probe"), &["login probe"]);
    let _ = fs::remove_dir_all(&probe_home);
}

/// Parse a command's pretty-JSON stdout.
pub(super) fn parse(stdout: &str, what: &str) -> serde_json::Value {
    serde_json::from_str(stdout).unwrap_or_else(|err| panic!("parse {what} JSON ({err}): {stdout}"))
}

pub(super) fn str_field(value: &serde_json::Value, field: &str) -> String {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("response missing `{field}`: {value}"))
        .to_string()
}

pub(super) fn unix_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}

/// Report whether the logged-in key's team is a personal (single-user) team.
///
/// The suite runs against whatever team the CI secret or the operator's key
/// belongs to, and two of the endpoints are defined to REFUSE personal teams.
/// Tests covering those branch on this instead of assuming an org.
pub(super) fn team_is_personal(home: &Path) -> bool {
    let args = ["team", "get"];
    let stdout = assert_success(&run(home, &args), &args);
    str_field(&parse(&stdout, "team"), "type") == "personal"
}

#[test]
fn every_read_endpoint_answers_and_the_team_joins_its_roster() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = ["team", "get"];
    let stdout = assert_success(&run(&home, &args), &args);
    let team = parse(&stdout, "team");
    assert!(!str_field(&team, "id").is_empty());
    assert!(!str_field(&team, "name").is_empty());
    // The suite requires an owner key, so the key's own standing must say so.
    assert_eq!(str_field(&team, "caller_role"), "tenant_owner");

    // owner_principal_id is documented to join to the members list — verify
    // the join actually holds on real data.
    let owner_principal = str_field(&team, "owner_principal_id");
    let args = ["member", "list"];
    let stdout = assert_success(&run(&home, &args), &args);
    let roster = parse(&stdout, "member list");
    let items = roster
        .get("items")
        .and_then(|v| v.as_array())
        .expect("member list has items");
    assert!(
        items
            .iter()
            .any(|m| str_field(m, "principal_id") == owner_principal),
        "team.owner_principal_id {owner_principal} not on the roster: {stdout}"
    );

    for args in [
        ["api-key", "list"].as_slice(),
        ["invitation", "list"].as_slice(),
    ] {
        let stdout = assert_success(&run(&home, args), args);
        let page = parse(&stdout, "list");
        assert!(
            page.get("items").is_some_and(|v| v.is_array()),
            "{args:?} returned no items array: {stdout}"
        );
    }

    let args = ["usage"];
    let stdout = assert_success(&run(&home, &args), &args);
    let usage = parse(&stdout, "usage");
    assert!(
        usage.get("quota").is_some(),
        "usage missing quota: {stdout}"
    );
    assert!(
        usage.get("totals").is_some(),
        "usage missing totals: {stdout}"
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn the_role_catalog_matches_what_the_writes_enforce() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = ["role", "list"];
    let output = run(&home, &args);
    // GET /admin/v1/roles ships with apiservice#73. Until that deploys, the
    // path answers the envelope 404 — report it and stop rather than fail a
    // suite that cannot see the endpoint yet. Once deployed, this branch goes
    // dead and the assertions below take over for good.
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        assert!(
            err.contains("404"),
            "role list failed for a reason other than not-yet-deployed: {err}"
        );
        eprintln!("role list: endpoint not deployed yet (404); skipping the catalog assertions");
        let _ = fs::remove_dir_all(&home);
        return;
    }
    let stdout = assert_success(&output, &args);
    let catalog = parse(&stdout, "role list");
    let roles = catalog.get("roles").and_then(|v| v.as_array()).unwrap();
    assert!(roles.len() >= 3, "built-ins missing: {stdout}");

    // The catalog's promises must match the write endpoints' behaviour: the
    // owner row says unassignable, and the built-ins lead in fixed order.
    assert_eq!(str_field(&roles[0], "key"), "tenant_owner");
    assert_eq!(roles[0].get("assignable"), Some(&serde_json::json!(false)));
    assert_eq!(str_field(&roles[1], "key"), "tenant_admin");
    assert_eq!(
        roles[1].get("admin_grant_only"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(str_field(&roles[2], "key"), "tenant_member");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn usage_honours_the_period_and_its_cap() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    // An explicit window is echoed back as the period the totals cover.
    let args = [
        "usage",
        "--start-date",
        "2026-08-01",
        "--end-date",
        "2026-08-27",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let usage = parse(&stdout, "usage");
    let period = usage.get("period").expect("usage has period");
    assert_eq!(str_field(period, "start_date"), "2026-08-01");
    assert_eq!(str_field(period, "end_date"), "2026-08-27");

    // A window over 92 days is a client error, surfaced with its error code.
    let args = [
        "usage",
        "--start-date",
        "2026-01-01",
        "--end-date",
        "2026-08-27",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("92") && err.contains("INVALID_ARGUMENT"),
        "cap violation not surfaced: {err}"
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_key_works_until_rotated_away_and_dies_on_revoke() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let name = unique_name("key");

    // Create: the one response that carries the full key.
    let args = ["api-key", "create", "--name", name.as_str()];
    let stdout = assert_success(&run(&home, &args), &args);
    let created = parse(&stdout, "api-key create");
    let id = str_field(&created, "id");
    let first_key = str_field(&created, "key");
    assert!(
        first_key.starts_with("sk-"),
        "created key has no sk- prefix: {first_key}"
    );

    // The minted key AUTHENTICATES, and lands in the same team.
    let team = assert_key_authenticates(&first_key);
    assert!(
        !str_field(&team, "id").is_empty(),
        "minted key sees no team: {team}"
    );

    // Read endpoints know it, but never return the key itself.
    let get_args = ["api-key", "get", id.as_str()];
    let stdout = assert_success(&run(&home, &get_args), &get_args);
    assert!(
        !stdout.contains(&first_key),
        "get leaked the full key: {stdout}"
    );

    let list_args = ["api-key", "list", "--name", name.as_str()];
    let stdout = assert_success(&run(&home, &list_args), &list_args);
    assert!(stdout.contains(&id), "list --name missed the key: {stdout}");

    // Rotate mints different material under the same id…
    let rotate_args = ["api-key", "rotate", id.as_str()];
    let stdout = assert_success(&run(&home, &rotate_args), &rotate_args);
    let rotated = parse(&stdout, "api-key rotate");
    assert_eq!(str_field(&rotated, "id"), id);
    let second_key = str_field(&rotated, "key");
    assert_ne!(second_key, first_key, "rotate returned the old key");

    // …the OLD key stops working immediately, the NEW one works.
    assert_key_rejected(&first_key);
    assert_key_authenticates(&second_key);

    // Revoke: the id is gone and the material no longer authenticates.
    let revoke_args = ["api-key", "revoke", id.as_str()];
    let stdout = assert_success(&run(&home, &revoke_args), &revoke_args);
    assert!(stdout.contains("revoked"), "{stdout}");
    assert_failure(&run(&home, &get_args), &get_args);
    assert_key_rejected(&second_key);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_virtual_members_key_acts_as_that_member() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    // The server caps a virtual member's display_name at 20 characters, so the
    // shared unique_name (pid + counter + nanos) does not fit. Nanos alone,
    // truncated, is unique enough for a scratch object that gets removed.
    let name = format!("mlcli-vm-{}", unix_now() % 10_000_000_000);

    // Create a virtual member…
    let args = [
        "member",
        "create",
        "--name",
        name.as_str(),
        "--role",
        "tenant_member",
    ];

    // On a personal team the endpoint is DEFINED to refuse — pin that
    // contract instead of skipping. The full lifecycle needs an org team.
    if team_is_personal(&home) {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains("STATE_NOT_READY") && err.contains("409"),
            "personal-team refusal not surfaced: {err}"
        );
        let _ = fs::remove_dir_all(&home);
        return;
    }

    let stdout = assert_success(&run(&home, &args), &args);
    let member = parse(&stdout, "member create");
    let principal_id = str_field(&member, "principal_id");
    assert_eq!(str_field(&member, "member_type"), "virtual");

    // …change its role, and prove the change on the roster, not just the
    // write's exit code.
    let role_args = [
        "member",
        "set-role",
        principal_id.as_str(),
        "--role",
        "tenant_admin",
    ];
    assert_success(&run(&home, &role_args), &role_args);
    let list_args = ["member", "list", "--name", name.as_str()];
    let stdout = assert_success(&run(&home, &list_args), &list_args);
    let roster = parse(&stdout, "member list");
    let row = roster
        .get("items")
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items
                .iter()
                .find(|m| str_field(m, "principal_id") == principal_id)
        })
        .unwrap_or_else(|| panic!("virtual member not on the roster: {stdout}"));
    assert_eq!(
        str_field(row, "role"),
        "tenant_admin",
        "set-role did not stick"
    );

    // A usage cap set by `member update` is what `member get` then reports,
    // and 0 removes it again.
    let cap_args = [
        "member",
        "update",
        principal_id.as_str(),
        "--max-tokens",
        "5000",
    ];
    let stdout = assert_success(&run(&home, &cap_args), &cap_args);
    assert_eq!(
        parse(&stdout, "member update").get("max_tokens"),
        Some(&serde_json::json!(5000)),
        "update did not report the cap: {stdout}"
    );
    let get_args = ["member", "get", principal_id.as_str()];
    let stdout = assert_success(&run(&home, &get_args), &get_args);
    assert_eq!(
        parse(&stdout, "member get").get("max_tokens"),
        Some(&serde_json::json!(5000)),
        "cap did not stick: {stdout}"
    );
    let uncap_args = [
        "member",
        "update",
        principal_id.as_str(),
        "--max-tokens",
        "0",
    ];
    let stdout = assert_success(&run(&home, &uncap_args), &uncap_args);
    assert!(
        parse(&stdout, "member uncap")
            .get("max_tokens")
            .is_none_or(|v| v.is_null()),
        "0 did not remove the cap: {stdout}"
    );
    let virtual_args = [
        "member",
        "list",
        "--type",
        "virtual",
        "--name",
        name.as_str(),
    ];
    let stdout = assert_success(&run(&home, &virtual_args), &virtual_args);
    assert!(
        stdout.contains(&principal_id),
        "--type virtual missed it: {stdout}"
    );

    // …issue it a key, and prove the key acts AS THE MEMBER: it carries the
    // member's role, and it hits the member's ceiling — team rename is
    // owner-only, so the admin-role key must be refused. The attempted name is
    // the CURRENT name so that an authorization bug cannot damage anything.
    let key_name = unique_name("vmkey");
    let key_args = [
        "api-key",
        "create",
        "--name",
        key_name.as_str(),
        "--member",
        principal_id.as_str(),
    ];
    let stdout = assert_success(&run(&home, &key_args), &key_args);
    let minted = parse(&stdout, "member key create");
    let key_id = str_field(&minted, "id");
    let vm_key = str_field(&minted, "key");

    let team = assert_key_authenticates(&vm_key);
    assert_eq!(
        str_field(&team, "caller_role"),
        "tenant_admin",
        "the key does not act as the virtual member"
    );
    // The member key hits the MEMBER's ceiling: team rename is owner-only.
    // In its own home so the main home stays identified as the owner; the
    // attempted name is the CURRENT name so an authorization bug that let it
    // through could not damage anything.
    let vm_home = temp_home();
    assert_success(&login(&vm_home, &vm_key, "vm"), &["login vm"]);
    let current_name = str_field(&team, "name");
    let rename_args = ["team", "rename", "--name", current_name.as_str()];
    let err = assert_failure(&run(&vm_home, &rename_args), &rename_args);
    assert!(
        err.contains("403"),
        "owner-only op not refused for the member key: {err}"
    );
    let _ = fs::remove_dir_all(&vm_home);

    // Removing the member DISABLES its keys — the server's documented promise.
    // The key is deliberately still live at removal time so the promise is
    // what's tested.
    let remove_args = ["member", "remove", principal_id.as_str()];
    let stdout = assert_success(&run(&home, &remove_args), &remove_args);
    assert!(stdout.contains("removed"), "{stdout}");
    assert_key_rejected(&vm_key);

    let stdout = assert_success(&run(&home, &list_args), &list_args);
    assert!(
        !stdout.contains(&principal_id),
        "removed member still listed: {stdout}"
    );

    // Clean up the disabled key row.
    let revoke_args = ["api-key", "revoke", key_id.as_str()];
    assert_success(&run(&home, &revoke_args), &revoke_args);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_idempotent_create_replays_without_the_secret() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let name = unique_name("idem");
    let idem = format!("mlcli-idem-{}", unix_now());

    let args = [
        "api-key",
        "create",
        "--name",
        name.as_str(),
        "--idempotency-key",
        idem.as_str(),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let first = parse(&stdout, "first create");
    let id = str_field(&first, "id");
    assert!(str_field(&first, "key").starts_with("sk-"));

    // The exact same command again: no second key, and — the server's
    // documented redaction — no secret in the replay. The server currently
    // renders the redacted field as `"key": ""` (the Go DTO lacks omitempty),
    // so absent, null, and empty all count as redacted.
    let stdout = assert_success(&run(&home, &args), &args);
    let replay = parse(&stdout, "replayed create");
    assert_eq!(str_field(&replay, "id"), id, "replay minted a second key");
    assert!(
        replay
            .get("key")
            .is_none_or(|v| v.is_null() || v.as_str() == Some("")),
        "replay leaked the secret: {stdout}"
    );

    let revoke_args = ["api-key", "revoke", id.as_str()];
    assert_success(&run(&home, &revoke_args), &revoke_args);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn pagination_walks_every_page_and_expiry_is_reported() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let tag = format!("mlcli-pg-{}", unix_now());
    let name_a = format!("{tag}-a");
    let name_b = format!("{tag}-b");
    let expires_at = ((unix_now() / 1_000_000_000) + 3600).to_string();

    // Two keys sharing a filterable tag; one with an expiry.
    let args = ["api-key", "create", "--name", name_a.as_str()];
    let stdout = assert_success(&run(&home, &args), &args);
    let id_a = str_field(&parse(&stdout, "create a"), "id");
    let args = [
        "api-key",
        "create",
        "--name",
        name_b.as_str(),
        "--expires-at",
        expires_at.as_str(),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let id_b = str_field(&parse(&stdout, "create b"), "id");

    // The expiry made it through and is reported by the read endpoint.
    let get_args = ["api-key", "get", id_b.as_str()];
    let stdout = assert_success(&run(&home, &get_args), &get_args);
    let fetched = parse(&stdout, "get b");
    assert_eq!(
        fetched.get("expires_at").and_then(|v| v.as_i64()),
        expires_at.parse::<i64>().ok(),
        "expires_at not honoured: {stdout}"
    );

    // Page 1 of 2: one item, a total of two, and a token onwards.
    let list_args = [
        "api-key",
        "list",
        "--name",
        tag.as_str(),
        "--page-size",
        "1",
    ];
    let stdout = assert_success(&run(&home, &list_args), &list_args);
    let page1 = parse(&stdout, "page 1");
    let items1 = page1.get("items").and_then(|v| v.as_array()).unwrap();
    assert_eq!(items1.len(), 1, "page_size ignored: {stdout}");
    assert_eq!(page1.get("total").and_then(|v| v.as_i64()), Some(2));
    let token = str_field(&page1, "continuation_token");

    // Page 2 of 2: the OTHER item, and no token past the end.
    let list_args = [
        "api-key",
        "list",
        "--name",
        tag.as_str(),
        "--page-size",
        "1",
        "--continuation-token",
        token.as_str(),
    ];
    let stdout = assert_success(&run(&home, &list_args), &list_args);
    let page2 = parse(&stdout, "page 2");
    let items2 = page2.get("items").and_then(|v| v.as_array()).unwrap();
    assert_eq!(items2.len(), 1, "page 2 wrong size: {stdout}");
    assert!(
        page2.get("continuation_token").is_none_or(|v| v.is_null()),
        "token past the last page: {stdout}"
    );
    let seen: Vec<String> = [&items1[0], &items2[0]]
        .iter()
        .map(|m| str_field(m, "id"))
        .collect();
    let mut expected = [id_a.clone(), id_b.clone()];
    expected.sort();
    let mut seen_sorted = seen.clone();
    seen_sorted.sort();
    assert_eq!(
        seen_sorted, expected,
        "pages repeated or skipped an item: {seen:?}"
    );

    for id in [id_a, id_b] {
        let revoke_args = ["api-key", "revoke", id.as_str()];
        assert_success(&run(&home, &revoke_args), &revoke_args);
    }

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_invitation_is_created_pending_and_dies_on_revoke() {
    // Opt-in: creating an invitation emails a real inbox and spends the
    // team's daily invitation cap, so this runs only when the operator
    // supplies an address via MEMORYLAKE_INVITE_EMAIL (env or .env — both
    // stay out of git). The address itself must never appear in this file.
    crate::common::load_dotenv();
    let Some(email) = std::env::var("MEMORYLAKE_INVITE_EMAIL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        eprintln!("MEMORYLAKE_INVITE_EMAIL not set; skipping the invitation live test");
        return;
    };
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    // Create: the invitee address comes back normalised, the state is
    // pending, and — the DTO's security promise — no invite token is
    // anywhere in the response.
    let args = [
        "invitation",
        "create",
        "--email",
        &email,
        "--role",
        "tenant_member",
    ];

    // Personal teams cannot invite; pin the refusal and stop (no email sent).
    if team_is_personal(&home) {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains("STATE_NOT_READY") && err.contains("409"),
            "personal-team refusal not surfaced: {err}"
        );
        let _ = fs::remove_dir_all(&home);
        return;
    }

    let stdout = assert_success(&run(&home, &args), &args);
    let invitation = parse(&stdout, "invitation create");
    let id = str_field(&invitation, "id");
    assert_eq!(str_field(&invitation, "email"), email.to_lowercase());
    assert_eq!(str_field(&invitation, "status"), "pending");
    assert!(
        !stdout.contains("token"),
        "invitation response must not carry the accept token: {stdout}"
    );

    // It reads back by id, and the email filter finds it.
    let get_args = ["invitation", "get", id.as_str()];
    let stdout = assert_success(&run(&home, &get_args), &get_args);
    assert_eq!(
        str_field(&parse(&stdout, "invitation get"), "status"),
        "pending"
    );
    let email_args = ["invitation", "list", "--email", email.as_str()];
    let stdout = assert_success(&run(&home, &email_args), &email_args);
    assert!(stdout.contains(&id), "email filter missed it: {stdout}");

    // It shows up under the pending filter…
    let list_args = ["invitation", "list", "--status", "pending"];
    let stdout = assert_success(&run(&home, &list_args), &list_args);
    assert!(stdout.contains(&id), "pending filter missed it: {stdout}");

    // …revoke kills it, and the state filters agree.
    let revoke_args = ["invitation", "revoke", id.as_str()];
    let stdout = assert_success(&run(&home, &revoke_args), &revoke_args);
    assert!(stdout.contains("revoked"), "{stdout}");
    let stdout = assert_success(&run(&home, &list_args), &list_args);
    assert!(
        !stdout.contains(&id),
        "revoked invitation still pending: {stdout}"
    );
    let list_args = ["invitation", "list", "--status", "revoked"];
    let stdout = assert_success(&run(&home, &list_args), &list_args);
    assert!(stdout.contains(&id), "revoked filter missed it: {stdout}");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn team_rename_roundtrip() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = ["team", "get"];
    let stdout = assert_success(&run(&home, &args), &args);
    let original = str_field(&parse(&stdout, "team"), "name");

    // Rename and rename back immediately, THEN assert — a failed assertion in
    // between would strand the team under the scratch name.
    let scratch = format!("{original} [cli-live]");
    let rename_args = ["team", "rename", "--name", scratch.as_str()];
    let renamed_output = run(&home, &rename_args);
    let restore_args = ["team", "rename", "--name", original.as_str()];
    let restored_output = run(&home, &restore_args);

    let renamed = parse(
        &assert_success(&renamed_output, &rename_args),
        "team rename",
    );
    assert_eq!(str_field(&renamed, "name"), scratch);
    let restored = parse(
        &assert_success(&restored_output, &restore_args),
        "team restore",
    );
    assert_eq!(str_field(&restored, "name"), original);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_api_key_update_changes_what_the_key_may_do() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let name = unique_name("upd");

    let args = ["api-key", "create", "--name", name.as_str()];
    let stdout = assert_success(&run(&home, &args), &args);
    let created = parse(&stdout, "api-key create");
    let id = str_field(&created, "id");
    let key = str_field(&created, "key");
    assert_key_authenticates(&key);

    // Pinned to a documentation-only address (RFC 5737), the key must stop
    // working from here; lifting the restriction must bring it back. Each
    // write is asserted only after the next one has undone it, so a failure
    // cannot strand the key locked out.
    let pin_args = [
        "api-key",
        "update",
        id.as_str(),
        "--allowed-ip",
        "192.0.2.1",
    ];
    let pinned = run(&home, &pin_args);
    let probe_home = temp_home();
    let pinned_works = login(&probe_home, &key, "probe").status.success();
    let _ = fs::remove_dir_all(&probe_home);
    let lift_args = ["api-key", "update", id.as_str(), "--no-ip-restriction"];
    let lifted = run(&home, &lift_args);
    let stdout = assert_success(&pinned, &pin_args);
    assert!(
        stdout.contains("192.0.2.1"),
        "restriction not echoed: {stdout}"
    );
    assert!(!pinned_works, "an IP-restricted key still authenticated");
    let stdout = assert_success(&lifted, &lift_args);
    assert_eq!(
        parse(&stdout, "lifted").get("allowed_ips"),
        Some(&serde_json::json!([])),
        "{stdout}"
    );
    assert_key_authenticates(&key);

    // Disabled, the key is refused; enabled again, it works.
    let disable_args = ["api-key", "update", id.as_str(), "--status", "disabled"];
    let stdout = assert_success(&run(&home, &disable_args), &disable_args);
    assert_eq!(str_field(&parse(&stdout, "disabled"), "status"), "disabled");
    assert_key_rejected(&key);
    let list_args = [
        "api-key",
        "list",
        "--status",
        "disabled",
        "--name",
        name.as_str(),
    ];
    let stdout = assert_success(&run(&home, &list_args), &list_args);
    assert!(
        stdout.contains(&id),
        "--status disabled missed it: {stdout}"
    );
    let enable_args = ["api-key", "update", id.as_str(), "--status", "enabled"];
    assert_success(&run(&home, &enable_args), &enable_args);
    assert_key_authenticates(&key);

    // A rename is what `get` reports afterwards.
    let renamed = format!("{name}-r");
    let rename_args = ["api-key", "update", id.as_str(), "--name", renamed.as_str()];
    assert_success(&run(&home, &rename_args), &rename_args);
    let get_args = ["api-key", "get", id.as_str()];
    let stdout = assert_success(&run(&home, &get_args), &get_args);
    let fetched = parse(&stdout, "api-key get");
    assert_eq!(str_field(&fetched, "name"), renamed);
    assert_eq!(
        str_field(&fetched, "key_suffix"),
        key[key.len() - 4..],
        "key_suffix does not match the key"
    );

    let revoke_args = ["api-key", "revoke", id.as_str()];
    assert_success(&run(&home, &revoke_args), &revoke_args);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn usage_summary_records_and_the_daily_series_agree_on_shape() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = ["usage", "summary"];
    let stdout = assert_success(&run(&home, &args), &args);
    let summary = parse(&stdout, "usage summary");
    assert!(
        matches!(str_field(&summary, "scope").as_str(), "team" | "self"),
        "{stdout}"
    );
    for window in ["today", "last_7_days", "month_to_date"] {
        assert!(
            summary
                .get(window)
                .and_then(|w| w.get("used_tokens"))
                .is_some(),
            "{window} missing: {stdout}"
        );
    }

    // A three-day period with a daily series has exactly three buckets, idle
    // days included — the documented promise.
    let args = [
        "usage",
        "--start-date",
        "2026-10-01",
        "--end-date",
        "2026-10-03",
        "--daily",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let report = parse(&stdout, "usage --daily");
    let buckets = report.get("buckets").and_then(|v| v.as_array()).unwrap();
    let dates: Vec<_> = buckets.iter().map(|b| str_field(b, "date")).collect();
    assert_eq!(dates, ["2026-10-01", "2026-10-02", "2026-10-03"]);

    let args = ["usage", "records", "--page-size", "1"];
    let stdout = assert_success(&run(&home, &args), &args);
    let page = parse(&stdout, "usage records");
    assert!(page.get("items").is_some_and(|v| v.is_array()), "{stdout}");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_created_team_dies_by_its_own_key() {
    // Opt-in: every run creates a real org team, which counts toward the
    // owner's team limit until it is deleted. Set
    // MEMORYLAKE_LIVE_TEAM_LIFECYCLE=1 (env or .env) to run it.
    crate::common::load_dotenv();
    if std::env::var("MEMORYLAKE_LIVE_TEAM_LIFECYCLE").as_deref() != Ok("1") {
        eprintln!("MEMORYLAKE_LIVE_TEAM_LIFECYCLE not set; skipping the team lifecycle live test");
        return;
    }
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let args = ["team", "get"];
    let own_team = str_field(
        &parse(&assert_success(&run(&home, &args), &args), "team"),
        "id",
    );

    let name = format!("mlcli-team-{}", unix_now() % 10_000_000_000);
    let args = ["team", "create", "--name", name.as_str(), "--issue-api-key"];
    let stdout = assert_success(&run(&home, &args), &args);
    let created = parse(&stdout, "team create");
    let team = created.get("team").expect("team");
    let team_id = str_field(team, "id");
    assert_eq!(str_field(team, "type"), "org");
    assert_ne!(team_id, own_team, "create returned the caller's own team");
    let team_key = str_field(created.get("api_key").expect("api_key"), "key");

    // The issued key lands in the NEW team as its owner…
    let seen = assert_key_authenticates(&team_key);
    assert_eq!(str_field(&seen, "id"), team_id);
    assert_eq!(str_field(&seen, "caller_role"), "tenant_owner");

    // …and deleting through it takes the team and the key with it, while
    // the creating key's own team is untouched.
    let team_home = temp_home();
    assert_success(&login(&team_home, &team_key, "lab"), &["login lab"]);
    let wrong = ["team", "delete", "--confirm", own_team.as_str()];
    let err = assert_failure(&run(&team_home, &wrong), &wrong);
    assert!(err.contains("does not match"), "{err}");
    let args = ["team", "delete", "--confirm", team_id.as_str()];
    let stdout = assert_success(&run(&team_home, &args), &args);
    assert_eq!(
        str_field(&parse(&stdout, "team delete"), "tenant_id"),
        team_id
    );
    assert_key_rejected(&team_key);
    let _ = fs::remove_dir_all(&team_home);

    let args = ["team", "get"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert_eq!(str_field(&parse(&stdout, "team"), "id"), own_team);
    let _ = fs::remove_dir_all(&home);
}
