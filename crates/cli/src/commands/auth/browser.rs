//! The local half of a browser login: the loopback listener the browser is
//! sent back to, the browser itself, and the pages it is shown.
//!
//! The protocol lives in [`memorylake_core::browser_login`].

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use memorylake_core::browser_login::{
    Callback, IssuedApiKey, LoginClient, MAX_DEVICE_CHARS, MAX_KEY_NAME_CHARS, Pkce,
    parse_callback, random_state, redirect_uri,
};

/// How long a login waits for the browser unless told otherwise, in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// The longest a login may be told to wait, in seconds: a day.
///
/// A bound at all, because the wait is added to the clock to get a deadline
/// and a value near the top of the range overflows it.
pub const MAX_TIMEOUT_SECS: u64 = 86_400;

/// What one browser login needs to know.
pub struct BrowserLogin<'a> {
    /// API base URL of the deployment to log in to.
    pub base_url: &'a str,
    /// Name for the key, when the caller chose one.
    pub key_name: Option<&'a str>,
    /// Whether to open a browser, or only print the URL for something else to
    /// open.
    pub open_browser: bool,
    /// How long to wait for the browser to come back.
    pub timeout: Duration,
}

/// Run a browser login to completion and return the key it created.
///
/// Nothing is stored here: the caller decides what the key replaces.
pub fn run(login: &BrowserLogin<'_>) -> Result<IssuedApiKey> {
    let device = device_name();
    let name = key_name(login.key_name, device.as_deref())?;

    let client = LoginClient::new(login.base_url).context("prepare browser login")?;
    // Loopback only, on a port the system picks: the authorization server
    // accepts any port for this address, and nothing off this machine can
    // reach it.
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .context("listen on 127.0.0.1 for the browser to come back to")?;
    let port = listener
        .local_addr()
        .context("read the listening port")?
        .port();
    let redirect = redirect_uri(port);
    let pkce = Pkce::generate()?;
    let state = random_state()?;
    let url = client.authorize_url(&redirect, &pkce, &state)?;

    announce(client.origin(), &url, login.open_browser)?;

    let deadline = Instant::now() + login.timeout;
    wait_for_callback(&listener, deadline, &state, |code| {
        let token = client.exchange_code(code, &redirect, &pkce)?;
        client.issue_api_key(&token, &name, device.as_deref())
    })
    .with_context(|| format!("browser login to {}", client.origin()))
}

/// Show where to log in.
///
/// The URL goes to standard output on a line of its own, flushed before
/// anything waits: a program that launched this command with `--no-browser`
/// reads it from there and opens it itself. Everything said to a person goes
/// to standard error.
fn announce(origin: &str, url: &str, open_browser: bool) -> Result<()> {
    // The host without its scheme: a launcher that scans everything this
    // command prints for a URL must find exactly one, the login URL.
    let host = origin.split_once("://").map_or(origin, |(_, host)| host);
    eprintln!("Log in to MemoryLake ({host}) in your browser:");
    {
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "{url}")
            .and_then(|()| stdout.flush())
            .context("print the login URL")?;
    }
    if open_browser && let Err(err) = open_in_browser(url) {
        tracing::debug!(error = %err, "could not start a browser");
        eprintln!("Could not open a browser from here; open the URL above yourself.");
    }
    eprintln!("Waiting for you to approve the login there…");
    Ok(())
}

