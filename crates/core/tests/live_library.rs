//! Live Library integration tests against a real MemoryLake environment.
//!
//! Requires `MEMORYLAKE_API_KEY` (from the environment or repo-root `.env`).
//! Missing or empty key fails the tests (no skip).
//!
//! Every test works inside its own uniquely-named folder under the workspace
//! root and deletes that folder when it finishes. Cleanup runs on the normal
//! path only: a failing test leaves its scratch folder behind on purpose, so
//! the state can be inspected.

mod common;

use memorylake_core::Error;
use memorylake_core::api::library::{
    CreateFolderRequest, ListChildrenParams, NameConflictStrategy, ROOT_ALIAS, UploadFileRequest,
    create_folder, create_upload_session, delete_item, get_item, list_children, upload_file,
};

use common::{live_client, unique_name, write_temp_file};

/// A file that must span more than one part.
///
/// The server picks part sizes (5 MiB observed for files this size), so this is
/// a lower bound rather than an exact part count; the test asserts the real
/// count from the returned plan.
const MULTIPART_BYTES: u64 = 6 * 1024 * 1024;

#[test]
fn upload_session_plans_multiple_parts_for_a_large_file() {
    let client = live_client();

    // Creating a session uploads nothing, so this is a cheap way to pin down
    // the boundary the multipart test depends on.
    let single = create_upload_session(&client, 1).expect("session for 1 byte");
    assert_eq!(single.part_items.len(), 1);
    assert_eq!(single.part_items[0].size, 1);
    assert_eq!(single.part_items[0].number, 1);

    let multi = create_upload_session(&client, MULTIPART_BYTES).expect("session for 6 MiB");
    assert!(
        multi.part_items.len() > 1,
        "{MULTIPART_BYTES} bytes should span multiple parts, got {}",
        multi.part_items.len()
    );

    let planned: u64 = multi.part_items.iter().map(|part| part.size).sum();
    assert_eq!(planned, MULTIPART_BYTES, "plan must cover the whole file");
    for (index, part) in multi.part_items.iter().enumerate() {
        assert_eq!(part.number, index as u32 + 1, "parts are 1-based and dense");
        assert!(!part.upload_url.is_empty());
    }
}

#[test]
fn multipart_upload_round_trips_through_get() {
    let client = live_client();
    let folder = create_folder(
        &client,
        &CreateFolderRequest {
            parent_item_id: ROOT_ALIAS.to_string(),
            name: unique_name("core-multipart"),
            name_conflict_strategy: Some(NameConflictStrategy::Deny),
        },
    )
    .expect("create scratch folder");

    let (dir, path) = write_temp_file("core-multipart", MULTIPART_BYTES);
    let created = upload_file(
        &client,
        &UploadFileRequest {
            source: path,
            parent_item_id: folder.item_id.clone(),
            name: "big.bin".to_string(),
            name_conflict_strategy: None,
        },
    )
    .expect("upload multipart file");

    let fetched = get_item(&client, &created.item_id).expect("get uploaded file");
    assert!(fetched.is_file());
    assert_eq!(fetched.name, created.name);
    assert_eq!(
        fetched.size,
        Some(MULTIPART_BYTES),
        "every byte must land exactly once"
    );
    // S3 composite ETags end in `-<part count>`; anything else means the file
    // was assembled from a single part and the multipart path went untested.
    let etag = fetched.etag.as_deref().expect("file has an etag");
    assert!(
        etag.rsplit_once('-').is_some_and(|(_, n)| n != "1"),
        "expected a multi-part composite etag, got {etag}"
    );

    let _ = std::fs::remove_dir_all(&dir);
    delete_item(&client, &folder.item_id).expect("clean up scratch folder");
}

#[test]
fn single_part_upload_round_trips_through_get() {
    let client = live_client();
    let folder = create_folder(
        &client,
        &CreateFolderRequest {
            parent_item_id: ROOT_ALIAS.to_string(),
            name: unique_name("core-single"),
            name_conflict_strategy: Some(NameConflictStrategy::Deny),
        },
    )
    .expect("create scratch folder");

    let (dir, path) = write_temp_file("core-single", 1_024);
    let created = upload_file(
        &client,
        &UploadFileRequest {
            source: path,
            parent_item_id: folder.item_id.clone(),
            name: "small.bin".to_string(),
            name_conflict_strategy: None,
        },
    )
    .expect("upload single-part file");

    let fetched = get_item(&client, &created.item_id).expect("get uploaded file");
    assert_eq!(fetched.size, Some(1_024));

    let _ = std::fs::remove_dir_all(&dir);
    delete_item(&client, &folder.item_id).expect("clean up scratch folder");
}

#[test]
fn folder_round_trips_through_list() {
    let client = live_client();
    let name = unique_name("core-folder");
    let folder = create_folder(
        &client,
        &CreateFolderRequest {
            parent_item_id: ROOT_ALIAS.to_string(),
            name: name.clone(),
            name_conflict_strategy: Some(NameConflictStrategy::Deny),
        },
    )
    .expect("create scratch folder");
    assert_eq!(folder.name, name, "deny must not rename");

    let root = get_item(&client, ROOT_ALIAS).expect("get workspace root");
    assert!(root.is_directory());

    let listed = find_in_listing(&client, &root.item_id, &folder.item_id);
    let listed = listed.expect("new folder appears in the root listing");
    assert!(listed.is_directory());
    assert_eq!(listed.name, name);
    assert_eq!(
        listed.parent_item_id.as_deref(),
        Some(root.item_id.as_str())
    );

    // A fresh folder lists as empty rather than erroring.
    let children = list_children(&client, &folder.item_id, &ListChildrenParams::default())
        .expect("list empty folder");
    assert!(children.items.is_empty());

    delete_item(&client, &folder.item_id).expect("clean up scratch folder");
}

