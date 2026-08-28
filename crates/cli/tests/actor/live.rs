//! Live `actor` tests (require `MEMORYLAKE_API_KEY`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run, temp_home,
};

/// Deletes the actor it guards when the test ends, so a failed assertion cannot
/// leave data behind in a real account. Disarm it after an asserted delete.
struct ActorGuard {
    home: PathBuf,
    id: String,
}

impl ActorGuard {
    fn new(home: &Path, id: &str) -> Self {
        Self {
            home: home.to_path_buf(),
            id: id.to_string(),
        }
    }

    /// Stop guarding, once the test has deleted the actor itself.
    fn disarm(&mut self) {
        self.id.clear();
    }
}

impl Drop for ActorGuard {
    fn drop(&mut self) {
        if self.id.is_empty() {
            return;
        }
        if !self.home.join(".memorylake").is_dir() {
            // Deleting needs credentials from this `$HOME`. Say so loudly
            // rather than leaking an actor silently.
            eprintln!(
                "WARNING: leaking actor {} — its temp $HOME was removed before cleanup ran",
                self.id
            );
            return;
        }
        // Best effort: the test has already reported the real failure, and a
        // panic here would mask it.
        let _ = run(&self.home, &["actor", "delete", self.id.as_str()]);
    }
}

/// A suffix unique across parallel tests and repeated runs.
///
/// `custom_id` is unique account-wide, and the system clock is not fine-grained
/// enough to separate tests that start in the same instant, so mix in the
/// process id and a counter.
fn unique_suffix() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    )
}

fn login_default(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

fn json_of(home: &Path, args: &[&str]) -> Value {
    let stdout = assert_success(&run(home, args), args);
    serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("parse JSON from memorylake {}: {err}", args.join(" ")))
}

/// Create a workspace to bind into.
///
/// The CLI has no delete-workspace command, so — like the existing workspace
/// live tests — this leaves the workspace behind.
fn create_workspace(home: &Path, suffix: &str) -> String {
    let name = format!("CLI Bin Actor WS {suffix}");
    let custom_id = format!("cli-bin-actor-ws-{suffix}");
    let args = [
        "workspace",
        "create",
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
    ];
    json_of(home, &args)["id"]
        .as_str()
        .expect("workspace create returns an id")
        .to_string()
}