/// Hand `url` to the system's default browser without waiting for it.
fn open_in_browser(url: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(windows)]
    let mut command = {
        // Not `cmd /c start`: `cmd` would read the `&` between query
        // parameters as a command separator.
        let mut command = Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

/// Serve the loopback listener until the login's callback arrives, then finish
/// the login with `finish` and tell the browser how it went.
///
/// Requests that are not this login's answer are turned away and the wait goes
/// on. Ending the login on the first odd request would let any web page that
/// can guess a port cancel it; a request without this login's `state` proves
/// nothing either way, so it is simply not the answer.
fn wait_for_callback<F>(
    listener: &TcpListener,
    deadline: Instant,
    state: &str,
    finish: F,
) -> Result<IssuedApiKey>
where
    F: FnOnce(&str) -> memorylake_core::Result<IssuedApiKey>,
{
    listener
        .set_nonblocking(true)
        .context("prepare the listener")?;

    let mut stream;
    let code = loop {
        stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    bail!(
                        "timed out waiting for the login to be approved in the browser; \
                         run `memorylake auth login` again"
                    );
                }
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(err) => return Err(err).context("accept the browser's connection"),
        };
        // An accepted socket inherits non-blocking mode on some systems.
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

        let Some((method, target)) = read_request_line(&mut stream) else {
            // A connection opened ahead of need and never used; browsers do that.
            continue;
        };
        if method != "GET" {
            respond(
                &mut stream,
                "405 Method Not Allowed",
                &Page::NOT_THE_LOGIN,
                None,
            );
            continue;
        }
        match parse_callback(&target, state) {
            Callback::NotCallback => {
                respond(&mut stream, "404 Not Found", &Page::NOT_THE_LOGIN, None);
            }
            Callback::WrongState => {
                respond(&mut stream, "400 Bad Request", &Page::NOT_THE_LOGIN, None);
            }
            Callback::Denied => {
                respond(&mut stream, "200 OK", &Page::CANCELLED, None);
                bail!("the login was cancelled in the browser");
            }
            Callback::Refused { error, description } => {
                let reason = match description {
                    Some(description) => format!("the login was refused: {description} [{error}]"),
                    None => format!("the login was refused [{error}]"),
                };
                respond(&mut stream, "200 OK", &Page::FAILED, Some(&reason));
                bail!(reason);
            }
            Callback::Code(code) => break code,
        }
    };

    // The browser is kept waiting for this, so the page it ends on is true.
    match finish(&code) {
        Ok(issued) => {
            respond(&mut stream, "200 OK", &Page::DONE, None);
            Ok(issued)
        }
        Err(err) => {
            // The whole chain: the cause that matters, such as a certificate
            // the system trusts and this program did not, is often the last.
            let err = anyhow::Error::from(err);
            respond(
                &mut stream,
                "200 OK",
                &Page::FAILED,
                Some(&format!("{err:#}")),
            );
            Err(err)
        }
    }
}

/// Read the request line (`GET /callback?… HTTP/1.1`) off a connection.
///
/// The rest of the head is read and dropped so the browser sees its request
/// taken before the answer comes. `None` when no complete head arrives.
fn read_request_line(stream: &mut TcpStream) -> Option<(String, String)> {
    const MAX_HEAD: usize = 16 * 1024;

    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        if head.len() > MAX_HEAD {
            return None;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(read) => head.extend_from_slice(&chunk[..read]),
        }
    }

    let head = String::from_utf8_lossy(&head);
    let mut parts = head.lines().next()?.split_ascii_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    Some((method, target))
}

/// A page shown in the browser.
///
/// Fixed text, except for why a login failed: the program that started the
/// login may have no terminal anyone can see (an app runs it in the
/// background), so the browser is the one place the reason is sure to reach.
/// That reason is escaped, and nothing from the request itself is ever echoed.
struct Page {
    title: &'static str,
    english: &'static str,
    chinese: &'static str,
}

impl Page {
    const DONE: Self = Self {
        title: "Login complete",
        english: "Login complete. You can close this page.",
        chinese: "登录完成，可以关闭此页面。",
    };
    const CANCELLED: Self = Self {
        title: "Login cancelled",
        english: "Login cancelled. You can close this page.",
        chinese: "已取消登录，可以关闭此页面。",
    };
    const FAILED: Self = Self {
        title: "Login failed",
        english: "Login failed.",
        chinese: "登录失败。",
    };
    const NOT_THE_LOGIN: Self = Self {
        title: "Not a login page",
        english: "This address only finishes a MemoryLake login that is waiting for it. \
                  Start again from the link the login printed.",
        chinese: "此地址仅用于完成一次正在等待的 MemoryLake 登录，请从登录命令打印的链接重新开始。",
    };

