//! Live `conversation` tests (require `MEMORYLAKE_API_KEY`).
//!
//! Two tests share one setup shape ([`Live`]): a scratch workspace holding a
//! scratch project and a bound actor. The project and actor are removed
//! however the test ends, including a mid-test panic; the workspace is left
//! behind because `workspace delete` is not implemented.
//!
//! * `conversation_lifecycle_create_append_read_delete` walks the common path.
//! * `conversation_options_reach_the_server` covers the flags the first test
//!   does not exercise, so nothing ships verified against a stub alone.
//!
//! `cook-status` is asserted for shape only. Memory is built asynchronously
//! and a conversation is not guaranteed to ever report finished, so waiting
//! for `true` would make these tests hang on a healthy server.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::common::{
    assert_success, live_base_url, login_args, require_api_key, run, temp_home, unique_name,
};

/// A logged-in temp `$HOME` plus the scratch objects a conversation needs.
///
/// Cleanup runs from `Drop` rather than the end of the test body: a failing
/// assertion panics, and that must not leave an actor and a project behind in
/// the real account.
struct Live {
    home: PathBuf,
    workspace: String,
    project: String,
    actor: String,
}

impl Live {
    /// Log in, then create a workspace, a project, and an actor bound to it.
    fn start(tag: &str) -> Self {
        let api_key = require_api_key();
        let home = temp_home();
        let base_url = live_base_url();
        let args = login_args(&api_key, "default", base_url.as_deref());
        assert_success(&run(&home, &args), &args);

        let workspace_name = unique_name(&format!("{tag}-ws"));
        let workspace = id_of(
            &json_of(
                &home,
                &[
                    "workspace",
                    "create",
                    "--name",
                    workspace_name.as_str(),
                    "--custom-id",
                    workspace_name.as_str(),
                ],
            ),
            "workspace create",
        );

        let project_name = unique_name(&format!("{tag}-proj"));
        let project = id_of(
            &json_of(
                &home,
                &[
                    "project",
                    "create",
                    "--workspace",
                    workspace.as_str(),
                    "--name",
                    project_name.as_str(),
                    "--custom-id",
                    project_name.as_str(),
                ],
            ),
            "project create",
        );

        let actor_name = unique_name(&format!("{tag}-actor"));
        let actor = id_of(
            &json_of(
                &home,
                &[
                    "actor",
                    "create",
                    "--custom-id",
                    actor_name.as_str(),
                    "--display-name",
                    actor_name.as_str(),
                    "--type",
                    "HUMAN",
                ],
            ),
            "actor create",
        );

        let args = [
            "actor",
            "bind",
            "--workspace",
            workspace.as_str(),
            "--actor",
            actor.as_str(),
        ];
        assert_success(&run(&home, &args), &args);

        Self {
            home,
            workspace,
            project,
            actor,
        }
    }

    /// Run a `conversation` command and parse its stdout as JSON.
    fn json(&self, args: &[&str]) -> Value {
        json_of(&self.home, args)
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = run(
            &self.home,
            &[
                "project",
                "delete",
                "--workspace",
                &self.workspace,
                &self.project,
            ],
        );
        let _ = run(&self.home, &["actor", "delete", &self.actor]);
        let _ = fs::remove_dir_all(&self.home);
    }
}

/// Run a command and parse its stdout as JSON.
fn json_of(home: &Path, args: &[&str]) -> Value {
    let stdout = assert_success(&run(home, args), args);
    serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("parse JSON from `{}`: {err}\n{stdout}", args.join(" ")))
}

fn id_of(value: &Value, what: &str) -> String {
    value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{what} response has an id: {value}"))
        .to_string()
}

/// Every `text` field found anywhere in a message listing's content blocks.
fn texts_of(listing: &Value) -> Vec<String> {
    listing
        .get("items")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("message list has items: {listing}"))
        .iter()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// Write a scratch JSON file and return its path.
fn scratch_json(tag: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("{}.json", unique_name(tag)));
    fs::write(&path, contents).expect("write scratch JSON");
    path
}

