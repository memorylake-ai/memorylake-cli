//! Interactive prompts for `auth login`.

use anyhow::{Result, bail};
use memorylake_core::{CN_BASE_URL, DEFAULT_BASE_URL};

use crate::interactive::{prompt_line, prompt_secret, select_index};

/// Login methods offered by interactive `auth login`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveLoginMethod {
    /// Store and validate a MemoryLake API key.
    ApiKey,
    /// Browser / OAuth device login (not implemented yet).
    OAuth,
}

impl InteractiveLoginMethod {
    const ALL: [Self; 2] = [Self::ApiKey, Self::OAuth];

    fn label(self) -> &'static str {
        match self {
            Self::ApiKey => "API key",
            Self::OAuth => "OAuth",
        }
    }
}

/// Ask the user which login method to use for `base_url`.
pub fn prompt_login_method(base_url: &str) -> Result<InteractiveLoginMethod> {
    let labels: Vec<&str> = InteractiveLoginMethod::ALL
        .iter()
        .map(|m| m.label())
        .collect();
    let prompt = format!("Select login method to {base_url}");
    let idx = select_index(prompt, &labels, 0)?;
    Ok(InteractiveLoginMethod::ALL[idx])
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
            if url.is_empty() {
                bail!("base URL must not be empty");
            }
            // Catch a bare host early: the resolved URL is joined with
            // `/api/v3/...` paths, and a scheme-less value fails later with a
            // parse error that does not mention what was typed.
            if !url.starts_with("http://") && !url.starts_with("https://") {
                bail!("base URL must start with http:// or https://, found `{url}`");
            }
            Ok(url)
        }
    }
}
