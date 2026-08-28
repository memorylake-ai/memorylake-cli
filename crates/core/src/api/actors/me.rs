//! The caller's own actor (`GET /api/v3/defaults/my-actor`).

use crate::client::Client;
use crate::error::{Error, Result};

use super::types::Actor;

/// Path of the endpoint. Takes no caller-supplied segment, so unlike
/// `actor_path` it needs no encoding.
const MY_ACTOR_PATH: &str = "/api/v3/defaults/my-actor";

/// Wire value of the API error code returned when the route does not exist.
const NOT_FOUND_CODE: &str = "NOT_FOUND";

/// Fetch the actor representing the API key making the call.
///
/// Every key has one, so a successful call always yields an actor and there is
/// no "no default actor" outcome to handle.
///
/// **A 200 does not mean the actor is bound to any workspace.** It frequently
/// is not: the actor is created with the account, while workspace membership is
/// a separate, explicit act. Callers that need an actor to write memories with
/// must still check `list_workspace_actors`, and must not assume this actor
/// appears there.
///
/// A `NOT_FOUND` is reported as an unsupported deployment rather than passed
/// through. Since the endpoint has no "missing" case of its own, a 404 can only
/// mean the server predates it — and a bare "NOT_FOUND" would otherwise read as
/// "you have no actor", which is the opposite of the truth.
pub fn get_my_actor(client: &Client) -> Result<Actor> {
    client
        .get_data(MY_ACTOR_PATH, &[])
        .map_err(|err| match err {
            Error::Api {
                code: Some(code),
                message,
            } if code == NOT_FOUND_CODE => Error::Api {
                message: format!(
                    "this MemoryLake deployment has no `{MY_ACTOR_PATH}` endpoint, so the \
                 calling key's actor cannot be looked up; upgrade the server or name \
                 the actor explicitly\n{message}"
                ),
                code: Some(code),
            },
            other => other,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_ok, one_shot_server};

    /// Captured from production on 2026-08-21, verbatim.
    const REAL_RESPONSE: &str = r#"{"success":true,"data":{"id":"actor-fd25f63e2bc0441f80b0e10fca8335fb","tags":[],"metadata":{},"custom_id":"user::e7f4fbd1149f44109589aece3310b0eb","actor_type":"HUMAN","display_name":"1594834522","created_at":"2026-07-07T05:11:09.50066Z","created_by":"user::e7f4fbd1149f44109589aece3310b0eb","updated_at":"2026-07-07T05:11:09.50066Z"}}"#;

    fn client_for(base_url: &str) -> Client {
        Client::new(base_url, "sk-test").expect("build client")
    }

    #[test]
    fn decodes_a_real_response() {
        let (base_url, handle) = one_shot_server(json_ok(REAL_RESPONSE));
        let actor = get_my_actor(&client_for(&base_url)).expect("decode real response");

        assert_eq!(actor.id, "actor-fd25f63e2bc0441f80b0e10fca8335fb");
        assert_eq!(
            actor.custom_id.as_deref(),
            Some("user::e7f4fbd1149f44109589aece3310b0eb")
        );
        assert_eq!(actor.actor_type, super::super::types::ActorType::Human);
        assert_eq!(actor.display_name, "1594834522");
        assert!(actor.tags.is_empty());

        let request = handle.join().expect("server thread");
        assert!(
            request.head.starts_with(&format!("GET {MY_ACTOR_PATH} ")),
            "{}",
            request.head
        );
    }

    #[test]
    fn preserves_an_actor_type_this_build_does_not_know() {
        // The crate decodes leniently everywhere else; this endpoint must not
        // be the one place a server-side addition breaks the command.
        let body = r#"{"success":true,"data":{"id":"act-1","actor_type":"SUPERVISOR","display_name":"Ada"}}"#;
        let (base_url, handle) = one_shot_server(json_ok(body));
        let actor = get_my_actor(&client_for(&base_url)).expect("decode unknown actor_type");

        assert_eq!(
            actor.actor_type,
            super::super::types::ActorType::Other("SUPERVISOR".to_string())
        );
        let _ = handle.join();
    }

    #[test]
    fn reports_a_missing_route_as_an_unsupported_deployment() {
        let body = r#"{"success":false,"message":"No static resource api/v3/defaults/my-actor.","error_code":"NOT_FOUND"}"#;
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (base_url, handle) = one_shot_server(response);

        let err = get_my_actor(&client_for(&base_url)).expect_err("404 must be an error");
        let rendered = err.to_string();
        assert!(
            rendered.contains("has no") && rendered.contains(MY_ACTOR_PATH),
            "the message must name the endpoint and blame the deployment: {rendered}"
        );
        // The server's own words are kept so the failure stays diagnosable.
        assert!(rendered.contains("No static resource"), "{rendered}");
        assert!(
            matches!(err, Error::Api { code: Some(ref code), .. } if code == NOT_FOUND_CODE),
            "the machine-readable code must survive the rewording"
        );
        let _ = handle.join();
    }

    #[test]
    fn other_api_errors_pass_through_untouched() {
        let body = r#"{"success":false,"message":"denied","error_code":"ACCESS_DENIED"}"#;
        let response = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (base_url, handle) = one_shot_server(response);

        let err = get_my_actor(&client_for(&base_url)).expect_err("403 must be an error");
        let rendered = err.to_string();
        assert!(rendered.contains("denied"), "{rendered}");
        assert!(
            !rendered.contains("deployment"),
            "only NOT_FOUND is reworded: {rendered}"
        );
        let _ = handle.join();
    }
}
