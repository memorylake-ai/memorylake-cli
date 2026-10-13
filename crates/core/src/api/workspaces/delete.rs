//! Delete a workspace (`DELETE /api/v3/workspaces/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::get::workspace_path;

/// `custom_id` of the workspace the platform creates for every team.
///
/// Other services look the team's default workspace up by this id, so
/// deleting it breaks them rather than just losing data; callers refuse it.
pub const DEFAULT_WORKSPACE_CUSTOM_ID: &str = "_sys_default_workspace";

/// Permanently delete a workspace.
///
/// Irreversible: everything the workspace holds goes with it. Needs the
/// `workspace:delete` permission. Success carries no payload, but the
/// response envelope is still checked, so a server-side failure is reported
/// as an error.
pub fn delete_workspace(client: &Client, id: &str) -> Result<()> {
    client.delete_empty(&workspace_path(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_ok, one_shot_server};

    #[test]
    fn delete_sends_a_delete_to_the_workspace() {
        let (base, server) = one_shot_server(json_ok(r#"{"success":true,"data":{}}"#));
        let client = Client::new(base, "sk-test").unwrap();
        delete_workspace(&client, "ws-1").expect("delete");

        let request = server.join().unwrap();
        assert!(
            request.head.starts_with("DELETE /api/v3/workspaces/ws-1 "),
            "{}",
            request.head
        );
    }
}