    fn html(&self, detail: Option<&str>) -> String {
        let detail = detail.map_or_else(String::new, |detail| {
            format!(
                "<pre>{}</pre>",
                escape_html(&truncate_chars(detail.trim(), MAX_DETAIL_CHARS))
            )
        });
        format!(
            "<!doctype html><html><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
             <title>{title} · MemoryLake</title>\
             <style>body{{font:16px/1.6 system-ui,sans-serif;margin:18vh auto;max-width:32em;\
             padding:0 1.5em;color:#1f2328;background:#fff}}h1{{font-size:1.3em}}\
             pre{{white-space:pre-wrap;word-break:break-word;font-size:.85em;padding:.8em 1em;\
             border-radius:6px;background:rgba(127,127,127,.12)}}\
             @media(prefers-color-scheme:dark){{body{{color:#e6edf3;background:#0d1117}}}}</style>\
             </head><body><h1>MemoryLake</h1><p>{english}</p><p lang=\"zh\">{chinese}</p>{detail}\
             </body></html>",
            title = self.title,
            english = self.english,
            chinese = self.chinese,
        )
    }
}

/// Answer the browser and close the connection. Best effort: by now the login
/// has succeeded or failed on its own account, and a tab that was closed early
/// must not change that.
fn respond(stream: &mut TcpStream, status: &str, page: &Page, detail: Option<&str>) {
    let body = page.html(detail);
    let head = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Referrer-Policy: no-referrer\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body.as_bytes()))
        .and_then(|()| stream.flush());
    let _ = stream.shutdown(std::net::Shutdown::Write);
}

/// Longest failure reason shown in the browser, in characters.
const MAX_DETAIL_CHARS: usize = 1000;

fn truncate_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// The name the new key is created under.
///
/// It is what the user later sees in the console's key list, so by default it
/// says what made the key and on which machine.
fn key_name(explicit: Option<&str>, device: Option<&str>) -> Result<String> {
    if let Some(name) = explicit {
        let name = name.trim();
        if name.is_empty() {
            bail!("--key-name must not be empty");
        }
        if name.chars().count() > MAX_KEY_NAME_CHARS {
            bail!("--key-name is limited to {MAX_KEY_NAME_CHARS} characters");
        }
        return Ok(name.to_string());
    }
    Ok(match device {
        Some(device) => format!("memorylake CLI · {device}"),
        None => "memorylake CLI".to_string(),
    })
}

/// This machine's name, for the key's name and its `device` label.
fn device_name() -> Option<String> {
    tidy_device_name(&hostname()?)
}

/// Trim a host name to something worth showing in a list of keys.
///
/// The `.local` that macOS appends says nothing about which machine it is.
fn tidy_device_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    let name = name.strip_suffix(".local").unwrap_or(name);
    let name: String = name.chars().take(MAX_DEVICE_CHARS).collect();
    (!name.is_empty()).then_some(name)
}

#[cfg(unix)]
fn hostname() -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: `gethostname` writes at most `len` bytes into a buffer of that
    // length. One byte is held back so the result is NUL-terminated even when
    // the name was truncated, which POSIX leaves unspecified.
    let status = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len() - 1) };
    if status != 0 {
        return None;
    }
    let end = buffer.iter().position(|&byte| byte == 0)?;
    Some(String::from_utf8_lossy(&buffer[..end]).into_owned())
}

