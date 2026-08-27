//! URL paths of the team management API.
//!
//! All of them are fixed except for one trailing id segment, which is
//! percent-encoded because key ids, principal ids, and invitation ids are
//! caller-supplied strings on revoke/update calls.

use crate::api::path::encode_segment;

pub(super) const TEAM: &str = "/admin/v1/team";
pub(super) const API_KEYS: &str = "/admin/v1/api-keys";
pub(super) const MEMBERS: &str = "/admin/v1/members";
pub(super) const INVITATIONS: &str = "/admin/v1/invitations";
pub(super) const ROLES: &str = "/admin/v1/roles";
pub(super) const USAGE: &str = "/admin/v1/usage";

pub(super) fn api_key(id: &str) -> String {
    format!("{API_KEYS}/{}", encode_segment(id))
}

pub(super) fn api_key_rotate(id: &str) -> String {
    format!("{}/rotate", api_key(id))
}

pub(super) fn member(principal_id: &str) -> String {
    format!("{MEMBERS}/{}", encode_segment(principal_id))
}

pub(super) fn invitation(id: &str) -> String {
    format!("{INVITATIONS}/{}", encode_segment(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_paths_take_the_typical_ids_verbatim() {
        assert_eq!(api_key("42"), "/admin/v1/api-keys/42");
        assert_eq!(api_key_rotate("42"), "/admin/v1/api-keys/42/rotate");
        assert_eq!(
            member("prin-b83fa7f09f19487f"),
            "/admin/v1/members/prin-b83fa7f09f19487f"
        );
        assert_eq!(invitation("7"), "/admin/v1/invitations/7");
    }

    #[test]
    fn a_hostile_id_cannot_restructure_the_path() {
        // These ids come from command-line arguments; an embedded `/` or `?`
        // must not address a different endpoint.
        assert_eq!(
            api_key("../members?x=1"),
            "/admin/v1/api-keys/..%2Fmembers%3Fx=1"
        );
    }
}