#[test]
fn deleting_a_folder_removes_its_contents() {
    let client = live_client();
    let folder = create_folder(
        &client,
        &CreateFolderRequest {
            parent_item_id: ROOT_ALIAS.to_string(),
            name: unique_name("core-cascade"),
            name_conflict_strategy: Some(NameConflictStrategy::Deny),
        },
    )
    .expect("create scratch folder");

    let (dir, path) = write_temp_file("core-cascade", 512);
    let child = upload_file(
        &client,
        &UploadFileRequest {
            source: path,
            parent_item_id: folder.item_id.clone(),
            name: "child.bin".to_string(),
            name_conflict_strategy: None,
        },
    )
    .expect("upload child file");
    let _ = std::fs::remove_dir_all(&dir);

    get_item(&client, &child.item_id).expect("child exists before delete");
    delete_item(&client, &folder.item_id).expect("delete folder");

    let after = get_item(&client, &child.item_id);
    assert!(
        after.is_err(),
        "deleting a folder must remove nested files, got {after:?}"
    );
}

#[test]
fn deny_strategy_reports_a_conflict() {
    let client = live_client();
    let name = unique_name("core-conflict");
    let request = CreateFolderRequest {
        parent_item_id: ROOT_ALIAS.to_string(),
        name: name.clone(),
        name_conflict_strategy: Some(NameConflictStrategy::Deny),
    };

    let folder = create_folder(&client, &request).expect("first create succeeds");

    // `name_conflict_strategy` only takes effect as a body field; if it were
    // sent as a query parameter the server would silently rename to `<name>_1`
    // and this would pass instead of conflicting.
    let conflict = create_folder(&client, &request);
    match conflict {
        Err(Error::Api { message, .. }) => {
            assert!(
                message.contains("DRIVE_ITEM_CONFLICT"),
                "expected a conflict error, got: {message}"
            );
        }
        other => panic!("expected a DRIVE_ITEM_CONFLICT error, got {other:?}"),
    }

    delete_item(&client, &folder.item_id).expect("clean up scratch folder");
}

#[test]
fn rename_strategy_returns_the_server_assigned_name() {
    let client = live_client();
    let name = unique_name("core-rename");
    let first = create_folder(
        &client,
        &CreateFolderRequest {
            parent_item_id: ROOT_ALIAS.to_string(),
            name: name.clone(),
            name_conflict_strategy: None,
        },
    )
    .expect("first create");

    let second = create_folder(
        &client,
        &CreateFolderRequest {
            parent_item_id: ROOT_ALIAS.to_string(),
            name: name.clone(),
            name_conflict_strategy: Some(NameConflictStrategy::Rename),
        },
    )
    .expect("second create renames");

    assert_eq!(first.name, name);
    assert_ne!(
        second.name, name,
        "the server appends a suffix, so its name is the authoritative one"
    );
    assert!(second.name.starts_with(&name));
    assert_ne!(first.item_id, second.item_id);

    delete_item(&client, &first.item_id).expect("clean up first folder");
    delete_item(&client, &second.item_id).expect("clean up second folder");
}

#[test]
fn workspace_root_cannot_be_deleted() {
    let client = live_client();
    let refused = delete_item(&client, ROOT_ALIAS);
    match refused {
        Err(Error::Api { message, .. }) => assert!(
            message.contains("ACCESS_DENIED"),
            "expected the root to be protected, got: {message}"
        ),
        other => panic!("deleting the workspace root must fail, got {other:?}"),
    }
}

#[test]
fn empty_file_is_rejected_before_a_session_is_created() {
    let client = live_client();
    let (dir, path) = write_temp_file("core-empty", 0);

    let err = upload_file(
        &client,
        &UploadFileRequest {
            source: path,
            parent_item_id: ROOT_ALIAS.to_string(),
            name: "empty.bin".to_string(),
            name_conflict_strategy: None,
        },
    )
    .expect_err("the API requires at least one byte");
    assert!(matches!(err, Error::EmptyUpload { .. }));

    let _ = std::fs::remove_dir_all(&dir);
}

/// Page through `parent` looking for `item_id`.
fn find_in_listing(
    client: &memorylake_core::Client,
    parent: &str,
    item_id: &str,
) -> Option<memorylake_core::api::library::Item> {
    let mut token = None;
    loop {
        let page = list_children(
            client,
            parent,
            &ListChildrenParams {
                page_size: Some(50),
                continuation_token: token,
            },
        )
        .expect("list children page");

        if let Some(found) = page.items.into_iter().find(|item| item.item_id == item_id) {
            return Some(found);
        }
        // No continuation token means that was the last page.
        token = Some(page.continuation_token?);
    }
}