#[test]
fn actor_lifecycle_end_to_end() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let suffix = unique_suffix();
    let custom_id = format!("cli-bin-actor-{suffix}");
    let display_name = format!("CLI Bin Actor {suffix}");

    // --- create -----------------------------------------------------------
    let created = json_of(
        &home,
        &[
            "actor",
            "create",
            "--custom-id",
            custom_id.as_str(),
            "--display-name",
            display_name.as_str(),
            "--type",
            "HUMAN",
            "--description",
            "created by memorylake-cli live test",
            "--metadata",
            r#"{"tier":"premium","region":"us-west"}"#,
        ],
    );
    let actor_id = created["id"]
        .as_str()
        .expect("create returns an id")
        .to_string();
    let mut guard = ActorGuard::new(&home, &actor_id);
    assert_eq!(created["custom_id"], Value::String(custom_id.clone()));
    assert_eq!(created["actor_type"], Value::String("HUMAN".to_string()));
    assert_eq!(created["display_name"], Value::String(display_name.clone()));
    assert_eq!(
        created["metadata"]["region"],
        Value::String("us-west".to_string())
    );

    // --- get by id --------------------------------------------------------
    let by_id = json_of(&home, &["actor", "get", actor_id.as_str()]);
    assert_eq!(by_id["id"], Value::String(actor_id.clone()));
    assert_eq!(by_id["display_name"], Value::String(display_name.clone()));

    // --- get by custom_id -------------------------------------------------
    let by_custom = json_of(
        &home,
        &["actor", "get", custom_id.as_str(), "--by-custom-id"],
    );
    assert_eq!(by_custom["id"], Value::String(actor_id.clone()));
    assert_eq!(by_custom["custom_id"], Value::String(custom_id.clone()));

    // --- update: metadata replaces, it does not merge ----------------------
    let updated_name = format!("{display_name} (VIP)");
    let updated = json_of(
        &home,
        &[
            "actor",
            "update",
            actor_id.as_str(),
            "--display-name",
            updated_name.as_str(),
            "--metadata",
            r#"{"tier":"enterprise"}"#,
        ],
    );
    assert_eq!(updated["display_name"], Value::String(updated_name.clone()));
    assert_eq!(
        updated["metadata"]["tier"],
        Value::String("enterprise".to_string())
    );
    assert!(
        updated["metadata"].get("region").is_none(),
        "metadata must be replaced wholesale, not merged: {}",
        updated["metadata"]
    );
    // An omitted field is left untouched.
    assert_eq!(
        updated["description"],
        Value::String("created by memorylake-cli live test".to_string())
    );

    // --- list with a fuzzy display-name filter ----------------------------
    let listed = json_of(
        &home,
        &[
            "actor",
            "list",
            "--page-size",
            "50",
            "--name",
            updated_name.as_str(),
        ],
    );
    assert!(
        listed["items"]
            .as_array()
            .expect("list returns items")
            .iter()
            .any(|item| item["id"] == Value::String(actor_id.clone())),
        "created actor {actor_id} not found in list results: {listed}"
    );

    // --- bind to a workspace ----------------------------------------------
    let workspace_id = create_workspace(&home, &suffix);
    let binding = json_of(
        &home,
        &[
            "actor",
            "bind",
            "--workspace",
            workspace_id.as_str(),
            "--actor",
            actor_id.as_str(),
        ],
    );
    assert_eq!(binding["actor_id"], Value::String(actor_id.clone()));

    // --- list actors in the workspace -------------------------------------
    let bound = json_of(
        &home,
        &[
            "actor",
            "list",
            "--workspace",
            workspace_id.as_str(),
            "--page-size",
            "50",
        ],
    );
    assert!(
        bound["items"]
            .as_array()
            .expect("workspace list returns items")
            .iter()
            .any(|item| item["actor_id"] == Value::String(actor_id.clone())),
        "bound actor {actor_id} not found in workspace listing: {bound}"
    );

    // --- unbind ------------------------------------------------------------
    let args = [
        "actor",
        "unbind",
        "--workspace",
        workspace_id.as_str(),
        "--actor",
        actor_id.as_str(),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("Unbound actor") && stdout.contains(actor_id.as_str()),
        "unexpected unbind output: {stdout}"
    );

    let after_unbind = json_of(
        &home,
        &[
            "actor",
            "list",
            "--workspace",
            workspace_id.as_str(),
            "--page-size",
            "50",
        ],
    );
    assert!(
        !after_unbind["items"]
            .as_array()
            .expect("workspace list returns items")
            .iter()
            .any(|item| item["actor_id"] == Value::String(actor_id.clone())),
        "actor {actor_id} still bound after unbind: {after_unbind}"
    );

    // --- delete -------------------------------------------------------------
    let args = ["actor", "delete", actor_id.as_str()];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("Deleted actor") && stdout.contains(actor_id.as_str()),
        "unexpected delete output: {stdout}"
    );
    guard.disarm();

    let args = ["actor", "get", actor_id.as_str()];
    assert_failure(&run(&home, &args), &args);

    let _ = fs::remove_dir_all(&home);
}

/// `actor me` names a real actor, not an alias.
///
/// Creates nothing, so it needs no guard.
#[test]
fn actor_me_returns_a_resolvable_actor() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let me = json_of(&home, &["actor", "me"]);
    let id = me["id"].as_str().expect("me returns an id").to_string();
    assert!(!id.is_empty());

    // The point of the cross-check: prove the id addresses the same actor
    // everywhere else, rather than being an alias only this endpoint knows.
    let fetched = json_of(&home, &["actor", "get", id.as_str()]);
    assert_eq!(fetched["id"], Value::String(id.clone()));
    assert_eq!(
        fetched["display_name"], me["display_name"],
        "`actor me` and `actor get` must describe the same actor"
    );

    // Documented and measured: a result says nothing about workspace
    // membership. Asserting the shape of `tags` here keeps `me` honest about
    // returning the same record `get` does.
    assert!(
        me["tags"].is_array(),
        "tags must be present as an array: {me}"
    );

    let _ = fs::remove_dir_all(&home);
}

