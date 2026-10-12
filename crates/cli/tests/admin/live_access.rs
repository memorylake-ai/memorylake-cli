//! Live access-control tests (require `MEMORYLAKE_API_KEY`, a team owner's).
//!
//! As in `live.rs`, these assert what a change DOES, not only that it was
//! accepted: a scope limit must narrow what the member's own key lists, an
//! access group must widen it again, and a stale ETag must stop a write.
//!
//! Every group, role, rule, member, and key created here carries an
//! `mlcli-` name and is removed before the test ends. Instances come from the
//! shared scratch workspace, which every live suite reuses.

use std::fs;
use std::path::Path;
use std::process::Output;

use super::live::{login, login_default, parse, str_field, team_is_personal, unix_now};
use crate::common::{
    assert_failure, assert_success, require_api_key, run, scratch_workspace, temp_home, unique_name,
};

/// Run a command that must succeed and parse its JSON output.
fn run_json(home: &Path, args: &[&str]) -> serde_json::Value {
    let stdout = assert_success(&run(home, args), args);
    parse(&stdout, &args.join(" "))
}

/// The ETag a read printed on standard error.
fn etag(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("ETag: "))
        .unwrap_or_else(|| panic!("no ETag on standard error: {stderr}"))
        .to_string()
}

fn items(value: &serde_json::Value) -> &[serde_json::Value] {
    value
        .get("items")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or_else(|| panic!("no `items`: {value}"))
}

/// Subject ids `access show` (or a grant's echo) lists as allowed.
fn allowed_subjects(access: &serde_json::Value) -> Vec<String> {
    access
        .get("allowed")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("no `allowed`: {access}"))
        .iter()
        .map(|row| str_field(row, "subject_id"))
        .collect()
}

/// Instance ids of one resource type in a scope or explanation.
fn scope_instances(value: &serde_json::Value, resource_type: &str) -> Vec<String> {
    value
        .get("scopes")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|entry| entry.get("resource_type").and_then(|v| v.as_str()) == Some(resource_type))
        .flat_map(|entry| {
            entry
                .get("instances")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
        })
        .map(|instance| str_field(&instance, "id"))
        .collect()
}

