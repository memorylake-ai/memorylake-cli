//! URL paths of the team management API.
//!
//! All of them are fixed except for one trailing id segment, which is
//! percent-encoded because key ids, principal ids, and invitation ids are
//! caller-supplied strings on get/revoke/update calls.

use crate::api::path::encode_segment;

pub(super) const TEAM: &str = "/admin/v1/team";
pub(super) const TEAM_TRANSFER_OWNERSHIP: &str = "/admin/v1/team/transfer-ownership";
pub(super) const TEAMS: &str = "/admin/v1/teams";
pub(super) const API_KEYS: &str = "/admin/v1/api-keys";
pub(super) const MEMBERS: &str = "/admin/v1/members";
pub(super) const INVITATIONS: &str = "/admin/v1/invitations";
pub(super) const ROLES: &str = "/admin/v1/roles";
pub(super) const USAGE: &str = "/admin/v1/usage";
pub(super) const USAGE_SUMMARY: &str = "/admin/v1/usage/summary";
pub(super) const USAGE_RECORDS: &str = "/admin/v1/usage/records";
pub(super) const ACCESS_GROUPS: &str = "/admin/v1/access-groups";
pub(super) const ACCESS: &str = "/admin/v1/access";
pub(super) const MY_PERMISSIONS: &str = "/admin/v1/me/permissions";
pub(super) const PERMISSION_CATALOG: &str = "/admin/v1/permission-catalog";
pub(super) const PERMISSION_RULES: &str = "/admin/v1/permission-rules";

pub(super) fn api_key(id: &str) -> String {
    format!("{API_KEYS}/{}", encode_segment(id))
}

pub(super) fn api_key_rotate(id: &str) -> String {
    format!("{}/rotate", api_key(id))
}

pub(super) fn member(principal_id: &str) -> String {
    format!("{MEMBERS}/{}", encode_segment(principal_id))
}

pub(super) fn member_scope(principal_id: &str) -> String {
    format!("{}/access-scope", member(principal_id))
}

pub(super) fn member_effective(principal_id: &str) -> String {
    format!("{}/effective-permissions", member(principal_id))
}

pub(super) fn invitation(id: &str) -> String {
    format!("{INVITATIONS}/{}", encode_segment(id))
}

pub(super) fn role(key: &str) -> String {
    format!("{ROLES}/{}", encode_segment(key))
}

pub(super) fn role_scope(key: &str) -> String {
    format!("{}/access-scope", role(key))
}

pub(super) fn access_group(id: &str) -> String {
    format!("{ACCESS_GROUPS}/{}", encode_segment(id))
}

pub(super) fn access_group_members(id: &str) -> String {
    format!("{}/members", access_group(id))
}

pub(super) fn access_group_member(id: &str, principal_id: &str) -> String {
    format!(
        "{}/{}",
        access_group_members(id),
        encode_segment(principal_id)
    )
}

pub(super) fn access_group_permissions(id: &str) -> String {
    format!("{}/permissions", access_group(id))
}

pub(super) fn access_group_scope(id: &str) -> String {
    format!("{}/access-scope", access_group(id))
}

pub(super) fn access_group_effective(id: &str) -> String {
    format!("{}/effective-permissions", access_group(id))
}

pub(super) fn permission_rule(id: &str) -> String {
    format!("{PERMISSION_RULES}/{}", encode_segment(id))
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
