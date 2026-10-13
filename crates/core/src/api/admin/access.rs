//! Who can reach one resource instance (`/admin/v1/access/*`).
//!
//! Every shareable resource type answers the same five calls: list who can
//! access it and why, show and set a member's permission level on it, and
//! add it to or remove it from a member's access scope or an access group's
//! instance list.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::api::path::encode_segment;
use crate::client::Client;
use crate::error::Result;

use super::path;
use super::{idempotency_headers, push_reason};

/// A resource type that can be shared instance by instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// A workspace.
    Workspace,
    /// A project.
    Project,
    /// An actor (team-wide).
    Actor,
    /// An agent (team-wide).
    Agent,
    /// A memory boundary.
    Boundary,
    /// A database connection.
    DbConnection,
    /// A database datasource.
    DbDatasource,
    /// A skill.
    Skill,
    /// An actor's binding in one workspace.
    WorkspaceActor,
    /// An agent's binding in one workspace.
    WorkspaceAgent,
}

impl ResourceKind {
    /// The URL collection name, e.g. `db-connections`.
    fn collection(self) -> &'static str {
        match self {
            Self::Workspace | Self::WorkspaceActor | Self::WorkspaceAgent => "workspaces",
            Self::Project => "projects",
            Self::Actor => "actors",
            Self::Agent => "agents",
            Self::Boundary => "boundaries",
            Self::DbConnection => "db-connections",
            Self::DbDatasource => "db-datasources",
            Self::Skill => "skills",
        }
    }

    /// Whether the instance lives inside a workspace that the path names.
    pub fn needs_workspace(self) -> bool {
        matches!(self, Self::WorkspaceActor | Self::WorkspaceAgent)
    }

    /// Whether writes may name the instance's workspace (projects only), so
    /// the project can be shown by name in access scopes.
    pub fn takes_workspace_hint(self) -> bool {
        self == Self::Project
    }
}

/// One resource instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    /// Its type.
    pub kind: ResourceKind,
    /// Its id, as the data API returns it.
    pub id: String,
    /// For workspace actors and agents: the workspace (part of the path).
    /// For projects: the project's workspace (sent with writes). Ignored
    /// otherwise.
    pub workspace_id: Option<String>,
}

impl ResourceRef {
    /// `/admin/v1/access/<collection>/<id>`, with the workspace segment for
    /// workspace actors and agents.
    fn base(&self) -> String {
        let kind = self.kind;
        let id = encode_segment(&self.id);
        if kind.needs_workspace() {
            let workspace = encode_segment(self.workspace_id.as_deref().unwrap_or_default());
            let child = if kind == ResourceKind::WorkspaceActor {
                "actors"
            } else {
                "agents"
            };
            format!("{}/workspaces/{workspace}/{child}/{id}", path::ACCESS)
        } else {
            format!("{}/{}/{id}", path::ACCESS, kind.collection())
        }
    }

    /// The workspace to send in a write body: projects only.
    fn workspace_hint(&self) -> Option<&str> {
        self.kind
            .takes_workspace_hint()
            .then_some(self.workspace_id.as_deref())
            .flatten()
    }
}

/// Who a scope grant is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// A member, by `principal_id`.
    Member(String),
    /// An access group, by id.
    AccessGroup(String),
}

impl Subject {
    fn segments(&self) -> String {
        let (kind, id) = match self {
            Self::Member(id) => ("members", id),
            Self::AccessGroup(id) => ("access-groups", id),
        };
        format!("{kind}/{}", encode_segment(id))
    }
}

/// Who can access one instance and why, plus the members who cannot.
///
/// The per-subject rows are printed as the server sends them; their source
/// records vary by kind and the CLI only reports them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAccess {
    /// Resource type, e.g. `memorylake:workspace`.
    pub resource_type: String,
    /// Instance id; `<workspace_id>/<id>` for workspace actors and agents.
    pub instance_id: String,
    /// Members who can access the instance.
    pub allowed_count: i64,
    /// Members and access groups that reach the instance.
    #[serde(default)]
    pub allowed: Vec<Value>,
    /// Members who cannot.
    #[serde(default)]
    pub blocked: Vec<Value>,
    /// Permission levels the caller may set here; empty when the caller
    /// cannot share it.
    #[serde(default)]
    pub grantable_levels: Vec<String>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A member's level on one instance and what the caller may change it to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionLevelOptions {
    /// The level set here; absent when none is.
    #[serde(default)]
    pub current_level: Option<String>,
    /// Levels the caller may set (the current one excluded).
    #[serde(default)]
    pub settable_levels: Vec<String>,
    /// Whether the caller may remove the level (`none`).
    pub can_set_none: bool,
    /// Why the current level cannot be lowered: `outranked`,
    /// `last_manager`, or `no_permission`.
    #[serde(default)]
    pub blocked_reason: Option<String>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Serialize)]
