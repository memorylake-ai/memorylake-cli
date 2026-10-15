//! Offline `router` tests: bodies are checked before credentials are read.

use std::fs;

use crate::common::{assert_failure, run, temp_home};

#[test]
fn chat_without_a_model_is_refused_before_credentials() {
    let home = temp_home();
    let args = ["router", "chat", "hi"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("--model"), "unexpected error output: {err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn messages_without_a_prompt_is_refused_before_credentials() {
    let home = temp_home();
    let args = ["router", "messages", "--model", "claude-haiku-4-5"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("nothing to send"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn raw_requires_stream() {
    let home = temp_home();
    let args = ["router", "chat", "hi", "--model", "m", "--raw"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("--stream"), "unexpected error output: {err}");
    let _ = fs::remove_dir_all(&home);
}
