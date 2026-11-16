//! Browser login: an OAuth authorization that ends in an API key.
//!
//! The user approves the login in a browser; the authorization server sends
//! the browser back to a loopback address this process listens on; the code
//! that arrives there is exchanged for a short-lived access token, and that
//! token for an API key. From then on the key is used like one pasted in, so
//! nothing else in the CLI knows this flow exists.
//!
//! The access token is good for one call and a few minutes and comes with no
//! refresh token, which is why nothing here stores, refreshes or revokes it.
//!
//! This module holds the protocol: the URLs, the requests, and how a callback
//! is read. Listening on the loopback address and opening the browser belong
//! to the binary.

use std::fmt;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::{ACCEPT, AUTHORIZATION};
use reqwest::{StatusCode, Url};
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::Value;

use crate::credentials::IssuedKey;
use crate::error::{Error, Result};

/// The CLI's registered OAuth client.
///
/// Fixed rather than registered on the fly: the scope below yields a
/// long-lived credential, and the server grants it only to this client, whose
/// redirect address must be a loopback one. An authorization code for it can
/// therefore only ever be delivered to the machine the browser runs on.
pub const CLIENT_ID: &str = "memorylake-cli";

/// The only scope a login asks for: permission to create one API key.
pub const SCOPE: &str = "memlake:apikey";

/// Path the authorization server sends the browser back to.
pub const CALLBACK_PATH: &str = "/callback";

const AUTHORIZE_PATH: &str = "/oauth2/mcp/authorize";
const TOKEN_PATH: &str = "/oauth2/mcp/token";
const API_KEYS_PATH: &str = "/oauth2/cli/api-keys";
const REVOKE_PATH: &str = "/oauth2/cli/api-keys/revoke";

/// Longest key name the server accepts, in characters.
pub const MAX_KEY_NAME_CHARS: usize = 100;

/// Longest device label the server accepts, in characters.
pub const MAX_DEVICE_CHARS: usize = 64;

/// Per-request limit for the login endpoints.
///
/// They answer at once or not at all, and a person is waiting on the result.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Limit for revoking a key on the way out.
///
/// Shorter, because `auth logout` should not hang on a network that is down:
/// the local credentials are removed either way.
const REVOKE_TIMEOUT: Duration = Duration::from_secs(10);

/// A PKCE verifier and the challenge derived from it (RFC 7636, `S256`).
///
/// The challenge travels through the browser; the verifier goes straight to
/// the token endpoint. A code lifted from the browser is useless without it.
pub struct Pkce {
    verifier: String,
    challenge: String,
}

impl Pkce {
    /// A fresh verifier from the operating system's random source.
    pub fn generate() -> Result<Self> {
        // 32 bytes encode to 43 characters, the shortest verifier allowed.
        Ok(Self::from_verifier(random_token(32)?))
    }

    fn from_verifier(verifier: String) -> Self {
        let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()));
        Self {
            verifier,
            challenge,
        }
    }

    /// The value sent to the authorization endpoint.
    pub fn challenge(&self) -> &str {
        &self.challenge
    }
}

impl fmt::Debug for Pkce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pkce")
            .field("verifier", &"<redacted>")
            .field("challenge", &self.challenge)
            .finish()
    }
}

/// A random `state` value tying a callback to the login that asked for it.
pub fn random_state() -> Result<String> {
    random_token(24)
}

fn random_token(bytes: usize) -> Result<String> {
    let mut buffer = vec![0u8; bytes];
    SystemRandom::new()
        .fill(&mut buffer)
        .map_err(|_| Error::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// The redirect address for a loopback listener on `port`.
///
/// Always the literal `127.0.0.1`: the server compares host and path exactly
/// and ignores only the port, and `localhost` can resolve somewhere else.
pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}{CALLBACK_PATH}")
}

/// What a request to the loopback listener turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Callback {
    /// Not the callback path at all: a favicon request, a port scan.
    NotCallback,
    /// The callback path, but without this login's `state`.
    ///
    /// Not this login's answer, whatever else it carries: another page can
    /// make a browser request this address, and only the page the
    /// authorization server redirected can know the state.
    WrongState,
    /// The user declined on the consent page.
    Denied,
    /// The authorization server reported another error.
    Refused {
        /// OAuth error code.
        error: String,
        /// Human-readable detail, when the server gave one.
        description: Option<String>,
    },
    /// The authorization code to exchange.
    Code(String),
}

