//! Offline `fact` tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::stub::logged_in_home;
use crate::common::{assert_failure, assert_success, run, temp_home};

/// A base URL nothing is listening on.
///
/// Local validation must fail *before* the CLI tries to reach this, so a test
/// that sees a connection error here has caught validation happening too late.
const UNREACHABLE: &str = "http://127.0.0.1:1/openapi/memorylake";

/// Assert the command failed locally rather than by contacting the network.
fn assert_rejected_locally(args: &[&str], expected: &str) {
    let home = logged_in_home(UNREACHABLE);
    let err = assert_failure(&run(&home, args), args);
    assert!(
        err.contains(expected),
        "expected {expected:?} for {args:?}, got: {err}"
    );
    assert!(
        !err.contains("could not connect") && !err.contains("timed out"),
        "validation must happen before any request, but {args:?} reached the network: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

/// `fact <args...> --workspace ws-1` with the given scope flags spliced in.
fn scoped<'a>(command: &[&'a str], scope: &[&'a str], rest: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["fact"];
    args.extend_from_slice(command);
    args.extend_from_slice(&["--workspace", "ws-1"]);
    args.extend_from_slice(scope);
    args.extend_from_slice(rest);
    args
}

/// Every scoped subcommand with the positional arguments it needs.
const SCOPED_COMMANDS: &[(&[&str], &[&str])] = &[
    (&["add"], &["text"]),
    (&["delete"], &["fact-1"]),
    (&["get"], &["fact-1"]),
    (&["update"], &["fact-1", "--text", "t"]),
    (&["trace"], &["fact-1"]),
    (&["conflict", "list"], &[]),
    (&["conflict", "get"], &["cfl-1"]),
    (
        &["conflict", "resolve"],
        &["cfl-1", "--strategy", "dismiss"],
    ),
    (&["instruction", "get"], &[]),
    (&["instruction", "set"], &["--text", "t"]),
    (&["instruction", "clear"], &[]),
    (&["instruction", "draft"], &[]),
];

#[test]
fn help_lists_fact_subcommands() {
    let home = temp_home();
    let args = ["fact", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in [
        "add",
        "delete",
        "get",
        "update",
        "trace",
        "list",
        "conflict",
        "instruction",
    ] {
        assert!(
            stdout.contains(subcommand),
            "`fact --help` missing {subcommand}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn group_help_lists_their_subcommands() {
    let home = temp_home();
    for (group, subcommands) in [
        ("conflict", &["list", "get", "resolve"][..]),
        ("instruction", &["get", "set", "clear", "draft"][..]),
    ] {
        let args = ["fact", group, "--help"];
        let stdout = assert_success(&run(&home, &args), &args);
        for subcommand in subcommands {
            assert!(
                stdout.contains(subcommand),
                "`fact {group} --help` missing {subcommand}: {stdout}"
            );
        }
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn help_states_the_documented_semantics() {
    let home = temp_home();
    for (args, needles) in [
        (
            &["fact", "update", "--help"][..],
            &["replaces the whole", "'{}'"][..],
        ),
        (&["fact", "trace", "--help"][..], &["forgotten"][..]),
        (
            &["fact", "instruction", "draft", "--help"][..],
            &["Nothing is saved", "English language name"][..],
        ),
        (
            &["fact", "instruction", "set", "--help"][..],
            &["replaces the old one whole", "2000", "`-` reads stdin"][..],
        ),
        (
            &["fact", "conflict", "resolve", "--help"][..],
            &["keep_fact", "edit_fact", "trust_document", "dismiss"][..],
        ),
    ] {
        let stdout = assert_success(&run(&home, args), args);
        for needle in needles {
            assert!(stdout.contains(needle), "{args:?} help missing {needle:?}");
        }
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn every_scoped_command_requires_a_scope() {
    for (command, rest) in SCOPED_COMMANDS {
        assert_rejected_locally(&scoped(command, &[], rest), "--agent");
    }
}

#[test]
fn every_scoped_command_rejects_several_scopes() {
    for (command, rest) in SCOPED_COMMANDS {
        assert_rejected_locally(
            &scoped(command, &["--actor", "actor-1", "--agent", "agent-1"], rest),
            "not several",
        );
        assert_rejected_locally(
            &scoped(command, &["--actor", "a", "--project", "p"], rest),
            "not several",
        );
    }
}

#[test]
fn scope_errors_win_over_missing_credentials() {
    // Local validation runs before credentials are resolved, so a logged-out
    // caller still learns what is wrong with the command itself.
    let home = temp_home();
    let args = ["fact", "get", "--workspace", "ws-1", "fact-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("a scope is required"), "{err}");
    assert!(!err.contains("not logged in"), "{err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn add_requires_at_least_one_fact() {
    let home = temp_home();
    let args = ["fact", "add", "--workspace", "ws-1", "--actor", "actor-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("TEXT"), "missing-text error: {err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn delete_requires_at_least_one_fact_id() {
    let home = temp_home();
    let args = [
        "fact",
        "delete",
        "--workspace",
        "ws-1",
        "--actor",
        "actor-1",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("FACT_ID"), "missing-id error: {err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_requires_a_change() {
    assert_rejected_locally(
        &scoped(&["update"], &["--agent", "agent-1"], &["fact-1"]),
        "nothing to update",
    );
}

#[test]
fn update_rejects_blank_text() {
    assert_rejected_locally(
        &scoped(
            &["update"],
            &["--agent", "agent-1"],
            &["fact-1", "--text", "  ", "--metadata", "{}"],
        ),
        "must not be blank",
    );
}

#[test]
fn update_rejects_non_object_metadata() {
    assert_rejected_locally(
        &scoped(
            &["update"],
            &["--project", "proj-1"],
            &["fact-1", "--metadata", "[1]"],
        ),
        "must be a JSON object",
    );
}

#[test]
fn list_requires_at_least_one_filter() {
    assert_rejected_locally(
        &["fact", "list", "--workspace", "ws-1"],
        "at least one of --actors / --projects / --agents",
    );
}

#[test]
fn list_rejects_empty_filter_entries() {
    assert_rejected_locally(
        &["fact", "list", "--workspace", "ws-1", "--agents", "a,,b"],
        "empty entry",
    );
}

#[test]
fn list_rejects_more_than_fifty_owners() {
    let actors: Vec<String> = (0..30).map(|i| format!("actor-{i}")).collect();
    let agents: Vec<String> = (0..21).map(|i| format!("agent-{i}")).collect();
    let (actors, agents) = (actors.join(","), agents.join(","));
    assert_rejected_locally(
        &[
            "fact",
            "list",
            "--workspace",
            "ws-1",
            "--actors",
            actors.as_str(),
            "--agents",
            agents.as_str(),
        ],
        "at most 50",
    );
}

#[test]
fn list_page_size_must_be_within_range() {
    for size in ["0", "201"] {
        assert_rejected_locally(
            &[
                "fact",
                "list",
                "--workspace",
                "ws-1",
                "--actors",
                "a",
                "--page-size",
                size,
            ],
            "--page-size",
        );
    }
}

#[test]
fn list_rejects_an_empty_query() {
    assert_rejected_locally(
        &[
            "fact",
            "list",
            "--workspace",
            "ws-1",
            "--actors",
            "a",
            "--query",
            "",
        ],
        "must not be empty",
    );
}

#[test]
fn conflict_list_validates_filters_locally() {
    let scope = ["--project", "proj-1"];
    assert_rejected_locally(
        &scoped(&["conflict", "list"], &scope, &["--page-size", "101"]),
        "--page-size",
    );
    assert_rejected_locally(
        &scoped(&["conflict", "list"], &scope, &["--category", "m2x"]),
        "--category",
    );
    assert_rejected_locally(
        &scoped(&["conflict", "list"], &scope, &["--conflict-type", "fuzzy"]),
        "--conflict-type",
    );
    assert_rejected_locally(
        &scoped(&["conflict", "list"], &scope, &["--resolved", "maybe"]),
        "--resolved",
    );
}

#[test]
fn conflict_resolve_validates_the_strategy_and_its_inputs() {
    let scope = ["--actor", "actor-1"];
    let cases: &[(&[&str], &str)] = &[
        // Unknown strategies would earn an INTERNAL_ERROR from the server.
        (&["cfl-1", "--strategy", "bogus"], "--strategy"),
        (&["cfl-1", "--strategy", "keep_fact"], "--keep-fact-id"),
        (
            &["cfl-1", "--strategy", "dismiss", "--keep-fact-id", "fact-1"],
            "only accepted with --strategy keep_fact",
        ),
        (
            &[
                "cfl-1",
                "--strategy",
                "keep_fact",
                "--keep-fact-id",
                "f",
                "--edit",
                "f=t",
            ],
            "only accepted with --strategy edit_fact",
        ),
        (&["cfl-1", "--strategy", "edit_fact"], "--edit"),
        (
            &[
                "cfl-1",
                "--strategy",
                "edit_fact",
                "--edit",
                "f=a",
                "--edit",
                "f=b",
            ],
            "more than once",
        ),
        (
            &["cfl-1", "--strategy", "edit_fact", "--edit", "no-separator"],
            "FACT_ID=TEXT",
        ),
        (
            &["cfl-1", "--strategy", "edit_fact", "--edit", "f= "],
            "must not be blank",
        ),
    ];
    for (rest, expected) in cases {
        assert_rejected_locally(&scoped(&["conflict", "resolve"], &scope, rest), expected);
    }
}

#[test]
fn instruction_set_requires_exactly_one_valid_source() {
    let scope = ["--agent", "agent-1"];
    assert_rejected_locally(
        &scoped(&["instruction", "set"], &scope, &[]),
        "--file <PATH>",
    );
    assert_rejected_locally(
        &scoped(
            &["instruction", "set"],
            &scope,
            &["--text", "a", "--file", "x.md"],
        ),
        "not both",
    );
    assert_rejected_locally(
        &scoped(&["instruction", "set"], &scope, &["--text", " \n "]),
        "fact instruction clear",
    );
    let too_long = "x".repeat(2001);
    assert_rejected_locally(
        &scoped(
            &["instruction", "set"],
            &scope,
            &["--text", too_long.as_str()],
        ),
        "at most 2000",
    );
    assert_rejected_locally(
        &scoped(
            &["instruction", "set"],
            &scope,
            &["--file", "/nonexistent/memorylake/instruction.md"],
        ),
        "read instruction file",
    );
}

#[test]
fn instruction_draft_validates_its_inputs() {
    let scope = ["--project", "proj-1"];
    let long_language = "l".repeat(65);
    assert_rejected_locally(
        &scoped(
            &["instruction", "draft"],
            &scope,
            &["--language", long_language.as_str()],
        ),
        "at most 64",
    );
    let long_guidance = "g".repeat(2001);
    assert_rejected_locally(
        &scoped(
            &["instruction", "draft"],
            &scope,
            &["--guidance", long_guidance.as_str()],
        ),
        "at most 2000",
    );
}

#[test]
fn every_subcommand_without_login_fails() {
    let home = temp_home();
    let mut cases: Vec<Vec<&str>> = SCOPED_COMMANDS
        .iter()
        .map(|(command, rest)| scoped(command, &["--agent", "agent-1"], rest))
        .collect();
    cases.push(vec![
        "fact",
        "list",
        "--workspace",
        "ws-1",
        "--agents",
        "agent-1",
    ]);
    for args in cases {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains("not logged in"),
            "expected login error for {args:?}: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}
