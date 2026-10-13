//! Team management API (`/admin/v1/*`).
//!
//! Governs the team the API key belongs to: the team itself, its API keys,
//! members, invitations, and usage — plus creating new teams — and who may
//! do what in it: roles, access groups, permission rules, access scopes, and
//! per-instance access. It also reads the team's billing and audit log. Same base URL and same key as every other
//! module here — the management endpoints simply live under `/admin/v1`
//! instead of `/api/v1`. The team is fixed by the key; nothing in a request
//! can address a different one.
//!
//! Write endpoints accept an optional `Idempotency-Key` header: retrying with
//! the same value replays the first result instead of repeating the write.
//! That matters most for [`api_keys::create_api_key`] and
//! [`api_keys::rotate_api_key`], whose responses carry a secret shown exactly
//! once.

mod access;
mod access_groups;
mod api_keys;
mod audit_logs;
mod billing;
mod invitations;
mod members;
mod path;
mod permissions;
mod roles;
mod scopes;
mod team;
mod types;
mod usage;

pub use access::{
    PermissionLevelOptions, ResourceAccess, ResourceKind, ResourceRef, Subject, add_scope_grant,
    get_permission_level_options, get_resource_access, remove_scope_grant, set_permission_level,
};
pub use access_groups::{
    AccessGroup, AccessGroupExplanation, AccessGroupMember, AccessGroupPermission,
    AccessGroupPermissions, AddedAccessGroupMembers, CreateAccessGroupRequest,
    UpdateAccessGroupRequest, add_access_group_members, create_access_group, delete_access_group,
    explain_access_group, get_access_group, get_access_group_scope, list_access_group_members,
    list_access_groups, remove_access_group_member, set_access_group_permissions,
    set_access_group_scope, update_access_group,
};
pub use permissions::{
    CatalogAction, CatalogResourceType, CreatePermissionRulesRequest, CreatedPermissionRules,
    ListPermissionRulesParams, MyPermissions, PermissionCatalog, PermissionRule,
    create_permission_rules, delete_permission_rule, get_my_permissions, get_permission_catalog,
    list_permission_rules,
};
pub use scopes::{
    AccessGroupScope, AccessScopeEntry, MemberAccessScope, RoleAccessScope, ScopeEntryInput,
    ScopeInputError, ScopeInstanceInput, SetScopesRequest, parse_scope_entries,
};

pub use api_keys::{
    CreateApiKeyRequest, ListApiKeysParams, UpdateApiKeyRequest, create_api_key, get_api_key,
    list_api_keys, revoke_api_key, rotate_api_key, update_api_key,
};
pub use audit_logs::{
    AuditLogEntry, ListAuditLogsParams, MAX_AUDIT_ACTIONS, MAX_AUDIT_ACTORS, MAX_AUDIT_TARGET_ID,
    MAX_AUDIT_TARGET_TYPE, MAX_AUDIT_WINDOW_SECS, list_audit_logs,
};
pub use billing::{
    AlertChannel, AlertThreshold, AutoTopup, AutoTopupAttempt, BalanceAlert, BalanceAlertEvent,
    BillingDetails, CreditBalance, CreditTransaction, Invoice, ListCreditTransactionsParams,
    ListOrdersParams, MAX_ALERT_PERCENT, MAX_ALERT_RECIPIENTS, MAX_ALERT_THRESHOLDS,
    MAX_ALERT_TOKENS, MAX_BILLING_ADDRESS, MAX_BILLING_FIELD, MAX_BILLING_TAX_ID, Money, Order,
    PaymentMethod, PlanChangePreview, SetBalanceAlertRequest, SetBillingDetailsRequest,
    Subscription, download_invoice_pdf, get_auto_topup, get_balance_alert, get_billing_details,
    get_credit_balance, get_invoice, get_order, get_subscription, list_auto_topup_attempts,
    list_balance_alert_events, list_credit_transactions, list_invoices, list_orders,
    list_payment_methods, preview_plan_change, reset_balance_alert, set_balance_alert,
    set_billing_details,
};
pub use invitations::{
    CreateInvitationRequest, ListInvitationsParams, create_invitation, get_invitation,
    list_invitations, revoke_invitation,
};
pub use members::{
    CreateMemberRequest, ListMembersParams, MAX_MEMBER_TOKENS, MemberExplanation,
    UpdateMemberRequest, create_member, explain_member, get_member, get_member_scope, list_members,
    remove_member, set_member_scope, update_member,
};
pub use roles::{
    CreateRoleRequest, Role, RoleDetail, RoleList, RolePermission, UpdateRoleRequest, create_role,
    delete_role, get_role, get_role_scope, list_roles, set_role_scope, update_role,
};
pub use team::{
    CreateTeamRequest, create_team, delete_team, get_team, rename_team, transfer_team_ownership,
};
pub use types::{
    ApiKey, ApiKeyCreated, ApiKeyOwner, CreatedTeam, DeletedTeam, Invitation, Member,
    OwnershipTransfer, Page, Team, Usage, UsageBucket, UsageByMember, UsageByModel, UsagePeriod,
    UsageQuota, UsageRecord, UsageRecordMember, UsageSummary, UsageSummaryWindow, UsageTotals,
};
pub use usage::{
    GetUsageParams, ListUsageRecordsParams, MAX_USAGE_WINDOW_DAYS, get_usage, get_usage_summary,
    list_usage_records,
};

/// Header that makes a retried write replay its first result.
const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";

/// Header that makes a write conditional on the `ETag` of an earlier read:
/// a resource changed since is refused with 412 and left untouched.
const IF_MATCH_HEADER: &str = "If-Match";

/// Render an optional idempotency key as the extra-headers slice the client
/// takes. `None` means "send nothing", not an empty header value.
fn idempotency_headers(key: Option<&str>) -> Vec<(&'static str, &str)> {
    key.map(|key| vec![(IDEMPOTENCY_KEY_HEADER, key)])
        .unwrap_or_default()
}

/// Headers of a write that may be retried and/or made conditional.
fn write_headers<'a>(
    idempotency_key: Option<&'a str>,
    if_match: Option<&'a str>,
) -> Vec<(&'static str, &'a str)> {
    let mut headers = idempotency_headers(idempotency_key);
    if let Some(etag) = if_match {
        headers.push((IF_MATCH_HEADER, etag));
    }
    headers
}

/// Append the optional audit-log `reason` some deletes take in the query.
fn push_reason(query: &mut Vec<(&'static str, String)>, reason: Option<&str>) {
    if let Some(reason) = reason {
        query.push(("reason", reason.to_string()));
    }
}

/// Discardable payload of writes that answer `{"success":true,"data":{}}`.
///
/// `serde_json::Value` rather than `()` or an empty struct: `{}` cannot
/// deserialize into `()`, and an empty struct would reject the equally valid
/// absent-`data` form (which decodes as `Value::Null`).
type EmptyData = serde_json::Value;
