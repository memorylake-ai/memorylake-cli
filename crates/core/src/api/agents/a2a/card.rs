//! Fetch an agent's A2A card
//! (`GET .../agents/{agent}/.well-known/agent-card.json`).

use serde_json::Value;

use crate::client::Client;
use crate::error::Result;

use super::{agent_card_path, describe_a2a_error};

/// Fetch the agent card: name, supported protocol bindings, capabilities and
/// the extensions the agent understands.
///
/// Returned as the server sends it. The card's `protocolVersion` reports the
/// default binding's version and is not the version this module speaks; the
/// per-binding truth is in `supportedInterfaces`.
pub fn get_agent_card(client: &Client, workspace_id: &str, agent_id: &str) -> Result<Value> {
    client
        .get_json_with_headers(&agent_card_path(workspace_id, agent_id), &[], &[])
        .map_err(describe_a2a_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_ok, one_shot_server};

    #[test]
    fn returns_the_card_verbatim_without_looking_for_an_envelope() {
        // The card is bare JSON with no `success` key.
        let card =
            r#"{"protocolVersion":"0.3","name":"Default Agent","capabilities":{"streaming":true}}"#;
        let (base_url, server) = one_shot_server(json_ok(card));
        let client = Client::new(base_url, "sk-test").unwrap();

        let value = get_agent_card(&client, "ws-1", "agt-1").expect("card");
        assert_eq!(value["name"], "Default Agent");
        assert_eq!(value["capabilities"]["streaming"], true);

        let captured = server.join().unwrap();
        assert!(
            captured.head.starts_with(
                "GET /api/v3/workspaces/ws-1/agents/agt-1/.well-known/agent-card.json "
            ),
            "{}",
            captured.head
        );
    }
}
