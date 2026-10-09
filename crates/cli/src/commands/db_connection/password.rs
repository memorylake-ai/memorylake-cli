//! Where a database password comes from.
//!
//! There is deliberately no `--password <VALUE>` flag: a value on the command
//! line lands in shell history and in the process list for every other user
//! of the machine to read. The password is read from an environment variable,
//! standard input, or a file instead, or prompted for without echo when the
//! command runs at a terminal and none of those was given.

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;
use memorylake_core::api::db_connections::DbPassword;

use crate::commands::input::{read_file_or_stdin, strip_one_line_ending};
use crate::interactive::prompt_secret_verbatim;

/// The three ways to hand over a password; at most one may be used.
#[derive(Debug, Clone, Default, Args)]
#[group(id = "password_source", multiple = false)]
pub struct PasswordArgs {
    /// Read the password from this environment variable, e.g. `PGPASSWORD`.
    #[arg(long, value_name = "VAR")]
    pub password_env: Option<String>,
    /// Read the password from standard input.
    ///
    /// One trailing newline is dropped, so `printf` and `echo` both work.
    /// When standard input is a terminal, asks without echo instead.
    #[arg(long)]
    pub password_stdin: bool,
    /// Read the password from this file.
    ///
    /// One trailing newline is dropped.
    #[arg(long, value_name = "PATH")]
    pub password_file: Option<PathBuf>,
}

/// Whether the command cannot proceed without a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordNeed {
    /// Required: prompt at a terminal, fail elsewhere.
    Required,
    /// Optional: no source means no password is sent.
    Optional,
}

impl PasswordArgs {
    /// Read the password from whichever source was named.
    ///
    /// With no source, a required password is prompted for when standard
    /// input is a terminal; otherwise `missing` explains what to pass.
    pub fn resolve(&self, need: PasswordNeed, missing: &str) -> Result<Option<DbPassword>> {
        let password = if let Some(var) = &self.password_env {
            match std::env::var(var) {
                Ok(value) => value,
                Err(std::env::VarError::NotPresent) => {
                    bail!("--password-env: environment variable `{var}` is not set")
                }
                Err(std::env::VarError::NotUnicode(_)) => {
                    bail!("--password-env: environment variable `{var}` is not valid UTF-8")
                }
            }
        } else if self.password_stdin {
            if std::io::stdin().is_terminal() {
                // Reading a terminal line would echo the password as it is
                // typed; ask without echo instead.
                prompt_password()?
            } else {
                let text = read_file_or_stdin(std::path::Path::new("-"), "password")?;
                strip_one_line_ending(&text).to_string()
            }
        } else if let Some(path) = &self.password_file {
            let text = read_file_or_stdin(path, "password file")?;
            strip_one_line_ending(&text).to_string()
        } else {
            return match need {
                PasswordNeed::Optional => Ok(None),
                PasswordNeed::Required if std::io::stdin().is_terminal() => {
                    Ok(Some(DbPassword::new(prompt_password()?)))
                }
                PasswordNeed::Required => bail!(
                    "{missing}\n\
                     pass it with --password-env <VAR>, --password-stdin, or --password-file <PATH>\n\
                     (there is no --password flag: a value there would be saved in shell history)"
                ),
            };
        };

        if password.is_empty() {
            bail!("the database password is empty");
        }
        Ok(Some(DbPassword::new(password)))
    }
}

/// Ask for the password without echo, keeping it exactly as typed — the same
/// as a file or standard input would deliver it.
fn prompt_password() -> Result<String> {
    prompt_secret_verbatim("Database password").context("read password")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_optional_password_with_no_source_is_absent() {
        let args = PasswordArgs::default();
        assert!(
            args.resolve(PasswordNeed::Optional, "unused")
                .expect("optional")
                .is_none()
        );
    }

    #[test]
    fn a_password_file_drops_one_trailing_newline() {
        let dir = std::env::temp_dir().join(format!("mlcli-pw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pw");
        std::fs::write(&path, " s3cret \n").unwrap();

        let args = PasswordArgs {
            password_file: Some(path),
            ..Default::default()
        };
        let password = args
            .resolve(PasswordNeed::Required, "unused")
            .expect("read")
            .expect("present");
        assert_eq!(password.expose(), " s3cret ");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_empty_password_file_is_rejected() {
        let dir = std::env::temp_dir().join(format!("mlcli-pw-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pw");
        std::fs::write(&path, "\n").unwrap();

        let args = PasswordArgs {
            password_file: Some(path),
            ..Default::default()
        };
        let err = args
            .resolve(PasswordNeed::Required, "unused")
            .expect_err("empty");
        assert!(err.to_string().contains("empty"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unset_environment_variable_is_named() {
        let args = PasswordArgs {
            password_env: Some("MLCLI_TEST_SURELY_UNSET_PASSWORD_VAR".into()),
            ..Default::default()
        };
        let err = args
            .resolve(PasswordNeed::Required, "unused")
            .expect_err("unset");
        assert!(
            err.to_string()
                .contains("MLCLI_TEST_SURELY_UNSET_PASSWORD_VAR"),
            "{err}"
        );
    }
}