#[test]
fn conversation_lifecycle_create_append_read_delete() {
    let live = Live::start("conv");

    // --- create -----------------------------------------------------------
    let conversation_custom_id = unique_name("conv-session");
    let created = live.json(&[
        "conversation",
        "create",
        "--workspace",
        live.workspace.as_str(),
        "--custom-id",
        conversation_custom_id.as_str(),
        "--project",
        live.project.as_str(),
        "--name",
        "CLI live conversation",
        "--kind",
        "DIRECT",
        "--actors",
        live.actor.as_str(),
    ]);
    let conversation = id_of(&created, "conversation create");
    assert_eq!(
        created["rw_project_ids"],
        Value::Array(vec![Value::String(live.project.clone())]),
        "--project becomes the conversation's read-write scope: {created}"
    );
    assert_eq!(
        created["name"], "CLI live conversation",
        "the title reaches the server: {created}"
    );
    assert_eq!(
        created["kind"], "DIRECT",
        "the kind reaches it too: {created}"
    );
    assert_eq!(
        created["actor_ids"],
        Value::Array(vec![Value::String(live.actor.clone())]),
        "--actors lands in actor_ids: {created}"
    );

    // --- get, by id and by custom_id --------------------------------------
    let by_id = live.json(&[
        "conversation",
        "get",
        "--workspace",
        live.workspace.as_str(),
        conversation.as_str(),
    ]);
    assert_eq!(by_id["id"], Value::String(conversation.clone()));

    let by_custom = live.json(&[
        "conversation",
        "get",
        "--workspace",
        live.workspace.as_str(),
        conversation_custom_id.as_str(),
        "--by-custom-id",
    ]);
    assert_eq!(
        by_custom["id"],
        Value::String(conversation.clone()),
        "custom_id lookup resolves to the same conversation: {by_custom}"
    );

    // --- append -----------------------------------------------------------
    let first_custom_id = unique_name("conv-msg");
    let appended = live.json(&[
        "conversation",
        "message",
        "append",
        conversation.as_str(),
        "--workspace",
        live.workspace.as_str(),
        "--actor",
        live.actor.as_str(),
        "--custom-id",
        first_custom_id.as_str(),
        "--text",
        "The CLI live test prefers vim.",
    ]);
    let first_message = id_of(&appended, "message append");

    // Re-appending the same custom_id must return the same message rather
    // than duplicate it — that idempotency is what makes a 409 retry safe.
    let retried = live.json(&[
        "conversation",
        "message",
        "append",
        conversation.as_str(),
        "--workspace",
        live.workspace.as_str(),
        "--actor",
        live.actor.as_str(),
        "--custom-id",
        first_custom_id.as_str(),
        "--text",
        "The CLI live test prefers vim.",
    ]);
    assert_eq!(
        id_of(&retried, "message append retry"),
        first_message,
        "a repeated custom_id returns the first message: {retried}"
    );

    // A second message carrying a block type `--text` cannot express.
    let second_custom_id = unique_name("conv-msg");
    let with_blocks = live.json(&[
        "conversation",
        "message",
        "append",
        conversation.as_str(),
        "--workspace",
        live.workspace.as_str(),
        "--actor",
        live.actor.as_str(),
        "--custom-id",
        second_custom_id.as_str(),
        "--content-json",
        r#"[{"block_type":"TEXT","text":"and ships on Fridays."},
            {"block_type":"THINKING","text":"weighing the release window"}]"#,
    ]);
    assert_ne!(
        id_of(&with_blocks, "message append with blocks"),
        first_message
    );

    // --- read back --------------------------------------------------------
    let listing = live.json(&[
        "conversation",
        "message",
        "list",
        conversation.as_str(),
        "--page-size",
        "50",
    ]);
    assert_eq!(
        listing["items"].as_array().map(Vec::len),
        Some(2),
        "two distinct messages were appended: {listing}"
    );
    let texts = texts_of(&listing);
    assert!(
        texts.iter().any(|text| text.contains("prefers vim")),
        "the appended text comes back: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text.contains("release window")),
        "a THINKING block survives the round trip: {texts:?}"
    );

    // --- cook status ------------------------------------------------------
    // Shape only: memory is built asynchronously and a conversation may never
    // report finished, so waiting on `true` here would hang.
    let status = live.json(&[
        "conversation",
        "cook-status",
        "--workspace",
        live.workspace.as_str(),
        conversation.as_str(),
    ]);
    assert!(
        status
            .get("cook_finished")
            .and_then(Value::as_bool)
            .is_some(),
        "cook-status reports the flag: {status}"
    );

    // --- list -------------------------------------------------------------
    let listing = live.json(&[
        "conversation",
        "list",
        "--workspace",
        live.workspace.as_str(),
        "--page-size",
        "50",
    ]);
    let found = listing["items"]
        .as_array()
        .unwrap_or_else(|| panic!("conversation list has items: {listing}"))
        .iter()
        .any(|item| item.get("id").and_then(Value::as_str) == Some(conversation.as_str()));
    assert!(found, "the new conversation is listed: {listing}");

    // --- delete -----------------------------------------------------------
    let args = [
        "conversation",
        "delete",
        "--workspace",
        live.workspace.as_str(),
        conversation.as_str(),
    ];
    let stdout = assert_success(&run(&live.home, &args), &args);
    assert!(
        stdout.contains(&conversation),
        "delete names the conversation: {stdout}"
    );

    let args = [
        "conversation",
        "get",
        "--workspace",
        live.workspace.as_str(),
        conversation.as_str(),
    ];
    assert!(
        !run(&live.home, &args).status.success(),
        "a deleted conversation must not still be readable"
    );
}