/// Read a request target (`/callback?code=…&state=…`) as a login callback.
pub fn parse_callback(target: &str, expected_state: &str) -> Callback {
    // A request target is not a URL on its own; any base makes it one.
    let Ok(url) = Url::parse(&format!("http://127.0.0.1{target}")) else {
        return Callback::NotCallback;
    };
    if url.path() != CALLBACK_PATH {
        return Callback::NotCallback;
    }

    let param = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .filter(|value| !value.is_empty())
    };

    let state_matches =
        param("state").is_some_and(|state| constant_time_eq(&state, expected_state));
    if !state_matches {
        return Callback::WrongState;
    }
    if let Some(error) = param("error") {
        return if error == "access_denied" {
            Callback::Denied
        } else {
            Callback::Refused {
                error,
                description: param("error_description"),
            }
        };
    }
    match param("code") {
        Some(code) => Callback::Code(code),
        None => Callback::Refused {
            error: "invalid_response".into(),
            description: Some("the callback carried neither a code nor an error".into()),
        },
    }
}

/// Compare without stopping at the first difference, so the time taken says
/// nothing about how much of a guessed state was right.
fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |diff, (x, y)| diff | (x ^ y))
            == 0
}

/// A short-lived access token. Never printed.
pub struct AccessToken(String);

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken(<redacted>)")
    }
}

/// An API key a browser login created, and what the server said about it.
pub struct IssuedApiKey {
    /// The key itself. Returned once; never printed.
    pub key: String,
    /// What to remember about the key alongside it.
    pub issued: IssuedKey,
}

impl fmt::Debug for IssuedApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IssuedApiKey")
            .field("key", &"<redacted>")
            .field("issued", &self.issued)
            .finish()
    }
}

/// How an attempt to revoke a key ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revocation {
    /// The key no longer works: revoked now, or already expired or revoked.
    Revoked,
    /// The server will not revoke this key through the login endpoint,
    /// because a login did not create it. It is still valid.
    NotIssuedByLogin,
}

/// Client for the login endpoints of one MemoryLake deployment.
#[derive(Debug, Clone)]
pub struct LoginClient {
    http: HttpClient,
    origin: String,
}

