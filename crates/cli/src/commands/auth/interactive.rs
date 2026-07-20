//! Interactive prompts for `auth login`.

use anyhow::Result;

use crate::interactive::{prompt_secret, select_index};

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