/// The workspace ids `home`'s key can list, sorted.
fn visible_workspaces(home: &Path) -> Vec<String> {
    let mut ids: Vec<String> = items(&run_json(home, &["ws", "list"]))
        .iter()
        .map(|ws| str_field(ws, "id"))
        .collect();
    ids.sort();
    ids
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

#[test]
fn the_permission_reads_agree_with_each_other() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let me = run_json(&home, &["permission", "me"]);
    assert_eq!(str_field(&me, "role"), "tenant_owner");
    assert!(
        me.get("permissions")
            .and_then(|v| v.as_array())
            .is_some_and(|p| !p.is_empty()),
        "an owner holds permissions: {me}"
    );

    // What an access group may be given is a subset of the full catalog.
    let types = |catalog: &serde_json::Value| -> Vec<String> {
        catalog
            .get("resource_types")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("no resource_types: {catalog}"))
            .iter()
            .map(|t| str_field(t, "resource_type"))
            .collect()
    };
    let all = types(&run_json(&home, &["perm", "catalog"]));
    let for_groups = types(&run_json(&home, &["perm", "catalog", "--access-group"]));
    assert!(!for_groups.is_empty() && for_groups.len() <= all.len());
    assert!(
        for_groups.iter().all(|t| all.contains(t)),
        "{for_groups:?} ⊄ {all:?}"
    );

    // A scope read carries an ETag on standard error and pure JSON on
    // standard output.
    let args = ["role", "scope", "get", "tenant_member"];
    let output = run(&home, &args);
    let scope = parse(&assert_success(&output, &args), "role scope");
    assert_eq!(str_field(&scope, "role_key"), "tenant_member");
    assert!(etag(&output).starts_with('"'), "{}", etag(&output));

    // Built-in roles' rules are listed but not editable — and the server
    // agrees.
    let rules = run_json(
        &home,
        &[
            "perm",
            "rule",
            "list",
            "--subject-type",
            "role",
            "--subject-id",
            "tenant_member",
        ],
    );
    let rules = items(&rules);
    assert!(!rules.is_empty());
    assert!(
        rules
            .iter()
            .all(|r| r.get("editable") == Some(&serde_json::Value::Bool(false))),
        "built-in rules must not be editable: {rules:?}"
    );
    let rule_id = str_field(&rules[0], "rule_id");
    let args = ["perm", "rule", "delete", rule_id.as_str()];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("built-in"), "{err}");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_access_group_grants_its_instances_and_honours_if_match() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = scratch_workspace(&home);
    let name = unique_name("ag");

    let group = run_json(
        &home,
        &[
            "ag",
            "create",
            "--name",
            name.as_str(),
            "--permission",
            "memorylake:actor:actor:read",
        ],
    );
    let group_id = str_field(&group, "group_id");
    let id = group_id.as_str();

    // A write carrying the ETag of the latest read goes through; replaying
    // the same, now stale, ETag is refused.
    let args = ["ag", "get", id];
    let fresh = etag(&run(&home, &args));
    let set = |if_match: &str| {
        run(
            &home,
            &[
                "ag",
                "permissions",
                "set",
                id,
                "--permission",
                "memorylake:actor:actor:read",
                "--permission",
                "memorylake:agent:agent:read",
                "--reason",
                "mlcli live test",
                "--if-match",
                if_match,
            ],
        )
    };
    assert_success(&set(&fresh), &["ag permissions set (fresh ETag)"]);
    let err = assert_failure(&set(&fresh), &["ag permissions set (stale ETag)"]);
    assert!(err.contains("412"), "a stale ETag must be refused: {err}");
    let explained = run_json(&home, &["ag", "explain", id]);
    assert!(
        explained
            .to_string()
            .contains("memorylake:agent:agent:read"),
        "the new permission must be in effect: {explained}"
    );

    // Granting an instance makes the group show up on it, and in the
    // group's own scope; revoking undoes both.
    let grant = [
        "access",
        "grant",
        "workspace",
        workspace.as_str(),
        "--group",
        id,
        "--reason",
        "mlcli live test",
    ];
    assert!(allowed_subjects(&run_json(&home, &grant)).contains(&group_id));
    let shown = run_json(&home, &["access", "show", "workspace", workspace.as_str()]);
    assert!(allowed_subjects(&shown).contains(&group_id), "{shown}");
    let scope = run_json(&home, &["ag", "scope", "get", id]);
    assert_eq!(
        scope_instances(&scope, "memorylake:workspace"),
        vec![workspace.clone()]
    );

    let revoke = [
        "access",
        "revoke",
        "workspace",
        workspace.as_str(),
        "--group",
        id,
        "--reason",
        "mlcli live test",
    ];
    assert!(!allowed_subjects(&run_json(&home, &revoke)).contains(&group_id));
    let scope = run_json(&home, &["ag", "scope", "get", id]);
    assert!(scope_instances(&scope, "memorylake:workspace").is_empty());

    let args = ["ag", "delete", id];
    assert_success(&run(&home, &args), &args);
    let args = ["ag", "get", id];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("NOT_FOUND"), "{err}");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_custom_role_carries_its_rules_and_scope_until_deleted() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = scratch_workspace(&home);
    let label = unique_name("role");

    let role = run_json(
        &home,
        &[
            "role",
            "create",
            "--label",
            label.as_str(),
            "--base-role",
            "tenant_member",
        ],
    );
    let key = str_field(&role, "key");
    let key = key.as_str();
    assert_eq!(str_field(&role, "base_role_key"), "tenant_member");

    let renamed = format!("{label}-2");
    let role = run_json(&home, &["role", "update", key, "--label", renamed.as_str()]);
    assert_eq!(str_field(&role, "label"), renamed);

    // A rule added to the role is listed as editable, and gone once deleted.
    let created = run_json(
        &home,
        &[
            "perm",
            "rule",
            "create",
            "--role",
            key,
            "--resource",
            "memorylake:skill",
            "--action",
            "skill:modify",
            "--auto-grant-prerequisites",
            "--reason",
            "mlcli live test",
        ],
    );
    let rule_ids: Vec<String> = created
        .get("rules")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("no rules: {created}"))
        .iter()
        .map(|r| str_field(r, "rule_id"))
        .collect();
    // The copied base rules alone fill more than a default page.
    let list = [
        "perm",
        "rule",
        "list",
        "--subject-type",
        "role",
        "--subject-id",
        key,
        "--page-size",
        "100",
    ];
    let has_modify = |rules: &serde_json::Value| {
        items(rules).iter().any(|r| {
            r.get("action").and_then(|v| v.as_str()) == Some("skill:modify")
                && r.get("editable") == Some(&serde_json::Value::Bool(true))
        })
    };
    assert!(has_modify(&run_json(&home, &list)));

    // The role's scope, written against the ETag of the read.
    let args = ["role", "scope", "get", key];
    let tag = etag(&run(&home, &args));
    let scopes = format!(
        r#"[{{"resource_type":"memorylake:workspace","mode":"instances","instances":[{{"id":"{workspace}"}}]}}]"#
    );
    let scope = run_json(
        &home,
        &[
            "role",
            "scope",
            "set",
            key,
            "--scopes",
            scopes.as_str(),
            "--reason",
            "mlcli live test",
            "--if-match",
            tag.as_str(),
        ],
    );
    assert_eq!(
        scope_instances(&scope, "memorylake:workspace"),
        vec![workspace.clone()]
    );

    for rule_id in &rule_ids {
        let args = [
            "perm",
            "rule",
            "delete",
            rule_id.as_str(),
            "--reason",
            "mlcli live test",
        ];
        assert_success(&run(&home, &args), &args);
    }
    assert!(!has_modify(&run_json(&home, &list)));

    let args = ["role", "delete", key];
    assert_success(&run(&home, &args), &args);
    let args = ["role", "get", key];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("NOT_FOUND"), "{err}");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_scope_limits_what_a_members_own_key_sees() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    // Virtual members exist only on org teams; `live.rs` pins the
    // personal-team refusal.
    if team_is_personal(&home) {
        let _ = fs::remove_dir_all(&home);
        return;
    }
    let limited_to = scratch_workspace(&home);
    let other = visible_workspaces(&home)
        .into_iter()
        .find(|id| *id != limited_to)
        .expect("every team has a default workspace besides the scratch one");

    let name = format!("mlcli-vm-{}", unix_now() % 10_000_000_000);
    let member = run_json(
        &home,
        &[
            "member",
            "create",
            "--name",
            name.as_str(),
            "--role",
            "tenant_member",
        ],
    );
    let principal_id = str_field(&member, "principal_id");
    let pid = principal_id.as_str();
    let key_name = unique_name("vm-key");
    let key = run_json(
        &home,
        &[
            "api-key",
            "create",
            "--name",
            key_name.as_str(),
            "--member",
            pid,
        ],
    );
    let key_id = str_field(&key, "id");
    // The member's key lives in its own home, so bare commands in `home`
    // stay the owner's.
    let member_home = temp_home();
    assert_success(
        &login(&member_home, &str_field(&key, "key"), "member"),
        &["login member"],
    );

    // Limited to one workspace, the member's key lists only that one.
    let scopes = format!(
        r#"[{{"resource_type":"memorylake:workspace","mode":"instances","instances":[{{"id":"{limited_to}"}}]}}]"#
    );
    run_json(
        &home,
        &[
            "member",
            "scope",
            "set",
            pid,
            "--scopes",
            scopes.as_str(),
            "--reason",
            "mlcli live test",
        ],
    );
    assert_eq!(visible_workspaces(&member_home), vec![limited_to.clone()]);

    // Revoking the only instance would lift the limit, so it is refused.
    let args = [
        "access",
        "revoke",
        "workspace",
        limited_to.as_str(),
        "--member",
        pid,
        "--reason",
        "mlcli live test",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("409"), "{err}");

    // A grant widens the limit, a revoke narrows it again.
    let both = sorted(vec![limited_to.clone(), other.clone()]);
    let on_other = |verb: &'static str| {
        [
            "access",
            verb,
            "workspace",
            other.as_str(),
            "--member",
            pid,
            "--reason",
            "mlcli live test",
        ]
    };
    run_json(&home, &on_other("grant"));
    assert_eq!(visible_workspaces(&member_home), both);
    run_json(&home, &on_other("revoke"));
    assert_eq!(visible_workspaces(&member_home), vec![limited_to.clone()]);

    // An access group the member joins adds its instances on top, and
    // `member explain` names the group as the source.
    let group_name = unique_name("ag");
    let group = run_json(&home, &["ag", "create", "--name", group_name.as_str()]);
    let group_id = str_field(&group, "group_id");
    run_json(&home, &["ag", "member", "add", group_id.as_str(), pid]);
    run_json(
        &home,
        &[
            "access",
            "grant",
            "workspace",
            other.as_str(),
            "--group",
            group_id.as_str(),
            "--reason",
            "mlcli live test",
        ],
    );
    assert_eq!(visible_workspaces(&member_home), both);
    let explained = run_json(&home, &["member", "explain", pid]);
    let via_group = explained
        .get("scopes")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .flat_map(|entry| entry.get("instances").and_then(|v| v.as_array()))
        .flatten()
        .find(|instance| instance.get("id").and_then(|v| v.as_str()) == Some(other.as_str()))
        .unwrap_or_else(|| panic!("{other} missing from the explanation: {explained}"));
    assert!(
        via_group.to_string().contains(group_id.as_str()),
        "{via_group}"
    );

    // A permission level set here is what `access levels` reports; `none`
    // removes it.
    let level = |value: &'static str| {
        [
            "access",
            "set-level",
            "workspace",
            limited_to.as_str(),
            "--member",
            pid,
            "--level",
            value,
            "--reason",
            "mlcli live test",
        ]
    };
    let levels = [
        "access",
        "levels",
        "workspace",
        limited_to.as_str(),
        "--member",
        pid,
    ];
    run_json(&home, &level("write"));
    assert_eq!(
        str_field(&run_json(&home, &levels), "current_level"),
        "write"
    );
    run_json(&home, &level("none"));
    assert!(run_json(&home, &levels)["current_level"].is_null());

    // Deleting the group takes its instances away again.
    let args = ["ag", "delete", group_id.as_str()];
    assert_success(&run(&home, &args), &args);
    assert_eq!(visible_workspaces(&member_home), vec![limited_to.clone()]);

    let args = ["api-key", "revoke", key_id.as_str()];
    assert_success(&run(&home, &args), &args);
    let args = ["member", "remove", pid];
    assert_success(&run(&home, &args), &args);
    let _ = fs::remove_dir_all(&member_home);
    let _ = fs::remove_dir_all(&home);
}