impl LoginClient {
    /// A client for the deployment whose API lives at `base_url`.
    ///
    /// The login endpoints sit at the root of the same host as the API
    /// (`https://app.memorylake.ai/openapi/memorylake` →
    /// `https://app.memorylake.ai/oauth2/…`), so the API base URL is all that
    /// is needed to find them, on a deployment of one's own as much as on the
    /// hosted ones.
    pub fn new(base_url: &str) -> Result<Self> {
        let origin = authorization_origin(base_url)?;
        let http = HttpClient::builder()
            .timeout(REQUEST_TIMEOUT)
            // The login endpoints answer directly. A redirect would send a
            // code, a token or a key somewhere this client was not pointed at.
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { http, origin })
    }

    /// Scheme, host and port the login endpoints are served from.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The URL to open in the browser.
    pub fn authorize_url(&self, redirect_uri: &str, pkce: &Pkce, state: &str) -> Result<String> {
        let mut url = self.endpoint(AUTHORIZE_PATH)?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", CLIENT_ID)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", SCOPE)
            .append_pair("code_challenge", pkce.challenge())
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state);
        Ok(url.into())
    }

    /// Exchange an authorization code for an access token.
    ///
    /// `redirect_uri` must be the one the code was issued for, to the byte.
    pub fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        pkce: &Pkce,
    ) -> Result<AccessToken> {
        let response = self
            .http
            .post(self.endpoint(TOKEN_PATH)?)
            .header(ACCEPT, "application/json")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", CLIENT_ID),
                ("code_verifier", pkce.verifier.as_str()),
            ])
            .send()?;
        let status = response.status();
        let body = response.text()?;
        tracing::debug!(status = status.as_u16(), "token endpoint answered");

        if !status.is_success() {
            let failure = Failure::read(status, &body);
            let message = if failure.code.as_deref() == Some("invalid_grant") {
                format!(
                    "the authorization code was refused (it expires quickly and works once); \
                     run `memorylake auth login` again\n{failure}"
                )
            } else if failure.endpoint_missing() {
                self.no_browser_login(&failure)
            } else {
                format!("could not finish the login\n{failure}")
            };
            return Err(failure.into_error(message));
        }

        let token = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| value.get("access_token")?.as_str().map(str::to_string))
            .filter(|token| !token.is_empty())
            .ok_or_else(|| Error::Api {
                message: format!(
                    "the token endpoint of {} answered HTTP {status} without an access token",
                    self.origin
                ),
                code: None,
            })?;
        Ok(AccessToken(token))
    }

    /// Spend the access token on an API key named `name`.
    ///
    /// Sent once and never repeated: the token is dead after the first call,
    /// whether or not the answer arrives, so a second attempt can only be
    /// refused. A failure here means logging in again.
    pub fn issue_api_key(
        &self,
        token: &AccessToken,
        name: &str,
        device: Option<&str>,
    ) -> Result<IssuedApiKey> {
        let mut request = serde_json::Map::new();
        request.insert("name".into(), Value::String(name.to_string()));
        if let Some(device) = device {
            request.insert("device".into(), Value::String(device.to_string()));
        }

        let response = self
            .http
            .post(self.endpoint(API_KEYS_PATH)?)
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, bearer(&token.0)?)
            .json(&request)
            .send()?;
        let status = response.status();
        let body = response.text()?;
        tracing::debug!(status = status.as_u16(), "API key endpoint answered");

        if !status.is_success() {
            let failure = Failure::read(status, &body);
            let message = match (status, failure.code.as_deref()) {
                (StatusCode::UNAUTHORIZED, _) => format!(
                    "the login expired before it could finish; run `memorylake auth login` again\n{failure}"
                ),
                (StatusCode::FORBIDDEN, Some("access_denied")) => format!(
                    "you are not allowed to create API keys in the team you picked; ask a team \
                     admin, or log in again and pick another team\n{failure}"
                ),
                (StatusCode::TOO_MANY_REQUESTS, _) => format!(
                    "MemoryLake is limiting how often logins may be attempted; wait a moment, \
                     then run `memorylake auth login` again\n{failure}"
                ),
                _ if failure.endpoint_missing() => self.no_browser_login(&failure),
                _ => format!("could not create an API key for this login\n{failure}"),
            };
            return Err(failure.into_error(message));
        }

        read_issued_key(&body, name).ok_or_else(|| Error::Api {
            // The body is deliberately left out: it may hold the key.
            message: format!(
                "the API key endpoint of {} answered HTTP {status} without a usable key",
                self.origin
            ),
            code: None,
        })
    }

    /// Revoke `api_key`, authenticating with the key itself.
    pub fn revoke_api_key(&self, api_key: &str) -> Result<Revocation> {
        let response = self
            .http
            .post(self.endpoint(REVOKE_PATH)?)
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, bearer(api_key)?)
            .timeout(REVOKE_TIMEOUT)
            .send()?;
        let status = response.status();
        let body = response.text()?;
        tracing::debug!(status = status.as_u16(), "revoke endpoint answered");

        match status {
            // Success counts only when the revoke endpoint is what answered.
            // A deployment without it can serve its web app for any path, as
            // a 200 carrying a page, and the key is then as valid as before.
            status if status.is_success() && !answered_by_an_endpoint(&body) => Err(Error::Api {
                message: format!(
                    "could not revoke the API key: {} answered HTTP {status} with something \
                     other than the revoke endpoint's reply",
                    self.origin
                ),
                code: None,
            }),
            status if status.is_success() => Ok(Revocation::Revoked),
            // A key the server no longer recognizes is as revoked as it gets.
            StatusCode::UNAUTHORIZED => Ok(Revocation::Revoked),
            StatusCode::FORBIDDEN => Ok(Revocation::NotIssuedByLogin),
            _ => {
                let failure = Failure::read(status, &body);
                let message = format!("could not revoke the API key\n{failure}");
                Err(failure.into_error(message))
            }
        }
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        Url::parse(&format!("{}{path}", self.origin)).map_err(|err| Error::Api {
            message: format!("could not build a login URL from {}: {err}", self.origin),
            code: None,
        })
    }

    fn no_browser_login(&self, failure: &Failure) -> String {
        format!(
            "{} does not offer browser login; log in with an API key instead \
             (`memorylake auth login --api-key …`)\n{failure}",
            self.origin
        )
    }
}

