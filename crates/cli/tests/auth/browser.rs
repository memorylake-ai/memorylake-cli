//! Browser login, end to end, against a loopback stand-in for a MemoryLake
//! deployment.
//!
//! The test plays two parts the real flow has: the deployment's login
//! endpoints (a scripted server) and the browser (it reads the login URL the
//! command prints and requests the callback address itself). No browser is
//! ever opened: every login here runs with `--no-browser`.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use crate::common::stub::{
    logged_in_home, read_http_request, request_body, request_header, request_line,
};
use crate::common::{assert_failure, assert_success, isolated, run, temp_home};

const TOKEN_OK: (&str, &str) = (
    "200 OK",
    r#"{"access_token":"at-secret","token_type":"Bearer","expires_in":300,"scope":"memlake:apikey"}"#,
);
const KEY_CREATED: (&str, &str) = (
    "201 Created",
    r#"{"id":"123","name":"test key","key":"sk-issued-key","key_prefix":"sk-iss","expires_at":4102444800,"tenant_id":"team-1"}"#,
);
const REVOKED: (&str, &str) = ("200 OK", "{}");

/// A loopback server answering a fixed script of responses, one per request,
/// and handing each request back to the test.
struct Deployment {
    /// API base URL, the way a profile stores it.
    base_url: String,
    /// Scheme, host and port: where the login endpoints are expected.
    origin: String,
    requests: Receiver<String>,
}

impl Deployment {
    fn answering(script: &[(&str, &str)]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind deployment");
        let origin = format!("http://{}", listener.local_addr().expect("address"));
        let responses: Vec<String> = script
            .iter()
            .map(|(status, body)| {
                format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
            })
            .collect();

        let (sender, requests) = channel();
        std::thread::spawn(move || {
            for response in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let _ = sender.send(read_http_request(&mut stream));
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
            // The script is over: the listener closes, so a request the test
            // did not expect fails loudly instead of hanging.
        });

        Self {
            base_url: format!("{origin}/openapi/memorylake"),
            origin,
            requests,
        }
    }

    /// The next request the deployment received.
    fn received(&self) -> String {
        self.requests
            .recv_timeout(Duration::from_secs(10))
            .expect("the deployment received no further request")
    }

    /// Assert that nothing (more) reached the deployment.
    fn received_nothing_more(&self) {
        if let Ok(request) = self.requests.recv_timeout(Duration::from_millis(200)) {
            panic!("unexpected request to the deployment:\n{request}");
        }
    }
}

/// A `memorylake auth login --no-browser` that has printed its URL and is
/// waiting for the browser.
struct Login {
    child: Child,
    stdout: BufReader<ChildStdout>,
    url: String,
}

/// What a finished login left behind.
struct Finished {
    success: bool,
    stdout: String,
    stderr: String,
}

impl Finished {
    fn assert_no_secret(&self) {
        for secret in ["at-secret", "sk-issued-key", "the-code"] {
            assert!(
                !self.stdout.contains(secret) && !self.stderr.contains(secret),
                "`{secret}` was printed:\nstdout: {}\nstderr: {}",
                self.stdout,
                self.stderr
            );
        }
    }
}

impl Login {
    fn start(home: &Path, args: &[&str]) -> Self {
        let mut all = vec!["auth", "login", "--no-browser"];
        all.extend_from_slice(args);
        let mut child = isolated(home, &all)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn memorylake auth login");

        let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        let mut url = String::new();
        stdout.read_line(&mut url).expect("read the login URL");
        if url.trim().is_empty() {
            let output = child.wait_with_output().expect("wait for failed login");
            panic!(
                "login printed no URL: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Self {
            child,
            stdout,
            url: url.trim().to_string(),
        }
    }

    /// A query parameter of the login URL, decoded.
    fn param(&self, name: &str) -> String {
        let query = self.url.split_once('?').expect("login URL has a query").1;
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| percent_decode(value))
            .unwrap_or_else(|| panic!("login URL has no `{name}`: {}", self.url))
    }

