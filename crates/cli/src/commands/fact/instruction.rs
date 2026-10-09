//! `memorylake fact instruction` commands.
//!
//! A fact instruction is Markdown saying what a scope's memory is about and
//! what it is not; the server uses it to narrow what it records there. It is
//! stored in the scope's memory settings, and an empty value means the
//! built-in default is in effect.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::facts::settings::{
    DraftFactInstructionRequest, MAX_DRAFT_GUIDANCE_LEN, MAX_DRAFT_LANGUAGE_LEN,
    MAX_FACT_INSTRUCTION_LEN, UpdateMemorySettingsRequest, draft_fact_instruction,
    get_memory_settings, update_memory_settings,
};

use super::scope::{ScopeArgs, Session};
use crate::commands::print_json;

/// `fact instruction` subcommands.
#[derive(Debug, Subcommand)]
pub enum InstructionCommand {
    /// Show the scope's fact instruction.
    ///
    /// An empty `fact_instruction` means none is set and the built-in default
    /// is in effect.
    Get {
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Replace the scope's fact instruction.
    ///
    /// The new Markdown replaces the old one whole; it is not appended. At
    /// most 2000 characters (counted as UTF-16 units, so most emoji count
    /// twice). Use `clear` to go back to the built-in default.
    Set {
        #[command(flatten)]
        scope: ScopeArgs,
        /// The instruction text. Exactly one of --text / --file.
        #[arg(long)]
        text: Option<String>,
        /// Read the instruction from this Markdown file; `-` reads stdin.
        /// Exactly one of --text / --file.
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
    },
    /// Remove the scope's fact instruction, restoring the built-in default.
    Clear {
        #[command(flatten)]
        scope: ScopeArgs,
    },
    /// Have the server write a candidate instruction. Nothing is saved.
    ///
    /// One synchronous language-model call: it usually takes a few seconds,
    /// longer when the scope holds many facts. Review or edit the draft, then
    /// save it with `fact instruction set`.
    Draft {
        #[command(flatten)]
        scope: ScopeArgs,
        /// What the draft should say, in any language (at most 2000
        /// characters). Omit it to draft from the defaults.
        #[arg(long)]
        guidance: Option<String>,
        /// Language to draft in, as an English language name such as
        /// "Simplified Chinese" or "Japanese" — not a locale tag like `zh-CN`.
        /// English when omitted.
        #[arg(long)]
        language: Option<String>,
        /// Let the draft read the facts already stored in this scope.
        #[arg(long)]
        use_existing_facts: bool,
        /// Let the draft read the names and summaries of the scope's
        /// documents. Only projects hold documents.
        #[arg(long)]
        use_documents: bool,
    },
}

/// Length as the server counts it: UTF-16 code units (it is a Java string).
fn server_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Reject `text` when it is longer than `max` server-counted characters.
fn check_len(what: &str, text: &str, max: usize) -> Result<()> {
    let len = server_len(text);
    if len > max {
        bail!("{what} is {len} characters long; the API accepts at most {max}");
    }
    Ok(())
}

/// Read the `set` text from exactly one of `--text` / `--file`.
fn read_instruction(text: Option<String>, file: Option<&Path>) -> Result<String> {
    let instruction = match (text, file) {
        (Some(_), Some(_)) => bail!("pass --text or --file, not both"),
        (None, None) => bail!("the instruction is required: pass --text <TEXT> or --file <PATH>"),
        (Some(text), None) => text,
        (None, Some(path)) if path == Path::new("-") => {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .context("read instruction from stdin")?;
            buffer
        }
        (None, Some(path)) => std::fs::read_to_string(path)
            .with_context(|| format!("read instruction file {}", path.display()))?,
    };
    validate_instruction(&instruction)?;
    Ok(instruction)
}

/// Check an instruction about to be saved.
///
/// Blank text is rejected: the server stores whitespace verbatim (measured
/// 2026-10-09), so it would replace the default with an empty-looking
/// instruction instead of restoring it — `clear` does that.
fn validate_instruction(instruction: &str) -> Result<()> {
    if instruction.trim().is_empty() {
        bail!(
            "the instruction must not be blank; use `fact instruction clear` to restore the default"
        );
    }
    check_len("the instruction", instruction, MAX_FACT_INSTRUCTION_LEN)
}

/// Build the draft request, checking the documented length limits.
fn build_draft(
    guidance: Option<String>,
    language: Option<String>,
    use_existing_facts: bool,
    use_documents: bool,
) -> Result<DraftFactInstructionRequest> {
    if let Some(guidance) = &guidance {
        check_len("--guidance", guidance, MAX_DRAFT_GUIDANCE_LEN)?;
    }
    if let Some(language) = &language {
        if language.trim().is_empty() {
            bail!("--language must not be blank; omit it to draft in English");
        }
        check_len("--language", language, MAX_DRAFT_LANGUAGE_LEN)?;
    }
    Ok(DraftFactInstructionRequest {
        guidance,
        language,
        // Unset flags stay off the wire; the server defaults both to false.
        use_existing_facts: use_existing_facts.then_some(true),
        use_documents: use_documents.then_some(true),
    })
}

/// Execute a `fact instruction` subcommand.
pub fn run(
    command: InstructionCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    match command {
        InstructionCommand::Get { scope } => {
            let (workspace, scope) = scope.resolve()?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let data = get_memory_settings(&session.client, &workspace, &scope)
                .context("get fact instruction")?;
            print_json(&data)?;
        }
        InstructionCommand::Set { scope, text, file } => {
            let (workspace, scope) = scope.resolve()?;
            let instruction = read_instruction(text, file.as_deref())?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let request = UpdateMemorySettingsRequest {
                fact_instruction: Some(instruction),
            };
            let data = update_memory_settings(&session.client, &workspace, &scope, &request)
                .context("set fact instruction")?;
            print_json(&data)?;
        }
        InstructionCommand::Clear { scope } => {
            let (workspace, scope) = scope.resolve()?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            // The API's spelling of "clear": an empty string, not null.
            let request = UpdateMemorySettingsRequest {
                fact_instruction: Some(String::new()),
            };
            let data = update_memory_settings(&session.client, &workspace, &scope, &request)
                .context("clear fact instruction")?;
            print_json(&data)?;
        }
        InstructionCommand::Draft {
            scope,
            guidance,
            language,
            use_existing_facts,
            use_documents,
        } => {
            let (workspace, scope) = scope.resolve()?;
            let request = build_draft(guidance, language, use_existing_facts, use_documents)?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let data = draft_fact_instruction(&session.client, &workspace, &scope, &request)
                .context("draft fact instruction")?;
            print_json(&data)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_is_counted_in_utf16_units() {
        assert_eq!(server_len("记录"), 2);
        assert_eq!(server_len("\u{1F600}"), 2);
        assert!(validate_instruction(&"记".repeat(2000)).is_ok());
        assert!(validate_instruction(&"\u{1F600}".repeat(1001)).is_err());
        assert!(validate_instruction(&"a".repeat(2001)).is_err());
    }

    #[test]
    fn a_blank_instruction_points_at_clear() {
        let err = validate_instruction(" \n").expect_err("blank");
        assert!(err.to_string().contains("clear"), "{err}");
    }

    #[test]
    fn exactly_one_source_is_required() {
        let err = read_instruction(None, None).expect_err("no source");
        assert!(err.to_string().contains("--text"), "{err}");
        let err =
            read_instruction(Some("x".into()), Some(Path::new("a.md"))).expect_err("two sources");
        assert!(err.to_string().contains("not both"), "{err}");
        assert_eq!(read_instruction(Some("x".into()), None).expect("text"), "x");
    }

    #[test]
    fn unset_draft_flags_stay_off_the_wire() {
        let request = build_draft(None, None, false, false).expect("valid");
        assert_eq!(request, DraftFactInstructionRequest::default());
        let request = build_draft(None, Some("Japanese".into()), true, true).expect("valid");
        assert_eq!(request.use_existing_facts, Some(true));
        assert_eq!(request.use_documents, Some(true));
    }

    #[test]
    fn draft_inputs_respect_their_limits() {
        assert!(build_draft(Some("g".repeat(2001)), None, false, false).is_err());
        assert!(build_draft(None, Some("l".repeat(65)), false, false).is_err());
        assert!(build_draft(None, Some(" ".into()), false, false).is_err());
    }
}