/// Scheme, host and port of `base_url`, where the login endpoints live.
pub fn authorization_origin(base_url: &str) -> Result<String> {
    let invalid = |reason: &str| Error::InvalidBaseUrl {
        url: base_url.trim().to_string(),
        origin: None,
        reason: reason.to_string(),
    };
    let url = Url::parse(base_url.trim()).map_err(|err| invalid(&err.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid(
            "browser login needs an http:// or https:// URL with a host",
        ));
    }
    Ok(url.origin().ascii_serialization())
}

/// Whether a successful response's body can have come from a login endpoint:
/// nothing at all (`204 No Content`), or JSON. A web page is neither.
fn answered_by_an_endpoint(body: &str) -> bool {
    body.trim().is_empty() || serde_json::from_str::<Value>(body).is_ok()
}

fn bearer(secret: &str) -> Result<reqwest::header::HeaderValue> {
    let mut value =
        reqwest::header::HeaderValue::from_str(&format!("Bearer {secret}")).map_err(|_| {
            Error::Api {
                message: "the credential holds a character that cannot be sent in a header".into(),
                code: None,
            }
        })?;
    value.set_sensitive(true);
    Ok(value)
}

/// Pick the key and its description out of the endpoint's answer.
///
/// Read field by field rather than into a struct: the id has been seen as a
/// string and may as well be a number, and only the key itself is needed for
/// the login to work. The answer is also accepted wrapped in the API's usual
/// `data` envelope.
fn read_issued_key(body: &str, requested_name: &str) -> Option<IssuedApiKey> {
    let value: Value = serde_json::from_str(body).ok()?;
    let fields = match value.get("data") {
        Some(data) if value.get("key").is_none() && data.is_object() => data,
        _ => &value,
    };

    let text = |name: &str| match fields.get(name)? {
        Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    };
    let expires_at = match fields.get("expires_at") {
        Some(Value::Number(number)) => number.as_i64(),
        Some(Value::String(text)) => text.trim().parse().ok(),
        _ => None,
    };

    Some(IssuedApiKey {
        key: text("key")?,
        issued: IssuedKey {
            id: text("id").unwrap_or_default(),
            name: text("name").unwrap_or_else(|| requested_name.to_string()),
            expires_at: expires_at.filter(|at| *at > 0),
            tenant_id: text("tenant_id"),
        },
    })
}

/// A refusal from a login endpoint, reduced to what is safe and useful to show.
struct Failure {
    status: StatusCode,
    code: Option<String>,
    description: Option<String>,
    /// Whether the body was JSON at all. A web page here means the request
    /// never reached a login endpoint.
    json: bool,
}

