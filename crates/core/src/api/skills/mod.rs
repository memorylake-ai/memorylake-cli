//! Skills v3 API (`/api/v3/skills`).
//!
//! A skill is a ZIP package (a `SKILL.md` plus whatever it needs) that agents
//! can be configured to use. Publishing one is two steps: upload the archive to
//! a pre-signed slot ([`upload_package`]), then create the skill or a new
//! version of it from the storage URI the slot handed out.
//!
//! Every published version goes through a security review. Only a version
//! whose review passed can be referenced from an agent version's `skills`
//! list; anything else is refused with `SKILL_NOT_USABLE`.

mod create;
mod create_version;
mod delete;
mod get;
mod get_version;
mod list;
mod list_versions;
mod package;
mod types;
mod update;

pub use create::{CreateSkillRequest, create_skill};
pub use create_version::{CreateSkillVersionRequest, create_skill_version};
pub use delete::delete_skill;
pub use get::get_skill;
pub use get_version::get_skill_version;
pub use list::{ListSkillsParams, SkillList, list_skills};
pub use list_versions::{ListSkillVersionsParams, SkillVersionList, list_skill_versions};
pub use package::{
    MAX_PACKAGE_BYTES, PackageUploadSlot, create_package_upload, upload_package, validate_package,
};
pub use types::{PackageRef, Skill, SkillVersion};
pub use update::{UpdateSkillRequest, update_skill};

use crate::api::path::encode_segment;

/// Skill collection endpoint.
///
/// Paths are relative to the configured base URL, which already carries the
/// `/openapi/memorylake` prefix — it must not be repeated here.
const SKILLS_PATH: &str = "/api/v3/skills";

/// `/api/v3/skills/package-uploads`
const PACKAGE_UPLOADS_PATH: &str = "/api/v3/skills/package-uploads";

/// `/api/v3/skills/{id}`
fn skill_path(id: &str) -> String {
    format!("{SKILLS_PATH}/{}", encode_segment(id))
}

/// `/api/v3/skills/{id}/versions`
fn skill_versions_path(id: &str) -> String {
    format!("{}/versions", skill_path(id))
}

/// `/api/v3/skills/{id}/versions/{version}`
fn skill_version_path(id: &str, version: u64) -> String {
    format!("{}/{version}", skill_versions_path(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_paths_are_relative_to_the_base_url() {
        assert_eq!(skill_path("skill-1"), "/api/v3/skills/skill-1");
        assert_eq!(
            skill_versions_path("skill-1"),
            "/api/v3/skills/skill-1/versions"
        );
        assert_eq!(
            skill_version_path("skill-1", 3),
            "/api/v3/skills/skill-1/versions/3"
        );
        assert_eq!(PACKAGE_UPLOADS_PATH, "/api/v3/skills/package-uploads");
    }

    #[test]
    fn ids_cannot_escape_their_path_segment() {
        assert_eq!(
            skill_path("../package-uploads"),
            "/api/v3/skills/..%2Fpackage-uploads"
        );
        assert_eq!(
            skill_version_path("a?b#c", 1),
            "/api/v3/skills/a%3Fb%23c/versions/1"
        );
    }
}
