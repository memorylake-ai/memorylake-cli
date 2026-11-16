//! Interactive prompts for `auth login`.

use anyhow::Result;
use memorylake_core::{CN_BASE_URL, DEFAULT_BASE_URL, normalize_base_url};

use crate::interactive::{prompt_line, prompt_secret, select_index};

/// Login methods offered by interactive `auth login`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveLoginMethod {
    /// Approve the login in a browser; MemoryLake creates the API key.
    Browser,
    /// Store and validate a MemoryLake API key the user already has.
    ApiKey,
}

impl InteractiveLoginMethod {
    /// Methods a user can actually complete, in the order offered.
    ///
    /// The browser comes first because it asks least of someone new: there is
    /// no key to create in the console and copy over. A method belongs here
    /// only once its flow works end to end: a picker whose first choice is a
    /// dead end is worse than no picker.
    const AVAILABLE: &'static [Self] = &[Self::Browser, Self::ApiKey];

    fn label(self) -> &'static str {
        match self {
            Self::Browser => "Browser (creates an API key for this machine)",
            Self::ApiKey => "API key (paste one you already have)",
        }
    }
}

/// Ask the user which login method to use for `base_url`.
///
/// With only one method available this returns it without prompting, for the
/// same reason `workspace use` skips a one-item menu: there is nothing to
/// decide, and the next prompt already makes clear what is being asked for.
pub fn prompt_login_method(base_url: &str) -> Result<InteractiveLoginMethod> {
    let available = InteractiveLoginMethod::AVAILABLE;
    if let [only] = available {
        return Ok(*only);
    }

    let labels: Vec<&str> = available.iter().map(|m| m.label()).collect();
    let prompt = format!("Select login method to {base_url}");
    let idx = select_index(prompt, &labels, 0)?;
    Ok(available[idx])
}

/// Prompt for an API key (input is hidden).
pub fn prompt_api_key() -> Result<String> {
    prompt_secret("API key")
}

/// Ask which MemoryLake deployment to log in to.
///
/// Only asked when the caller expressed no preference at all — no
/// `--base-url`, nothing on the profile, no `MEMORYLAKE_BASE_URL` — because the
/// two deployments are separate installations rather than mirrors: an account
/// on one does not exist on the other, so logging in to the wrong one fails in
/// a way that reads like a rejected API key rather than a wrong endpoint.
pub fn prompt_base_url() -> Result<String> {
    let labels = [
        format!("Global — {DEFAULT_BASE_URL}"),
        format!("China — {CN_BASE_URL}"),
        "Other (enter a URL)".to_string(),
    ];
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let idx = select_index("Select the MemoryLake endpoint", &refs, 0)?;

    match idx {
        0 => Ok(DEFAULT_BASE_URL.to_string()),
        1 => Ok(CN_BASE_URL.to_string()),
        _ => {
            let url = prompt_line("Base URL")?;
            Ok(normalize_base_url(&url)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_is_offered_first_and_a_key_remains_possible() {
        // The first entry is the default the picker lands on. A key login has
        // to stay: it is the only way in for a deployment without browser
        // login, and for anyone who already holds a key.
        assert_eq!(
            InteractiveLoginMethod::AVAILABLE,
            &[
                InteractiveLoginMethod::Browser,
                InteractiveLoginMethod::ApiKey
            ],
        );
    }

    #[test]
    fn choosing_a_method_needs_a_terminal() {
        // Tests run without one. With two methods there is a real question to
        // ask, and nobody to ask must be an error, not a silent default: the
        // flags `--browser` and `--api-key` are how a script answers it.
        let err = prompt_login_method("https://app.memorylake.ai/openapi/memorylake")
            .expect_err("no terminal to ask on");
        assert!(err.to_string().contains("TTY"), "{err}");
    }
}