struct SetLevelRequest<'a> {
    level: &'a str,
    reason: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<&'a str>,
}

#[derive(Serialize)]
struct ScopeGrantRequest<'a> {
    reason: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<&'a str>,
}

/// List every member and access group that can access the instance and why.
pub fn get_resource_access(client: &Client, resource: &ResourceRef) -> Result<ResourceAccess> {
    client.get_data(&resource.base(), &[])
}

/// Show a member's level on the instance and the levels the caller may set.
pub fn get_permission_level_options(
    client: &Client,
    resource: &ResourceRef,
    principal_id: &str,
) -> Result<PermissionLevelOptions> {
    let path = format!(
        "{}/permission-levels/{}/options",
        resource.base(),
        Subject::Member(principal_id.to_string()).segments()
    );
    client.get_data(&path, &[])
}

/// Set a member's final level on the instance: `read`, `write`, `manage`,
/// or `none`. `none` removes the level; it is not a deny. The last member
/// who can manage the instance cannot be lowered (409).
pub fn set_permission_level(
    client: &Client,
    resource: &ResourceRef,
    principal_id: &str,
    level: &str,
    reason: &str,
    idempotency_key: Option<&str>,
) -> Result<ResourceAccess> {
    let path = format!(
        "{}/permission-levels/{}",
        resource.base(),
        Subject::Member(principal_id.to_string()).segments()
    );
    client.put_data_with_headers(
        &path,
        &SetLevelRequest {
            level,
            reason,
            workspace_id: resource.workspace_hint(),
        },
        &idempotency_headers(idempotency_key),
    )
}

/// Add the instance to a limited member's access scope or to an access
/// group's instance list. Repeating it changes nothing.
pub fn add_scope_grant(
    client: &Client,
    resource: &ResourceRef,
    subject: &Subject,
    reason: &str,
    idempotency_key: Option<&str>,
) -> Result<ResourceAccess> {
    let path = format!("{}/scope-grants/{}", resource.base(), subject.segments());
    client.put_data_with_headers(
        &path,
        &ScopeGrantRequest {
            reason,
            workspace_id: resource.workspace_hint(),
        },
        &idempotency_headers(idempotency_key),
    )
}

/// Remove the instance from a member's access scope or an access group's
/// instance list (404 when it is not there). Removing a limited member's
/// last instance is refused (409): it would lift the limit instead.
pub fn remove_scope_grant(
    client: &Client,
    resource: &ResourceRef,
    subject: &Subject,
    reason: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<ResourceAccess> {
    let path = format!("{}/scope-grants/{}", resource.base(), subject.segments());
    let mut query = Vec::new();
    push_reason(&mut query, reason);
    client.delete_data_with_query_and_headers(&path, &query, &idempotency_headers(idempotency_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(kind: ResourceKind, id: &str, workspace: Option<&str>) -> ResourceRef {
        ResourceRef {
            kind,
            id: id.into(),
            workspace_id: workspace.map(str::to_string),
        }
    }

    #[test]
    fn every_kind_addresses_its_collection() {
        assert_eq!(
            at(ResourceKind::DbDatasource, "ds-1", None).base(),
            "/admin/v1/access/db-datasources/ds-1"
        );
        assert_eq!(
            at(ResourceKind::Project, "proj-1", Some("ws-1")).base(),
            "/admin/v1/access/projects/proj-1",
            "a project's workspace rides the body, not the path"
        );
        assert_eq!(
            at(ResourceKind::WorkspaceAgent, "ag-1", Some("ws-1")).base(),
            "/admin/v1/access/workspaces/ws-1/agents/ag-1"
        );
    }

    #[test]
    fn only_projects_send_their_workspace() {
        assert_eq!(
            at(ResourceKind::Project, "p", Some("ws")).workspace_hint(),
            Some("ws")
        );
        assert_eq!(
            at(ResourceKind::WorkspaceActor, "a", Some("ws")).workspace_hint(),
            None
        );
        assert_eq!(
            at(ResourceKind::Skill, "s", Some("ws")).workspace_hint(),
            None
        );
    }

    #[test]
    fn a_hostile_id_cannot_restructure_the_path() {
        assert_eq!(
            at(ResourceKind::Skill, "../team?x", None).base(),
            "/admin/v1/access/skills/..%2Fteam%3Fx"
        );
    }
}
