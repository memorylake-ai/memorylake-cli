//! Shared workspace resource types.

use serde::{Deserialize, Serialize};

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
    pub metadata: Option<serde_json::Value>,
    /// Caller-defined stable external id.
    #[serde(default)]
    pub custom_id: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default)]
    pub updated_at: Option<String>,
}
