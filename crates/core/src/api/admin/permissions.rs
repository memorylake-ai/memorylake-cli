//! The permission model: what the caller holds, what exists, and the rules
//! granting it (`/admin/v1/me/permissions`, `/admin/v1/permission-catalog`,
//! `/admin/v1/permission-rules`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::types::{Page, push_opt, push_page_query};
use super::{EmptyData, idempotency_headers, push_reason};

/// What the caller may do in this team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MyPermissions {
    /// The caller's role.
    pub role: String,
    /// Every permission held, as `<resource_type>:<action>`.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Per scopable resource type: every instance, or only the listed ones.
    #[serde(default)]
    pub scopes: Vec<Value>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The resource types and actions permission rules can name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionCatalog {
    /// Resource types and their actions. Switched-off features are left out.
    #[serde(default)]
    pub resource_types: Vec<CatalogResourceType>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One resource type of the [`PermissionCatalog`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogResourceType {
    /// Resource type, as permission rules and scopes name it.
    pub resource_type: String,
    /// Actions available on it.
    #[serde(default)]
    pub actions: Vec<CatalogAction>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One action of a [`CatalogResourceType`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogAction {
    /// Action key, as rules name it.
    pub action: String,
    /// Deletes or irreversibly changes data.
    pub destructive: bool,
    /// false: only the owner holds it; it cannot be granted.
    pub grantable: bool,
    /// Actions this one needs to be useful.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerequisites: Option<Vec<Value>>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A permission rule on a role or a member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRule {
    /// Opaque id; pass it to [`delete_permission_rule`].
    pub rule_id: String,
    /// `role` or `member`.
    pub subject_type: String,
    /// Role key or `principal_id`.
    pub subject_id: String,
    /// Resource type, or one instance as `<resource_type>:<instance_id>`.
    pub resource: String,
    /// Action.
    pub action: String,
    /// `allow` or `deny`. Only allow rules can be created.
    pub effect: String,
    /// false for the built-in roles' rules.
    pub editable: bool,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Rules just created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedPermissionRules {
    /// One rule per distinct action.
    #[serde(default)]
    pub rules: Vec<PermissionRule>,
    /// How many extra rules were added for prerequisites.
    pub prerequisite_rule_count: i64,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Request body for allowing actions on a resource for a custom role or a
/// member.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CreatePermissionRulesRequest {
    /// `role` or `member`.
    pub subject_type: String,
    /// A custom role key, or a member's `principal_id`. Built-in roles
    /// cannot be changed.
    pub subject_id: String,
    /// Resource type, or one instance as `<resource_type>:<instance_id>`.
    pub resource: String,
    /// Actions to allow; one rule per distinct action.
    pub actions: Vec<String>,
    /// Also add actions these depend on that the subject lacks; without it
    /// such a request is refused (409) naming what is missing.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub auto_grant_prerequisites: bool,
    /// Why; recorded in the audit log.
    pub reason: String,
}

/// Query parameters for listing permission rules.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListPermissionRulesParams {
    /// `role` or `member`.
    pub subject_type: Option<String>,
    /// Only this subject's rules; needs `subject_type`.
    pub subject_id: Option<String>,
    /// Page size, 1–100.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

/// Return what the caller (the key's owner) may do in this team. Any member
/// may read their own.
pub fn get_my_permissions(client: &Client) -> Result<MyPermissions> {
    client.get_data(path::MY_PERMISSIONS, &[])
}

/// Return the permission catalog. With `access_group_only`, only what can be
/// granted to an access group (needs `access_group:view`).
pub fn get_permission_catalog(
    client: &Client,
    access_group_only: bool,
) -> Result<PermissionCatalog> {
    let mut query = Vec::new();
    if access_group_only {
        query.push(("grantable_to", "access_group".to_string()));
    }
    client.get_data(path::PERMISSION_CATALOG, &query)
}

/// List the team's permission rules on roles (built-ins included) and
/// members. Scopes, permission levels, and access-group permissions have
/// their own endpoints and are not listed here.
pub fn list_permission_rules(
    client: &Client,
    params: &ListPermissionRulesParams,
) -> Result<Page<PermissionRule>> {
    let mut query = Vec::new();
    push_opt(&mut query, "subject_type", params.subject_type.as_ref());
    push_opt(&mut query, "subject_id", params.subject_id.as_ref());
    push_page_query(
        &mut query,
        params.page_size,
        params.continuation_token.as_deref(),
    );
    client.get_data(path::PERMISSION_RULES, &query)
}

/// Allow actions on a resource for a custom role or a member, one rule per
/// action. Deny rules cannot be created: to take a permission away, delete
/// the rule that grants it.
pub fn create_permission_rules(
    client: &Client,
    request: &CreatePermissionRulesRequest,
    idempotency_key: Option<&str>,
) -> Result<CreatedPermissionRules> {
    client.post_data_with_headers(
        path::PERMISSION_RULES,
        request,
        &idempotency_headers(idempotency_key),
    )
}

/// Delete one rule. The built-in admin and member roles' rules cannot be
/// deleted (409).
pub fn delete_permission_rule(
    client: &Client,
    rule_id: &str,
    reason: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<()> {
    let mut query = Vec::new();
    push_reason(&mut query, reason);
    client
        .delete_data_with_query_and_headers::<EmptyData>(
            &path::permission_rule(rule_id),
            &query,
            &idempotency_headers(idempotency_key),
        )
        .map(|_| ())
}
