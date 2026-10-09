//! Wire-level `library` tests against a loopback stub of the API.
//!
//! Library item ids embed a `:` that must reach the wire unencoded, and the
//! extended-attribute endpoints carry their payload in two different places:
//! a JSON body for `set`, a comma-separated `key` query for `delete`.

use serde_json::{Value, json};

use crate::common::assert_success;
use crate::common::stub::{exchange, request_body, request_line};

const ITEM: &str = "sc-a:inode-b";
const VOID: &str = r#"{"success":true,"message":"Xattrs set successfully"}"#;
const CREATED: &str = r#"{"success":true,"data":{"uri":"drive://d/sc-a:inode-c","item_id":"sc-a:inode-c","name":"docs"}}"#;

fn body_of(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body is JSON")
}

#[test]
fn xattr_set_puts_the_map_and_confirms_the_keys() {
    let args = [
        "library",
        "xattr",
        "set",
        ITEM,
        "--attrs",
        r#"{"team":"core","owner":"me"}"#,
    ];
    let (request, output) = exchange(VOID, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PUT /api/v1/drives/items/sc-a:inode-b/xattrs HTTP/1.1"
    );
    assert_eq!(
        body_of(&request),
        json!({"x_attrs": {"team": "core", "owner": "me"}})
    );
    assert!(
        stdout.contains("owner, team") && stdout.contains(ITEM),
        "{stdout}"
    );
}

#[test]
fn xattr_delete_sends_the_keys_comma_separated() {
    let args = ["lib", "xattr", "delete", ITEM, "--keys", "team, owner"];
    let (request, output) = exchange(VOID, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "DELETE /api/v1/drives/items/sc-a:inode-b/xattrs?key=team%2Cowner HTTP/1.1"
    );
    assert!(stdout.contains("team, owner"), "{stdout}");
}

#[test]
fn mkdir_sends_xattrs_only_when_given() {
    let args = ["library", "mkdir", "docs"];
    let (request, output) = exchange(CREATED, &args);
    assert_success(&output, &args);
    assert_eq!(
        body_of(&request),
        json!({"item_type": "folder", "parent_item_id": "MY_SPACE", "name": "docs"})
    );

    let args = ["library", "mkdir", "docs", "--xattrs", r#"{"team":"core"}"#];
    let (request, output) = exchange(CREATED, &args);
    assert_success(&output, &args);
    assert_eq!(request_line(&request), "POST /api/v1/drives/items HTTP/1.1");
    assert_eq!(
        body_of(&request),
        json!({
            "item_type": "folder",
            "parent_item_id": "MY_SPACE",
            "name": "docs",
            "x_attrs": {"team": "core"}
        })
    );
}

#[test]
fn list_asks_for_the_named_xattr_keys() {
    let args = ["library", "list", ITEM, "--xattr-keys", "team,owner"];
    let (request, output) = exchange(r#"{"success":true,"data":{"items":[]}}"#, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v1/drives/items/sc-a:inode-b/children?with_xattr_keys=team%2Cowner HTTP/1.1"
    );
}
