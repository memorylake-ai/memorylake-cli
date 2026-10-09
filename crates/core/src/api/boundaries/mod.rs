//! Boundaries v3 API (`/api/v3/boundaries`).
//!
//! A boundary is a named, saved search scope inside a workspace: a set of
//! projects plus, optionally, one human actor and one agent whose memories are
//! in scope. Creating or changing one requires search permission on every
//! project, actor and agent it names, and each of them must belong to (be
//! bound to) the boundary's workspace.

mod create;
mod delete;
mod get;
mod list;
mod types;
mod update;

pub use create::{CreateBoundaryRequest, create_boundary};
pub use delete::delete_boundary;
pub use get::{get_boundary, get_boundary_by_custom_id};
pub use list::{BoundaryList, ListBoundariesParams, list_boundaries};
pub use types::Boundary;
pub use update::{UpdateBoundaryRequest, update_boundary};

use crate::api::path::encode_segment;

/// Boundary collection endpoint.
///
/// Paths are relative to the configured base URL, which already carries the
/// `/openapi/memorylake` prefix — it must not be repeated here.
const BOUNDARIES_PATH: &str = "/api/v3/boundaries";

/// `/api/v3/boundaries/{id}`
fn boundary_path(id: &str) -> String {
    format!("{BOUNDARIES_PATH}/{}", encode_segment(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_paths_are_relative_to_the_base_url() {
        assert_eq!(BOUNDARIES_PATH, "/api/v3/boundaries");
        assert_eq!(boundary_path("bnd-1"), "/api/v3/boundaries/bnd-1");
    }

    #[test]
    fn ids_cannot_escape_their_path_segment() {
        assert_eq!(
            boundary_path("my/boundary?x"),
            "/api/v3/boundaries/my%2Fboundary%3Fx"
        );
    }
}