/// The flags the lifecycle test does not exercise.
///
/// Everything here is otherwise covered only by the loopback stub, which
/// proves what the CLI sends but not that the server accepts it.
#[test]
fn conversation_options_reach_the_server() {
    let live = Live::start("convopt");

    // --- GROUP kind, metadata ---------------------------------------------
    let group_custom_id = unique_name("conv-group");
    let group = live.json(&[
        "conversation",
        "create",
        "--workspace",
        live.workspace.as_str(),
        "--custom-id",
        group_custom_id.as_str(),
        "--project",
        live.project.as_str(),
        "--name",
        "CLI live group",
        "--kind",
        "GROUP",
        "--actors",
        live.actor.as_str(),
        "--metadata",
        "source=cli-live-test",
        "--metadata",
        "tier=scratch",
    ]);
    let conversation = id_of(&group, "group conversation create");
    assert_eq!(
        group["kind"], "GROUP",
        "GROUP is accepted, not just DIRECT: {group}"
    );
    assert_eq!(
        group["metadata"]["source"], "cli-live-test",
        "repeated --metadata flags are stored: {group}"
    );
    assert_eq!(group["metadata"]["tier"], "scratch", "both pairs: {group}");

    // --- cook-status --by-custom-id ---------------------------------------
    let status = live.json(&[
        "conversation",
        "cook-status",
        "--workspace",
        live.workspace.as_str(),
        group_custom_id.as_str(),
        "--by-custom-id",
    ]);
    assert!(
        status
            .get("cook_finished")
            .and_then(Value::as_bool)
            .is_some(),
        "cook-status resolves a custom_id: {status}"
    );

    // --- append --content-file --------------------------------------------
    let blocks = scratch_json(
        "conv-blocks",
        r#"[{"block_type":"TEXT","text":"loaded from a content file"}]"#,
    );
    let first = live.json(&[
        "conversation",
        "message",
        "append",
        conversation.as_str(),
        "--workspace",
        live.workspace.as_str(),
        "--actor",
        live.actor.as_str(),
        "--custom-id",
        unique_name("conv-msg").as_str(),
        "--content-file",
        blocks.to_str().expect("utf-8 scratch path"),
    ]);
    let first_message = id_of(&first, "append from content file");
    let _ = fs::remove_file(&blocks);

    // --- append --parent --timestamp --metadata ---------------------------
    let second = live.json(&[
        "conversation",
        "message",
        "append",
        conversation.as_str(),
        "--workspace",
        live.workspace.as_str(),
        "--actor",
        live.actor.as_str(),
        "--custom-id",
        unique_name("conv-msg").as_str(),
        "--text",
        "explicitly parented",
        "--parent",
        first_message.as_str(),
        "--timestamp",
        "2026-08-13T00:00:00Z",
        "--metadata",
        "origin=live-test",
    ]);
    let second_message = id_of(&second, "parented append");
    assert_ne!(second_message, first_message);

    // The append *response* omits `metadata`, `timestamp` and `actor_type`
    // even when they were sent (measured 2026-08-13). They are stored — the
    // listing has them — so this pins the asymmetry rather than the bug it
    // looks like: a caller must not read the append response to confirm what
    // it just wrote.
    assert_eq!(
        second["metadata"],
        Value::Null,
        "append echoes no metadata; assert against the listing instead: {second}"
    );
    assert_eq!(
        second["timestamp"],
        Value::Null,
        "append echoes no timestamp: {second}"
    );

    let listed = live.json(&[
        "conversation",
        "message",
        "list",
        conversation.as_str(),
        "--page-size",
        "50",
    ]);
    let stored = listed["items"]
        .as_array()
        .expect("listing has items")
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(second_message.as_str()))
        .unwrap_or_else(|| panic!("the parented message is listed: {listed}"));
    assert_eq!(
        stored["metadata"]["origin"], "live-test",
        "message metadata is stored even though the append did not echo it: {stored}"
    );
    assert_eq!(
        stored["timestamp"], "2026-08-13T00:00:00Z",
        "the caller-supplied timestamp is kept rather than replaced by server time: {stored}"
    );
    assert_eq!(
        stored["actor_type"], "HUMAN",
        "the actor's type is resolved on read: {stored}"
    );
    assert_eq!(
        stored["sequence_no"], 2,
        "--parent placed the message after the one it named: {stored}"
    );

    // --- append --wait ----------------------------------------------------
    // A third message, so a page size of 2 leaves a second page to fetch, and
    // the one that exercises --wait.
    //
    // Either outcome is legitimate here: the server may finish building the
    // conversation's memory within the timeout, or not — the API documents
    // that a conversation is not guaranteed to reach a finished state. So this
    // asserts what must hold either way (the message is stored, and a timeout
    // says so plainly) rather than pinning a race.
    let waited_custom_id = unique_name("conv-msg");
    let args = [
        "conversation",
        "message",
        "append",
        conversation.as_str(),
        "--workspace",
        live.workspace.as_str(),
        "--actor",
        live.actor.as_str(),
        "--custom-id",
        waited_custom_id.as_str(),
        "--text",
        "third message",
        "--wait",
        "--timeout",
        "30",
    ];
    let output = run(&live.home, &args);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let appended: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("--wait still prints the appended message: {err}\n{stdout}"));
    let waited_message = id_of(&appended, "append --wait");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("still building its memory"),
            "the only acceptable --wait failure is the timeout: {stderr}"
        );
        assert!(
            stderr.contains("processing continues on the server"),
            "a timeout must say the append was not undone: {stderr}"
        );
    }

    // Whatever the wait concluded, the message itself is stored.
    let after_wait = live.json(&[
        "conversation",
        "message",
        "list",
        conversation.as_str(),
        "--page-size",
        "50",
    ]);
    assert!(
        after_wait["items"]
            .as_array()
            .expect("listing has items")
            .iter()
            .any(|item| item.get("id").and_then(Value::as_str) == Some(waited_message.as_str())),
        "--wait does not change what was appended: {after_wait}"
    );

    // --- message list paging ----------------------------------------------
    let page = live.json(&[
        "conversation",
        "message",
        "list",
        conversation.as_str(),
        "--page-size",
        "2",
    ]);
    assert_eq!(
        page["items"].as_array().map(Vec::len),
        Some(2),
        "--page-size caps the page: {page}"
    );
    let token = page["continuation_token"]
        .as_str()
        .unwrap_or_else(|| panic!("three messages over pages of two leave a token: {page}"))
        .to_string();

    let next = live.json(&[
        "conversation",
        "message",
        "list",
        conversation.as_str(),
        "--page-size",
        "2",
        "--continuation-token",
        token.as_str(),
    ]);
    let next_items = next["items"]
        .as_array()
        .unwrap_or_else(|| panic!("second page has items: {next}"));
    assert_eq!(next_items.len(), 1, "the tail of three messages: {next}");
    let first_page_ids: Vec<&str> = page["items"]
        .as_array()
        .expect("first page items")
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .collect();
    let tail_id = next_items[0]
        .get("id")
        .and_then(Value::as_str)
        .expect("tail message has an id");
    assert!(
        !first_page_ids.contains(&tail_id),
        "the second page must not repeat the first: {first_page_ids:?} then {tail_id}"
    );

    // --- conversation list paging -----------------------------------------
    // A second conversation, so the workspace listing has something to page.
    let other = id_of(
        &live.json(&[
            "conversation",
            "create",
            "--workspace",
            live.workspace.as_str(),
            "--custom-id",
            unique_name("conv-second").as_str(),
            "--project",
            live.project.as_str(),
            "--actors",
            live.actor.as_str(),
        ]),
        "second conversation create",
    );

    let page = live.json(&[
        "conversation",
        "list",
        "--workspace",
        live.workspace.as_str(),
        "--page-size",
        "1",
    ]);
    assert_eq!(
        page["items"].as_array().map(Vec::len),
        Some(1),
        "--page-size caps the conversation page: {page}"
    );
    let token = page["continuation_token"]
        .as_str()
        .unwrap_or_else(|| panic!("two conversations over pages of one leave a token: {page}"))
        .to_string();

    let next = live.json(&[
        "conversation",
        "list",
        "--workspace",
        live.workspace.as_str(),
        "--page-size",
        "1",
        "--continuation-token",
        token.as_str(),
    ]);
    let seen: Vec<&str> = page["items"]
        .as_array()
        .expect("first page")
        .iter()
        .chain(next["items"].as_array().expect("second page"))
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .collect();
    assert!(
        seen.contains(&conversation.as_str()) && seen.contains(&other.as_str()),
        "paging through the workspace reaches both conversations: {seen:?}"
    );

    for id in [&conversation, &other] {
        let args = [
            "conversation",
            "delete",
            "--workspace",
            live.workspace.as_str(),
            id.as_str(),
        ];
        assert_success(&run(&live.home, &args), &args);
    }
}