impl Failure {
    /// Read an OAuth error (`error`, `error_description`), falling back to the
    /// API's own envelope (`error_code`, `message`) and to the gateway's
    /// nested form (`error.type`, `error.message`).
    fn read(status: StatusCode, body: &str) -> Self {
        let value = serde_json::from_str::<Value>(body).ok();
        let text = |path: &[&str]| {
            let mut node = value.as_ref()?;
            for key in path {
                node = node.get(key)?;
            }
            node.as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        Self {
            status,
            code: text(&["error"])
                .or_else(|| text(&["error_code"]))
                .or_else(|| text(&["error", "type"])),
            description: text(&["error_description"])
                .or_else(|| text(&["message"]))
                .or_else(|| text(&["error", "message"])),
            json: value.is_some(),
        }
    }

    /// Whether this looks like a deployment without the endpoint, as opposed
    /// to an endpoint that said no.
    fn endpoint_missing(&self) -> bool {
        matches!(
            self.status,
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        ) || (!self.json && !self.status.is_server_error())
    }

    fn into_error(self, message: String) -> Error {
        Error::Api {
            message,
            code: self.code,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP {}", self.status)?;
        match (&self.code, &self.description) {
            (Some(code), Some(description)) => write!(f, ": {description} [{code}]"),
            (Some(code), None) => write!(f, " [{code}]"),
            (None, Some(description)) => write!(f, ": {description}"),
            (None, None) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::one_shot_server;

    fn response(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    fn pkce() -> Pkce {
        Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into())
    }

    fn api_error(result: Result<impl fmt::Debug>) -> (String, Option<String>) {
        match result {
            Err(Error::Api { message, code }) => (message, code),
            other => panic!("expected an API error, got {other:?}"),
        }
    }

    #[test]
    fn the_challenge_is_the_s256_of_the_verifier() {
        // The worked example from RFC 7636, appendix B.
        assert_eq!(
            pkce().challenge(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_generated_verifier_is_long_enough_and_never_repeats() {
        let first = Pkce::generate().expect("random");
        let second = Pkce::generate().expect("random");
        assert_eq!(first.verifier.len(), 43, "RFC 7636 asks for 43 to 128");
        assert!(
            first
                .verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert_ne!(first.verifier, second.verifier);
        assert_ne!(
            random_state().expect("random"),
            random_state().expect("random")
        );
    }

    #[test]
    fn debug_output_holds_no_secret() {
        let rendered = format!(
            "{:?} {:?} {:?}",
            pkce(),
            AccessToken("at-secret".into()),
            IssuedApiKey {
                key: "sk-secret".into(),
                issued: IssuedKey {
                    id: "1".into(),
                    name: "n".into(),
                    expires_at: None,
                    tenant_id: None,
                },
            }
        );
        for secret in ["dBjftJeZ4CVP", "at-secret", "sk-secret"] {
            assert!(!rendered.contains(secret), "{secret} leaked: {rendered}");
        }
    }

    #[test]
    fn the_login_endpoints_sit_at_the_root_of_the_api_host() {
        for (base_url, origin) in [
            (
                "https://app.memorylake.ai/openapi/memorylake",
                "https://app.memorylake.ai",
            ),
            (
                "https://app.memorylake.cn/openapi/memorylake/",
                "https://app.memorylake.cn",
            ),
            (
                "http://127.0.0.1:8080/openapi/memorylake",
                "http://127.0.0.1:8080",
            ),
            ("https://ml.example.com:8443", "https://ml.example.com:8443"),
        ] {
            assert_eq!(authorization_origin(base_url).expect(base_url), origin);
        }
        assert!(matches!(
            authorization_origin("ftp://example.com/x"),
            Err(Error::InvalidBaseUrl { .. })
        ));
    }

    #[test]
    fn the_authorize_url_asks_for_one_api_key_and_nothing_else() {
        let client =
            LoginClient::new("https://app.memorylake.ai/openapi/memorylake").expect("client");
        let url = client
            .authorize_url(&redirect_uri(51234), &pkce(), "state-1")
            .expect("url");
        assert_eq!(
            url,
            "https://app.memorylake.ai/oauth2/mcp/authorize\
             ?response_type=code\
             &client_id=memorylake-cli\
             &redirect_uri=http%3A%2F%2F127.0.0.1%3A51234%2Fcallback\
             &scope=memlake%3Aapikey\
             &code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM\
             &code_challenge_method=S256\
             &state=state-1"
        );
        // A launcher that scans output for a URL must get all of it: nothing
        // in it may end the match early or look like a second URL.
        assert!(!url.contains(' ') && !url.contains('"') && url.matches("://").count() == 1);
    }

    #[test]
    fn a_callback_is_only_this_logins_when_the_state_matches() {
        let state = "expected-state";
        assert_eq!(
            parse_callback("/callback?code=abc&state=expected-state", state),
            Callback::Code("abc".into())
        );
        assert_eq!(
            parse_callback("/callback?state=expected-state&code=a%2Fb%2Bc", state),
            Callback::Code("a/b+c".into()),
            "the code arrives percent-encoded"
        );
        for target in [
            "/callback?code=abc&state=other",
            "/callback?code=abc",
            "/callback?code=abc&state=",
            "/callback?error=access_denied&state=other",
            "/callback",
        ] {
            assert_eq!(
                parse_callback(target, state),
                Callback::WrongState,
                "{target}"
            );
        }
    }

    #[test]
    fn a_callback_reports_refusal_and_cancellation_apart() {
        let state = "s";
        assert_eq!(
            parse_callback("/callback?error=access_denied&state=s", state),
            Callback::Denied
        );
        assert_eq!(
            parse_callback(
                "/callback?error=invalid_scope&error_description=no+such+scope&state=s",
                state
            ),
            Callback::Refused {
                error: "invalid_scope".into(),
                description: Some("no such scope".into()),
            }
        );
        assert!(matches!(
            parse_callback("/callback?state=s", state),
            Callback::Refused { .. }
        ));
    }

    #[test]
    fn other_paths_are_not_callbacks() {
        for target in [
            "/",
            "/favicon.ico",
            "/callback/extra?state=s",
            "/Callback?state=s",
        ] {
            assert_eq!(
                parse_callback(target, "s"),
                Callback::NotCallback,
                "{target}"
            );
        }
    }

    #[test]
    fn the_code_is_exchanged_with_the_verifier_and_no_credentials() {
        let (base_url, server) = one_shot_server(response(
            "200 OK",
            r#"{"access_token":"at-1","token_type":"Bearer","expires_in":300,"scope":"memlake:apikey"}"#,
        ));
        let client = LoginClient::new(&format!("{base_url}/openapi/memorylake")).expect("client");

        let token = client
            .exchange_code("code/1", "http://127.0.0.1:5/callback", &pkce())
            .expect("exchange");
        assert_eq!(token.0, "at-1");

        let request = server.join().expect("server");
        assert!(
            request.head.starts_with("POST /oauth2/mcp/token HTTP/1.1"),
            "{}",
            request.head
        );
        assert!(
            !request.has_header("authorization"),
            "a public client sends no credentials"
        );
        assert_eq!(
            String::from_utf8_lossy(&request.body),
            "grant_type=authorization_code\
             &code=code%2F1\
             &redirect_uri=http%3A%2F%2F127.0.0.1%3A5%2Fcallback\
             &client_id=memorylake-cli\
             &code_verifier=dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        );
    }

    #[test]
    fn a_refused_code_says_to_log_in_again() {
        let (base_url, _server) = one_shot_server(response(
            "400 Bad Request",
            r#"{"error":"invalid_grant","error_description":"code expired"}"#,
        ));
        let client = LoginClient::new(&base_url).expect("client");
        let (message, code) =
            api_error(client.exchange_code("c", "http://127.0.0.1:5/callback", &pkce()));
        assert_eq!(code.as_deref(), Some("invalid_grant"));
        assert!(message.contains("memorylake auth login"), "{message}");
        assert!(
            message.contains("code expired [invalid_grant]"),
            "{message}"
        );
        assert!(
            !message.contains("dBjftJeZ4CVP"),
            "the verifier leaked: {message}"
        );
    }

    #[test]
    fn a_deployment_without_the_endpoints_points_at_key_login() {
        let page = "<html>not found</html>";
        let (base_url, _server) = one_shot_server(format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{page}",
            page.len()
        ));
        let client = LoginClient::new(&base_url).expect("client");
        let (message, _) =
            api_error(client.exchange_code("c", "http://127.0.0.1:5/callback", &pkce()));
        assert!(
            message.contains("does not offer browser login"),
            "{message}"
        );
        assert!(message.contains("--api-key"), "{message}");
        assert!(
            !message.contains("<html>"),
            "a web page is not worth showing: {message}"
        );
    }

    #[test]
    fn the_token_buys_one_named_key() {
        let (base_url, server) = one_shot_server(response(
            "201 Created",
            r#"{"id":"123","name":"memorylake CLI · my-mac","key":"sk-issued","key_prefix":"sk-iss","expires_at":1786953600,"tenant_id":"t-1"}"#,
        ));
        let client = LoginClient::new(&base_url).expect("client");

        let issued = client
            .issue_api_key(
                &AccessToken("at-1".into()),
                "memorylake CLI · my-mac",
                Some("my-mac"),
            )
            .expect("issue");
        assert_eq!(issued.key, "sk-issued");
        assert_eq!(
            issued.issued,
            IssuedKey {
                id: "123".into(),
                name: "memorylake CLI · my-mac".into(),
                expires_at: Some(1_786_953_600),
                tenant_id: Some("t-1".into()),
            }
        );

        let request = server.join().expect("server");
        assert!(
            request
                .head
                .starts_with("POST /oauth2/cli/api-keys HTTP/1.1"),
            "{}",
            request.head
        );
        assert!(
            request
                .head
                .to_ascii_lowercase()
                .contains("authorization: bearer at-1")
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&request.body).expect("json body"),
            serde_json::json!({"name": "memorylake CLI · my-mac", "device": "my-mac"})
        );
    }

    #[test]
    fn a_key_is_read_from_a_numeric_id_or_an_envelope() {
        let issued = read_issued_key(r#"{"id":7,"key":"sk-a"}"#, "asked").expect("bare");
        assert_eq!(issued.issued.id, "7");
        assert_eq!(issued.issued.name, "asked", "the requested name stands in");
        assert_eq!(issued.issued.expires_at, None);

        let issued = read_issued_key(
            r#"{"success":true,"data":{"id":"8","key":"sk-b","expires_at":"1786953600"}}"#,
            "asked",
        )
        .expect("enveloped");
        assert_eq!(issued.key, "sk-b");
        assert_eq!(issued.issued.expires_at, Some(1_786_953_600));

        assert!(
            read_issued_key(r#"{"id":"9"}"#, "asked").is_none(),
            "no key, no login"
        );
        assert!(read_issued_key(r#"{"key":""}"#, "asked").is_none());
    }

    #[test]
    fn a_team_that_forbids_key_creation_is_named_as_the_problem() {
        let (base_url, _server) = one_shot_server(response(
            "403 Forbidden",
            r#"{"error":"access_denied","error_description":"no permission"}"#,
        ));
        let client = LoginClient::new(&base_url).expect("client");
        let (message, code) =
            api_error(client.issue_api_key(&AccessToken("at-secret".into()), "n", None));
        assert_eq!(code.as_deref(), Some("access_denied"));
        assert!(
            message.contains("not allowed to create API keys"),
            "{message}"
        );
        assert!(
            !message.contains("at-secret"),
            "the token leaked: {message}"
        );
    }

    #[test]
    fn a_spent_token_says_to_log_in_again() {
        let (base_url, _server) = one_shot_server(response(
            "401 Unauthorized",
            r#"{"error":"invalid_token","error_description":"token already used"}"#,
        ));
        let client = LoginClient::new(&base_url).expect("client");
        let (message, code) = api_error(client.issue_api_key(&AccessToken("at".into()), "n", None));
        assert_eq!(code.as_deref(), Some("invalid_token"));
        assert!(message.contains("memorylake auth login"), "{message}");
    }

    #[test]
    fn a_success_without_a_key_does_not_echo_the_body() {
        let (base_url, _server) = one_shot_server(response("201 Created", r#"{"kee":"sk-typo"}"#));
        let client = LoginClient::new(&base_url).expect("client");
        let (message, _) = api_error(client.issue_api_key(&AccessToken("at".into()), "n", None));
        assert!(message.contains("without a usable key"), "{message}");
        assert!(!message.contains("sk-typo"), "{message}");
    }

    #[test]
    fn a_key_revokes_itself() {
        let (base_url, server) = one_shot_server(response("200 OK", "{}"));
        let client = LoginClient::new(&base_url).expect("client");
        assert_eq!(
            client.revoke_api_key("sk-issued").expect("revoke"),
            Revocation::Revoked
        );

        let request = server.join().expect("server");
        assert!(
            request
                .head
                .starts_with("POST /oauth2/cli/api-keys/revoke HTTP/1.1"),
            "{}",
            request.head
        );
        assert!(
            request
                .head
                .to_ascii_lowercase()
                .contains("authorization: bearer sk-issued")
        );
        assert!(request.body.is_empty());
    }

    #[test]
    fn a_web_page_is_not_a_revocation() {
        // What a deployment without the endpoint answers when its web app
        // takes every unknown path: success, and a page.
        let page = "<!doctype html><html><title>Loading...</title></html>";
        let (base_url, _server) = one_shot_server(format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{page}",
            page.len()
        ));
        let client = LoginClient::new(&base_url).expect("client");
        let (message, _) = api_error(client.revoke_api_key("sk-still-valid"));
        assert!(message.contains("could not revoke"), "{message}");
        assert!(!message.contains("<html>"), "{message}");
        assert!(!message.contains("sk-still-valid"), "{message}");

        // An empty success is how an endpoint says "done" without a body.
        let (base_url, _server) =
            one_shot_server("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n".to_string());
        let client = LoginClient::new(&base_url).expect("client");
        assert_eq!(
            client.revoke_api_key("sk").expect("revoke"),
            Revocation::Revoked
        );
    }

    #[test]
    fn revocation_tells_a_dead_key_from_one_it_may_not_touch() {
        for (status, expected) in [
            ("401 Unauthorized", Revocation::Revoked),
            ("403 Forbidden", Revocation::NotIssuedByLogin),
        ] {
            let (base_url, _server) = one_shot_server(response(status, r#"{"error":"x"}"#));
            let client = LoginClient::new(&base_url).expect("client");
            assert_eq!(
                client.revoke_api_key("sk").expect(status),
                expected,
                "{status}"
            );
        }

        let (base_url, _server) = one_shot_server(response("500 Internal Server Error", "{}"));
        let client = LoginClient::new(&base_url).expect("client");
        assert!(client.revoke_api_key("sk").is_err());
    }
}