/// Tags, end to end: what the server stores, how it filters, and what each of
/// the two tag flags does.
///
/// The filter tag carries the run's unique suffix so a parallel run or a stale
/// actor from an earlier one cannot make an assertion pass or fail by accident.
#[test]
fn actor_tags_end_to_end() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let suffix = unique_suffix();
    let custom_id = format!("cli-bin-tag-{suffix}");
    let display_name = format!("CLI Bin Tag {suffix}");
    let unique_tag = format!("cli-tag-{suffix}");

    // --- create: the server trims and de-duplicates ------------------------
    // `vip` is passed twice and one entry is padded, so the response shows
    // whose job normalization is. The CLI sends what it was given.
    let created = json_of(
        &home,
        &[
            "actor",
            "create",
            "--custom-id",
            custom_id.as_str(),
            "--display-name",
            display_name.as_str(),
            "--tags",
            &format!("vip, cn , vip, {unique_tag}"),
        ],
    );
    let actor_id = created["id"]
        .as_str()
        .expect("create returns an id")
        .to_string();
    let mut guard = ActorGuard::new(&home, &actor_id);

    let tags: Vec<String> = created["tags"]
        .as_array()
        .expect("create returns a tags array")
        .iter()
        .map(|tag| tag.as_str().expect("tags are strings").to_string())
        .collect();
    assert_eq!(
        tags,
        vec!["vip".to_string(), "cn".to_string(), unique_tag.clone()],
        "the server de-duplicates and trims, and keeps first-seen order: {}",
        created["tags"]
    );

    // --- an actor with no tags reports an empty list, not a missing field ---
    let plain_custom_id = format!("cli-bin-notag-{suffix}");
    let plain = json_of(
        &home,
        &[
            "actor",
            "create",
            "--custom-id",
            plain_custom_id.as_str(),
            "--display-name",
            &format!("CLI Bin NoTag {suffix}"),
        ],
    );
    let plain_id = plain["id"]
        .as_str()
        .expect("create returns an id")
        .to_string();
    let mut plain_guard = ActorGuard::new(&home, &plain_id);
    assert_eq!(
        plain["tags"],
        Value::Array(Vec::new()),
        "no tags must print as [], which is what the API sends: {plain}"
    );

    // --- filter by one tag --------------------------------------------------
    let found = json_of(
        &home,
        &["actor", "list", "--page-size", "50", "--tags", &unique_tag],
    );
    let ids: Vec<&str> = found["items"]
        .as_array()
        .expect("list returns items")
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        vec![actor_id.as_str()],
        "a unique tag must match exactly the actor carrying it: {found}"
    );

    // --- several tags are ANDed, not ORed -----------------------------------
    let both = json_of(
        &home,
        &[
            "actor",
            "list",
            "--page-size",
            "50",
            "--tags",
            &format!("vip,{unique_tag}"),
        ],
    );
    assert!(
        both["items"]
            .as_array()
            .expect("list returns items")
            .iter()
            .any(|item| item["id"] == Value::String(actor_id.clone())),
        "an actor carrying both tags must be returned: {both}"
    );

    let missing = json_of(
        &home,
        &[
            "actor",
            "list",
            "--page-size",
            "50",
            "--tags",
            &format!("{unique_tag},not-a-tag-on-this-actor"),
        ],
    );
    assert!(
        missing["items"]
            .as_array()
            .expect("list returns items")
            .is_empty(),
        "AND means one absent tag excludes the actor; OR would have matched: {missing}"
    );

    // --- matching is case-sensitive -----------------------------------------
    let wrong_case = json_of(
        &home,
        &[
            "actor",
            "list",
            "--page-size",
            "50",
            "--tags",
            &unique_tag.to_uppercase(),
        ],
    );
    assert!(
        wrong_case["items"]
            .as_array()
            .expect("list returns items")
            .is_empty(),
        "tags are matched exactly, so an upper-cased tag must not match: {wrong_case}"
    );

    // --- a binding carries the actor's tags and its status ------------------
    let workspace_id = create_workspace(&home, &suffix);
    let binding = json_of(
        &home,
        &[
            "actor",
            "bind",
            "--workspace",
            workspace_id.as_str(),
            "--actor",
            actor_id.as_str(),
        ],
    );
    assert!(
        binding["tags"]
            .as_array()
            .expect("a binding reports tags")
            .iter()
            .any(|tag| tag == &Value::String(unique_tag.clone())),
        "the binding must carry the actor's tags: {binding}"
    );
    assert_eq!(
        binding["status"],
        Value::String("ACTIVE".to_string()),
        "a live actor's binding is ACTIVE: {binding}"
    );

    // --- --tags replaces the whole list ------------------------------------
    let replaced = json_of(
        &home,
        &["actor", "update", actor_id.as_str(), "--tags", "gold"],
    );
    assert_eq!(
        replaced["tags"],
        Value::Array(vec![Value::String("gold".to_string())]),
        "--tags replaces rather than adds: {replaced}"
    );

    // --- an omitted --tags leaves them alone -------------------------------
    let renamed = json_of(
        &home,
        &[
            "actor",
            "update",
            actor_id.as_str(),
            "--description",
            "tags must survive this",
        ],
    );
    assert_eq!(
        renamed["tags"],
        Value::Array(vec![Value::String("gold".to_string())]),
        "an update that does not mention tags must not disturb them: {renamed}"
    );

    // --- --clear-tags empties the list --------------------------------------
    let cleared = json_of(
        &home,
        &["actor", "update", actor_id.as_str(), "--clear-tags"],
    );
    assert_eq!(
        cleared["tags"],
        Value::Array(Vec::new()),
        "--clear-tags must remove every tag: {cleared}"
    );

    // --- the server rejects a tag the CLI cannot catch ----------------------
    // Length is left to the server, so prove its error actually surfaces.
    let too_long = "x".repeat(65);
    let args = [
        "actor",
        "update",
        actor_id.as_str(),
        "--tags",
        too_long.as_str(),
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("64"),
        "the server's length limit must reach the user: {err}"
    );

    for id in [actor_id.as_str(), plain_id.as_str()] {
        let args = ["actor", "delete", id];
        assert_success(&run(&home, &args), &args);
    }
    guard.disarm();
    plain_guard.disarm();

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_with_duplicate_custom_id_surfaces_the_server_error() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let suffix = unique_suffix();
    let custom_id = format!("cli-bin-actor-dup-{suffix}");
    let display_name = format!("CLI Bin Actor Dup {suffix}");
    let create_args = [
        "actor",
        "create",
        "--custom-id",
        custom_id.as_str(),
        "--display-name",
        display_name.as_str(),
    ];

    let created = json_of(&home, &create_args);
    let actor_id = created["id"]
        .as_str()
        .expect("create returns an id")
        .to_string();
    let mut guard = ActorGuard::new(&home, &actor_id);

    // `custom_id` is unique account-wide, so the retry must fail with the
    // server's own message rather than a client-side guess.
    let err = assert_failure(&run(&home, &create_args), &create_args);
    assert!(
        err.contains("create actor") && err.contains("HTTP"),
        "duplicate create should surface the server response: {err}"
    );

    let args = ["actor", "delete", actor_id.as_str()];
    assert_success(&run(&home, &args), &args);
    guard.disarm();

    // Only now: the guard deletes through this `$HOME`, so removing it earlier
    // would strand the actor with no credentials to delete it.
    let _ = fs::remove_dir_all(&home);
}
