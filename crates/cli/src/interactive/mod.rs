//! Shared interactive prompts for the CLI.
//!
//! Use these helpers instead of calling `dialoguer` directly so Ctrl-C always
//! restores cursor / ECHO for the user's shell.

mod tty;

pub use tty::{TerminalGuard, map_prompt_error, prepare};

use anyhow::{Context, Result, bail};
use dialoguer::{Input, Password, Select, theme::ColorfulTheme};

/// Select one item by index from `items`.
pub fn select_index(prompt: impl Into<String>, items: &[&str], default: usize) -> Result<usize> {
    prepare()?;
    let _guard = TerminalGuard::new();
    Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(items)
        .default(default)
        .interact()
        .map_err(map_prompt_error)
        .context("select option")
}

/// Prompt for a hidden secret (API key, password, …).
pub fn prompt_secret(prompt: impl Into<String>) -> Result<String> {
    prepare()?;
    let _guard = TerminalGuard::new();
    let value = Password::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .interact()
        .map_err(map_prompt_error)
        .context("read secret")?;
    let value = value.trim().to_string();
    if value.is_empty() {
        bail!("value must not be empty");
    }
    Ok(value)
}

/// Prompt for a visible line of text.
pub fn prompt_line(prompt: impl Into<String>) -> Result<String> {
    prepare()?;
    let _guard = TerminalGuard::new();
    let value: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .interact_text()
        .map_err(map_prompt_error)
        .context("read input")?;
    Ok(value.trim().to_string())
}

/// Prompt for a visible line with a default value.
#[allow(dead_code)] // available for future interactive commands
pub fn prompt_line_with_default(
    prompt: impl Into<String>,
    default: impl Into<String>,
) -> Result<String> {
    prepare()?;
    let _guard = TerminalGuard::new();
    let value: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(default.into())
        .interact_text()
        .map_err(map_prompt_error)
        .context("read input")?;
    Ok(value.trim().to_string())
}
