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
    ///
    /// Kept out of [`Self::AVAILABLE`] until the flow exists, so nothing
    /// constructs it today. Retained rather than deleted because `run_login`
    /// still handles it, which is what will carry the real flow.
    #[allow(dead_code)] // planned login method; not offered until it works
    OAuth,
}

impl InteractiveLoginMethod {
    /// Methods a user can actually complete today.
    ///
    /// [`Self::OAuth`] is deliberately absent: it is not wired to the API yet,
    /// so offering it means a first-time user can pick the one option that
    /// cannot work and be told so only afterwards. A picker whose extra choice
    /// is a dead end is worse than no picker. Add it back here — nothing else
    /// needs to change — once the flow exists.
    const AVAILABLE: &'static [Self] = &[Self::ApiKey];

    fn label(self) -> &'static str {
        match self {
            Self::ApiKey => "API key",
            Self::OAuth => "OAuth",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_available_method_is_returned_without_prompting() {
        // This test passing at all is the assertion: it runs with no TTY, so it
        // would fail if the one-method case still opened a picker. It also pins
        // that the method offered is the one that works — OAuth is defined but
        // not wired up, and must not be reachable through this path.
        let method = prompt_login_method("https://app.memorylake.ai/openapi/memorylake")
            .expect("a single method needs no terminal");
        assert_eq!(method, InteractiveLoginMethod::ApiKey);
    }

    #[test]
    fn only_implemented_methods_are_offered() {
        assert_eq!(
            InteractiveLoginMethod::AVAILABLE,
            &[InteractiveLoginMethod::ApiKey],
            "a method that cannot complete must not be offered; \
             add it here together with its flow"
        );
    }
}
