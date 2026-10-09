//! Extended attributes on Library items
//! (`PUT` / `DELETE /api/v1/drives/items/{item_id}/xattrs`).
//!
//! Extended attributes are a flat `string → string` map on every file and
//! folder. Some keys are managed by the server (`x_source`,
//! `x_thumbnail_uri`, …). The spec says touching those is rejected; in
//! production both set and delete answer success and silently leave them
//! unchanged (measured 2026-10-09). System folders such as `MY_SPACE` refuse
//! changes with 403.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::client::Client;
use crate::error::{Error, Result};

use super::paths::xattrs_path;

#[derive(Debug, Serialize)]
struct SetXattrsBody<'a> {
    x_attrs: &'a BTreeMap<String, String>,
}

/// Add or overwrite extended attributes on an item.
///
/// Merge semantics: only the keys given are written, every other attribute
/// stays as it was. Remove keys with [`delete_xattrs`]. An empty map is
/// refused locally, because the server rejects it with 400.
pub fn set_xattrs(
    client: &Client,
    item_id: &str,
    attributes: &BTreeMap<String, String>,
) -> Result<()> {
    if attributes.is_empty() {
        return Err(Error::NoXattrs { action: "set" });
    }
    client.put_empty(
        &xattrs_path(item_id),
        &SetXattrsBody {
            x_attrs: attributes,
        },
    )
}

/// Remove extended attributes from an item by key.
///
/// Keys travel as one comma-separated `key` query parameter, so a key that
/// itself contains a comma cannot be named and is refused locally, as is an
/// empty list. A key the item does not carry is not an error.
pub fn delete_xattrs(client: &Client, item_id: &str, keys: &[String]) -> Result<()> {
    client.delete_empty_with_query(&xattrs_path(item_id), &[("key", key_param(keys)?)])
}

/// Join keys into the `key` parameter, refusing what it cannot express.
fn key_param(keys: &[String]) -> Result<String> {
    if keys.is_empty() {
        return Err(Error::NoXattrs { action: "delete" });
    }
    if let Some(bad) = keys.iter().find(|key| key.is_empty() || key.contains(',')) {
        return Err(Error::InvalidXattrKey { key: bad.clone() });
    }
    Ok(keys.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_ok, one_shot_server};

    #[test]
    fn set_puts_the_map_under_x_attrs() {
        let (base, server) = one_shot_server(json_ok(
            r#"{"success":true,"message":"Xattrs set successfully"}"#,
        ));
        let client = Client::new(base, "sk-test").unwrap();
        let attributes = BTreeMap::from([
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string()),
        ]);
        set_xattrs(&client, "sc-a:inode-b", &attributes).expect("set");

        let request = server.join().unwrap();
        assert!(
            request
                .head
                .starts_with("PUT /api/v1/drives/items/sc-a:inode-b/xattrs "),
            "{}",
            request.head
        );
        assert_eq!(request.body, br#"{"x_attrs":{"a":"1","b":"2"}}"#);
    }

    #[test]
    fn set_refuses_an_empty_map_without_a_request() {
        let client = Client::new("http://127.0.0.1:1", "sk-test").unwrap();
        let err = set_xattrs(&client, "sc-a:inode-b", &BTreeMap::new()).expect_err("empty");
        assert!(matches!(err, Error::NoXattrs { action: "set" }), "{err:?}");
    }

    #[test]
    fn keys_are_joined_with_commas() {
        assert_eq!(
            key_param(&["k1".to_string(), "k2".to_string()]).unwrap(),
            "k1,k2"
        );
    }

    #[test]
    fn keys_that_cannot_be_expressed_are_refused() {
        assert!(matches!(
            key_param(&[]),
            Err(Error::NoXattrs { action: "delete" })
        ));
        for bad in ["a,b", ""] {
            assert!(
                matches!(key_param(&[bad.to_string()]), Err(Error::InvalidXattrKey { ref key }) if key == bad),
                "{bad:?}"
            );
        }
    }
}
