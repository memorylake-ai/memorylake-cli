//! Shared workspace resource types.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A MemoryLake workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// Server-assigned workspace id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional caller-defined metadata.
    #[serde(default)]
    pub metadata: Option<Value>,
    /// Caller-defined stable external id.
    #[serde(default)]
    pub custom_id: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Id of the user or agent that created the workspace. Absent for
    /// workspaces created before this was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Fields the CLI does not model yet, kept so printing a workspace never
    /// drops what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