    /// Request the callback address the way the browser would after the
    /// authorization server redirected it, and return the response.
    fn browser_returns(&self, query: &str) -> String {
        let redirect = self.param("redirect_uri");
        let address = redirect
            .strip_prefix("http://")
            .and_then(|rest| rest.strip_suffix("/callback"))
            .unwrap_or_else(|| panic!("unexpected redirect address {redirect}"));
        let mut stream = TcpStream::connect(address).expect("connect to the callback address");
        write!(
            stream,
            "GET /callback?{query} HTTP/1.1\r\nHost: {address}\r\n\r\n"
        )
        .expect("send callback");
        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
        response
    }

    /// The callback a successful authorization produces.
    fn browser_approves(&self) -> String {
        self.browser_returns(&format!("code=the-code&state={}", self.param("state")))
    }

    fn finish(mut self) -> Finished {
        let mut stdout = String::new();
        self.stdout
            .read_to_string(&mut stdout)
            .expect("read the rest of stdout");
        let output = self.child.wait_with_output().expect("wait for login");
        Finished {
            success: output.status.success(),
            stdout,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = (bytes[index] == b'%')
            .then(|| value.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match hex {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(if bytes[index] == b'+' {
                    b' '
                } else {
                    bytes[index]
                });
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).expect("decoded parameter is UTF-8")
}

fn credentials_path(home: &Path) -> PathBuf {
    home.join(".memorylake").join("credentials.toml")
}

fn read_state_file(home: &Path, name: &str) -> String {
    fs::read_to_string(home.join(".memorylake").join(name))
        .unwrap_or_else(|err| panic!("read {name}: {err}"))
}

/// A temp `$HOME` holding a key that an earlier browser login created.
fn home_with_issued_key(base_url: &str, expires_at: i64) -> PathBuf {
    let home = temp_home();
    let root = home.join(".memorylake");
    fs::create_dir_all(&root).expect("create .memorylake");
    fs::write(
        root.join("config.toml"),
        format!("active_profile = \"default\"\n\n[profiles.default]\nbase_url = \"{base_url}\"\n"),
    )
    .expect("write config.toml");
    fs::write(
        root.join("credentials.toml"),
        format!(
            "[profiles.default]\napi_key = \"sk-earlier-key\"\nlogin_method = \"api_key\"\n\n\
             [profiles.default.issued]\nid = \"77\"\nname = \"memorylake CLI · old-mac\"\n\
             expires_at = {expires_at}\ntenant_id = \"team-1\"\n"
        ),
    )
    .expect("write credentials.toml");
    home
}

#[test]
fn a_browser_login_stores_the_key_the_deployment_issues() {
    let deployment = Deployment::answering(&[TOKEN_OK, KEY_CREATED]);
    let home = temp_home();
    let login = Login::start(
        &home,
        &["--base-url", &deployment.base_url, "--key-name", "test key"],
    );

    // The URL a launcher opens: the fixed client, one scope, PKCE, a loopback
    // callback address.
    assert!(
        login
            .url
            .starts_with(&format!("{}/oauth2/mcp/authorize?", deployment.origin)),
        "{}",
        login.url
    );
    assert_eq!(login.param("response_type"), "code");
    assert_eq!(login.param("client_id"), "memorylake-cli");
    assert_eq!(login.param("scope"), "memlake:apikey");
    assert_eq!(login.param("code_challenge_method"), "S256");
    assert_eq!(login.param("code_challenge").len(), 43);
    let redirect = login.param("redirect_uri");
    assert!(
        redirect.starts_with("http://127.0.0.1:") && redirect.ends_with("/callback"),
        "{redirect}"
    );
    assert_eq!(
        login.url.matches("://").count(),
        1,
        "the callback address must be encoded, or a launcher scanning for a URL finds two: {}",
        login.url
    );

    let url = login.url.clone();
    let page = login.browser_approves();
    assert!(page.contains("Login complete"), "{page}");
    let finished = login.finish();
    assert!(finished.success, "{}", finished.stderr);
    assert!(
        finished
            .stdout
            .contains("Logged in to profile `default` using browser"),
        "{}",
        finished.stdout
    );
    assert!(
        finished
            .stderr
            .contains("Created API key `test key`, valid until 2100-01-01."),
        "{}",
        finished.stderr
    );
    finished.assert_no_secret();
    assert!(
        !finished.stderr.contains("://"),
        "the login URL must be the only URL printed, or a launcher may open the wrong one: {}",
        finished.stderr
    );

    let exchange = deployment.received();
    assert_eq!(request_line(&exchange), "POST /oauth2/mcp/token HTTP/1.1");
    assert_eq!(request_header(&exchange, "authorization"), None);
    let form = request_body(&exchange);
    for field in [
        "grant_type=authorization_code",
        "code=the-code",
        "client_id=memorylake-cli",
        "redirect_uri=http%3A%2F%2F127.0.0.1%3A",
    ] {
        assert!(form.contains(field), "`{field}` missing from {form}");
    }
    let verifier = form
        .split('&')
        .find_map(|pair| pair.strip_prefix("code_verifier="))
        .expect("the verifier goes to the token endpoint");
    assert!(verifier.len() >= 43, "{verifier}");
    assert!(
        !url.contains(verifier),
        "and only there: the browser is shown the challenge, never the verifier"
    );

    let create = deployment.received();
    assert_eq!(request_line(&create), "POST /oauth2/cli/api-keys HTTP/1.1");
    assert_eq!(
        request_header(&create, "authorization"),
        Some("Bearer at-secret")
    );
    let body: serde_json::Value = serde_json::from_str(request_body(&create)).expect("JSON body");
    assert_eq!(body["name"], "test key");
    deployment.received_nothing_more();

    let credentials = read_state_file(&home, "credentials.toml");
    for line in [
        "api_key = \"sk-issued-key\"",
        // Not `oauth`: an older binary sharing this file must still accept it.
        "login_method = \"api_key\"",
        "[profiles.default.issued]",
        "id = \"123\"",
        "name = \"test key\"",
        "expires_at = 4102444800",
        "tenant_id = \"team-1\"",
    ] {
        assert!(
            credentials.contains(line),
            "`{line}` missing from:\n{credentials}"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(credentials_path(&home))
            .expect("credentials metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "the key is for its owner only");
    }
    let config = read_state_file(&home, "config.toml");
    assert!(config.contains("active_profile = \"default\""), "{config}");
    assert!(
        config.contains(&format!("base_url = \"{}\"", deployment.base_url)),
        "{config}"
    );
    let leftovers: Vec<_> = fs::read_dir(home.join(".memorylake"))
        .expect("list state directory")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "scratch files left behind: {leftovers:?}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_callback_without_the_logins_state_does_not_log_anyone_in() {
    let deployment = Deployment::answering(&[TOKEN_OK, KEY_CREATED]);
    let home = temp_home();
    let login = Login::start(&home, &["--base-url", &deployment.base_url]);

    // Another page can make the browser request this address with a code of
    // its choosing, hoping to log the user in to an account it controls.
    let forged = login.browser_returns("code=someone-elses-code&state=guessed");
    assert!(forged.starts_with("HTTP/1.1 400"), "{forged}");
    deployment.received_nothing_more();

    // The real answer still gets through afterwards.
    login.browser_approves();
    let finished = login.finish();
    assert!(finished.success, "{}", finished.stderr);
    assert!(request_body(&deployment.received()).contains("code=the-code"));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn declining_in_the_browser_stores_nothing_and_asks_the_deployment_nothing() {
    let deployment = Deployment::answering(&[]);
    let home = temp_home();
    let login = Login::start(&home, &["--base-url", &deployment.base_url]);

    let page = login.browser_returns(&format!(
        "error=access_denied&state={}",
        login.param("state")
    ));
    assert!(page.contains("Login cancelled"), "{page}");
    let finished = login.finish();
    assert!(!finished.success);
    assert!(finished.stderr.contains("cancelled"), "{}", finished.stderr);
    assert!(!credentials_path(&home).exists());
    deployment.received_nothing_more();
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_team_that_forbids_key_creation_fails_the_login_and_says_why() {
    let deployment = Deployment::answering(&[
        TOKEN_OK,
        (
            "403 Forbidden",
            r#"{"error":"access_denied","error_description":"no permission to create API keys"}"#,
        ),
    ]);
    let home = temp_home();
    let login = Login::start(&home, &["--base-url", &deployment.base_url]);

    let page = login.browser_approves();
    assert!(page.contains("Login failed"), "{page}");
    assert!(!page.contains("Login complete"), "{page}");
    let finished = login.finish();
    assert!(!finished.success);
    assert!(
        finished
            .stderr
            .contains("not allowed to create API keys in the team you picked"),
        "{}",
        finished.stderr
    );
    finished.assert_no_secret();
    assert!(
        !credentials_path(&home).exists(),
        "a failed login stores nothing"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_login_nobody_approves_gives_up() {
    let deployment = Deployment::answering(&[]);
    let home = temp_home();
    let login = Login::start(
        &home,
        &["--base-url", &deployment.base_url, "--timeout", "1"],
    );
    let finished = login.finish();
    assert!(!finished.success);
    assert!(finished.stderr.contains("timed out"), "{}", finished.stderr);
    assert!(!credentials_path(&home).exists());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_browser_login_without_a_terminal_never_stops_to_ask() {
    // A program that launches the login has no terminal to answer on. With no
    // endpoint chosen anywhere the login must still get as far as printing its
    // URL, for the default deployment, rather than fail on a prompt.
    let home = temp_home();
    let login = Login::start(&home, &["--timeout", "1"]);
    assert!(
        login
            .url
            .starts_with("https://app.memorylake.ai/oauth2/mcp/authorize?"),
        "{}",
        login.url
    );
    let finished = login.finish();
    assert!(finished.stderr.contains("timed out"), "{}", finished.stderr);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn logging_in_again_revokes_the_key_the_new_login_replaces() {
    let deployment = Deployment::answering(&[TOKEN_OK, KEY_CREATED, REVOKED]);
    let home = home_with_issued_key(&deployment.base_url, 4_102_444_800);
    // No `--base-url`: the profile already names the deployment.
    let login = Login::start(&home, &[]);
    login.browser_approves();
    let finished = login.finish();
    assert!(finished.success, "{}", finished.stderr);

    deployment.received();
    deployment.received();
    let revoke = deployment.received();
    assert_eq!(
        request_line(&revoke),
        "POST /oauth2/cli/api-keys/revoke HTTP/1.1"
    );
    assert_eq!(
        request_header(&revoke, "authorization"),
        Some("Bearer sk-earlier-key"),
        "the old key revokes itself"
    );
    assert!(
        finished
            .stderr
            .contains("Revoked the API key this login replaces."),
        "{}",
        finished.stderr
    );
    assert!(read_state_file(&home, "credentials.toml").contains("sk-issued-key"));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_failed_login_leaves_the_earlier_one_working() {
    let deployment = Deployment::answering(&[(
        "400 Bad Request",
        r#"{"error":"invalid_grant","error_description":"code expired"}"#,
    )]);
    let home = home_with_issued_key(&deployment.base_url, 4_102_444_800);
    let login = Login::start(&home, &[]);
    login.browser_approves();
    let finished = login.finish();

    assert!(!finished.success);
    assert!(
        finished.stderr.contains("memorylake auth login"),
        "{}",
        finished.stderr
    );
    deployment.received();
    deployment.received_nothing_more();
    assert!(
        read_state_file(&home, "credentials.toml").contains("sk-earlier-key"),
        "the earlier key is neither replaced nor revoked"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn logout_revokes_a_key_that_a_browser_login_created() {
    let deployment = Deployment::answering(&[REVOKED]);
    let home = home_with_issued_key(&deployment.base_url, 4_102_444_800);
    let args = ["auth", "logout"];
    let output = run(&home, &args);
    let stdout = assert_success(&output, &args);

    let revoke = deployment.received();
    assert_eq!(
        request_line(&revoke),
        "POST /oauth2/cli/api-keys/revoke HTTP/1.1"
    );
    assert_eq!(
        request_header(&revoke, "authorization"),
        Some("Bearer sk-earlier-key")
    );
    assert!(stdout.contains("Logged out profile `default`"), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Revoked API key `memorylake CLI · old-mac`."),
        "{stderr}"
    );
    assert!(!read_state_file(&home, "credentials.toml").contains("sk-earlier-key"));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn logout_still_logs_out_when_the_key_cannot_be_revoked() {
    // Offline, or the deployment is down: the local credentials must go
    // anyway, and the user must learn that the key outlived them.
    let home = home_with_issued_key("http://127.0.0.1:1/openapi/memorylake", 4_102_444_800);
    let args = ["auth", "logout"];
    let output = run(&home, &args);
    assert_success(&output, &args);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Could not revoke API key"), "{stderr}");
    assert!(stderr.contains("still valid"), "{stderr}");
    assert!(!stderr.contains("sk-earlier-key"), "{stderr}");
    assert!(!read_state_file(&home, "credentials.toml").contains("sk-earlier-key"));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn logout_of_a_pasted_key_asks_the_deployment_nothing() {
    // A key the user pasted in is theirs: it may be in use elsewhere, and it
    // is never sent to an endpoint it was not created by. The unreachable
    // address proves no request is attempted: the command would say so.
    let home = logged_in_home("http://127.0.0.1:1/openapi/memorylake");
    let args = ["auth", "logout"];
    let output = run(&home, &args);
    assert_success(&output, &args);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.trim().is_empty(), "{stderr}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn status_describes_a_key_that_a_browser_login_created() {
    let deployment =
        Deployment::answering(&[("200 OK", r#"{"success":true,"data":{"items":[]}}"#)]);
    let home = home_with_issued_key(&deployment.base_url, 4_102_444_800);
    let args = ["auth", "status"];
    let stdout = assert_success(&run(&home, &args), &args);

    for line in [
        "Login method: browser",
        "Key name: memorylake CLI · old-mac",
        "Team: team-1",
        "Key expires: 2100-01-01 (in ",
        // Installers read this exact line; it must survive the additions.
        "Logged in: yes",
        "Credentials: valid",
    ] {
        assert!(stdout.contains(line), "`{line}` missing from:\n{stdout}");
    }
    assert!(
        !stdout.contains("Hint:"),
        "nothing to warn about yet:\n{stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn status_says_when_the_key_has_run_out() {
    let home = home_with_issued_key("http://127.0.0.1:1/openapi/memorylake", 86_400);
    let output = run(&home, &["auth", "status"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Key expires: 1970-01-02 (expired)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Hint: this API key has expired; run `memorylake auth login`"),
        "{stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn status_of_a_pasted_key_reads_as_it_always_did() {
    let home = logged_in_home("http://127.0.0.1:1/openapi/memorylake");
    let output = run(&home, &["auth", "status"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Login method: api_key"), "{stdout}");
    for absent in ["Key name:", "Team:", "Key expires:", "Hint:"] {
        assert!(!stdout.contains(absent), "`{absent}` in:\n{stdout}");
    }
    let _ = fs::remove_dir_all(&home);
}

const KEY_REJECTED: (&str, &str) = (
    "401 Unauthorized",
    r#"{"success":false,"message":"service account not found","error_code":"UNAUTHORIZE"}"#,
);

#[test]
fn a_key_the_api_stops_accepting_says_to_log_in_again() {
    // What an expired or revoked key looks like from any command.
    let deployment = Deployment::answering(&[KEY_REJECTED]);
    let home = logged_in_home(&deployment.base_url);
    let args = ["workspace", "list"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("may have expired or been revoked"), "{err}");
    assert!(err.contains("memorylake auth login"), "{err}");
    assert!(
        err.contains("service account not found"),
        "the API's words stay: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_key_refused_at_login_is_not_called_expired() {
    // Here the key was never accepted: it is the one being offered.
    let deployment = Deployment::answering(&[KEY_REJECTED]);
    let home = temp_home();
    let args = [
        "auth",
        "login",
        "--api-key",
        "sk_wrong",
        "--base-url",
        &deployment.base_url,
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("could not verify API key"), "{err}");
    assert!(!err.contains("may have expired"), "{err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_key_and_a_browser_login_cannot_be_asked_for_together() {
    let home = temp_home();
    for flag in ["--browser", "--no-browser"] {
        let args = ["auth", "login", "--api-key", "sk_x", flag];
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains("cannot be used with"), "{flag}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}
