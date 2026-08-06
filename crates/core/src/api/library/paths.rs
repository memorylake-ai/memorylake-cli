//! URL paths for the Library endpoints.
//!
//! The resolved base URL already carries the `/openapi/memorylake` service
//! prefix, so paths here start at `/api/v1`. The published docs write the full
//! prefixed path; repeating it would produce a 404.

use crate::api::path::encode_segment;

/// Collection path for creating items and starting uploads.
pub(super) const ITEMS_PATH: &str = "/api/v1/drives/items";

/// Path to a single item.
pub(super) fn item_path(item_id: &str) -> String {
    format!("{ITEMS_PATH}/{}", encode_segment(item_id))
}

/// Path to a folder's children.
pub(super) fn children_path(item_id: &str) -> String {
    format!("{}/children", item_path(item_id))
}

/// Path for starting a chunked upload session.
pub(super) fn upload_path() -> String {
    format!("{ITEMS_PATH}/upload")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_path_preserves_the_colon_in_item_ids() {
        assert_eq!(
            item_path("sc-d68ddc76b98c4df4a3002cd53aecfc5b:inode-043abbb41bbd449fae79404c6"),
            "/api/v1/drives/items/sc-d68ddc76b98c4df4a3002cd53aecfc5b:inode-043abbb41bbd449fae79404c6"
        );
    }

    #[test]
    fn item_path_accepts_the_root_alias() {
        assert_eq!(item_path("MY_SPACE"), "/api/v1/drives/items/MY_SPACE");
    }

    #[test]
    fn children_path_appends_the_collection() {
        assert_eq!(
            children_path("MY_SPACE"),
            "/api/v1/drives/items/MY_SPACE/children"
        );
    }

    #[test]
    fn item_path_escapes_url_structural_chars() {
        assert_eq!(
            item_path("weird id/here?x"),
            "/api/v1/drives/items/weird%20id%2Fhere%3Fx"
        );
    }
}