#[cfg(not(unix))]
fn hostname() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use memorylake_core::IssuedKey;
    use std::sync::mpsc::channel;

    fn issued() -> IssuedApiKey {
        IssuedApiKey {
            key: "sk-issued".into(),
            issued: IssuedKey {
                id: "1".into(),
                name: "n".into(),
                expires_at: None,
                tenant_id: None,
            },
        }
    }

    /// Send one request to `port` and return the whole response.
    fn browser_get(port: u16, target: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to listener");
        write!(
            stream,
            "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
        )
        .expect("send");
        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
        response
    }

    /// How a wait ended, and the code it was handed, if any.
    type Outcome = (Result<IssuedApiKey>, Option<String>);

    /// Wait for a callback on a fresh listener in a background thread.
    fn waiting(
        timeout: Duration,
        outcome: memorylake_core::Result<IssuedApiKey>,
    ) -> (u16, std::sync::mpsc::Receiver<Outcome>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let mut seen = None;
            let result =
                wait_for_callback(&listener, Instant::now() + timeout, "the-state", |code| {
                    seen = Some(code.to_string());
                    outcome
                });
            let _ = sender.send((result, seen));
        });
        (port, receiver)
    }

    #[test]
    fn the_callback_code_finishes_the_login_and_the_browser_is_told() {
        let (port, done) = waiting(Duration::from_secs(10), Ok(issued()));
        let response = browser_get(port, "/callback?code=the-code&state=the-state");

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.contains("Login complete"), "{response}");
        assert!(
            !response.contains("the-code"),
            "nothing from the request is echoed"
        );
        let (result, seen) = done.recv().expect("login thread");
        assert_eq!(result.expect("login").key, "sk-issued");
        assert_eq!(seen.as_deref(), Some("the-code"));
    }

    #[test]
    fn requests_that_are_not_the_answer_are_turned_away_and_the_wait_goes_on() {
        let (port, done) = waiting(Duration::from_secs(10), Ok(issued()));

        assert!(browser_get(port, "/favicon.ico").starts_with("HTTP/1.1 404"));
        let forged = browser_get(port, "/callback?code=attacker-code&state=guess");
        assert!(forged.starts_with("HTTP/1.1 400"), "{forged}");
        let cancel = browser_get(port, "/callback?error=access_denied&state=guess");
        assert!(
            cancel.starts_with("HTTP/1.1 400"),
            "a stranger cannot cancel: {cancel}"
        );
        assert!(
            done.try_recv().is_err(),
            "none of those may have ended the login"
        );

        browser_get(port, "/callback?code=real&state=the-state");
        let (result, seen) = done.recv().expect("login thread");
        assert!(result.is_ok());
        assert_eq!(
            seen.as_deref(),
            Some("real"),
            "the forged code was never used"
        );
    }

    #[test]
    fn declining_in_the_browser_ends_the_login_without_spending_anything() {
        let (port, done) = waiting(Duration::from_secs(10), Ok(issued()));
        let response = browser_get(port, "/callback?error=access_denied&state=the-state");

        assert!(response.contains("Login cancelled"), "{response}");
        let (result, seen) = done.recv().expect("login thread");
        let err = result.expect_err("cancelled");
        assert!(err.to_string().contains("cancelled"), "{err}");
        assert_eq!(seen, None);
    }

    #[test]
    fn a_login_that_fails_after_the_callback_does_not_tell_the_browser_it_worked() {
        let failure = memorylake_core::Error::Api {
            message: "you are not allowed to create API keys".into(),
            code: Some("access_denied".into()),
        };
        let (port, done) = waiting(Duration::from_secs(10), Err(failure));
        let response = browser_get(port, "/callback?code=c&state=the-state");

        assert!(response.contains("Login failed"), "{response}");
        assert!(!response.contains("Login complete"), "{response}");
        assert!(
            response.contains("you are not allowed to create API keys"),
            "the browser is told why: {response}"
        );
        let (result, _) = done.recv().expect("login thread");
        assert!(
            result
                .expect_err("failed")
                .to_string()
                .contains("not allowed")
        );
    }

    #[test]
    fn a_refusal_shows_its_reason_escaped() {
        let (port, done) = waiting(Duration::from_secs(10), Ok(issued()));
        let response = browser_get(
            port,
            "/callback?error=server_error&error_description=%3Cscript%3Ex%3C%2Fscript%3E&state=the-state",
        );

        assert!(response.contains("Login failed"), "{response}");
        assert!(
            response.contains("&lt;script&gt;x&lt;/script&gt; [server_error]"),
            "{response}"
        );
        assert!(!response.contains("<script>"), "{response}");
        let (result, _) = done.recv().expect("login thread");
        assert!(
            result
                .expect_err("refused")
                .to_string()
                .contains("server_error")
        );
    }

    #[test]
    fn a_long_reason_is_cut_short() {
        let long = "x".repeat(MAX_DETAIL_CHARS + 50);
        let cut = truncate_chars(&long, MAX_DETAIL_CHARS);
        assert_eq!(cut.chars().count(), MAX_DETAIL_CHARS + 1);
        assert!(cut.ends_with('…'));
        assert_eq!(truncate_chars("名字", 5), "名字");
    }

    #[test]
    fn nobody_coming_back_is_a_timeout() {
        let (_port, done) = waiting(Duration::from_millis(200), Ok(issued()));
        let (result, seen) = done.recv().expect("login thread");
        let err = result.expect_err("timeout");
        assert!(err.to_string().contains("timed out"), "{err}");
        assert_eq!(seen, None);
    }

    #[test]
    fn the_key_is_named_after_the_machine_unless_the_caller_names_it() {
        assert_eq!(
            key_name(None, Some("my-mac")).expect("default"),
            "memorylake CLI · my-mac"
        );
        assert_eq!(key_name(None, None).expect("default"), "memorylake CLI");
        assert_eq!(
            key_name(Some("  WorkBuddy on my-mac "), Some("my-mac")).expect("explicit"),
            "WorkBuddy on my-mac"
        );
        assert!(key_name(Some("  "), None).is_err());
        assert!(
            key_name(Some(&"名".repeat(100)), None).is_ok(),
            "the limit counts characters"
        );
        assert!(key_name(Some(&"名".repeat(101)), None).is_err());
    }

    #[test]
    fn the_device_label_is_short_and_says_which_machine() {
        assert_eq!(
            tidy_device_name(" my-mac.local\n").as_deref(),
            Some("my-mac")
        );
        assert_eq!(
            tidy_device_name("build-7.example.com").as_deref(),
            Some("build-7.example.com")
        );
        assert_eq!(
            tidy_device_name(&"名".repeat(80)).map(|name| name.chars().count()),
            Some(MAX_DEVICE_CHARS)
        );
        assert_eq!(tidy_device_name("  "), None);
        assert_eq!(tidy_device_name(".local"), None);
        if let Some(device) = device_name() {
            assert!(!device.is_empty() && device.chars().count() <= MAX_DEVICE_CHARS);
        }
    }
}
