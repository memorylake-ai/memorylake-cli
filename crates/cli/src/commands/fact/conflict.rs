//! `memorylake fact conflict` commands.
//!
//! The server compares each scope's facts with each other (and a project's
//! facts with its documents) and records contradictions for review. These
//! commands list, inspect, and resolve them.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use memorylake_core::api::facts::conflicts::{
    ConflictCategory, ConflictFactEdit, ConflictResolution, ConflictType, ListConflictsParams,
    MAX_CONFLICT_PAGE_SIZE, get_conflict, list_conflicts, resolve_conflict,
};

use super::scope::{ScopeArgs, Session};
use crate::commands::print_json;

/// `fact conflict` subcommands.
#[derive(Debug, Subcommand)]
pub enum ConflictCommand {
    /// List a scope's memory conflicts, newest first.
    ///
    /// Filters combine with AND; a boolean filter left out returns both
    /// states.
    List {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Keep only resolved (`true`) or unresolved (`false`) conflicts.
        #[arg(long, value_name = "BOOL")]
        resolved: Option<bool>,
        /// Keep only conflicts between two facts (`m2m`), a fact and a
        /// document (`m2d`, projects only), or within one fact (`self`).
        #[arg(long, value_enum)]
        category: Option<CategoryArg>,
        /// Keep only `logical` or `knowledge` conflicts. Actors and agents
        /// only produce `logical` ones.
        #[arg(long, value_enum)]
        conflict_type: Option<ConflictTypeArg>,
        /// Keep only conflicts whose facts did (`true`) or did not (`false`)
        /// change after detection.
        #[arg(long, value_name = "BOOL")]
        stale: Option<bool>,
        /// Page size, 1-100. The server defaults to 20.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=i64::from(MAX_CONFLICT_PAGE_SIZE)))]
        page_size: Option<u32>,
        /// Continuation token from a previous page.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Show one conflict, including how it was resolved once it has been.
    Get {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Conflict id (`cfl-...`).
        #[arg(value_name = "CONFLICT_ID")]
        conflict_id: String,
    },
    /// Resolve a conflict. A conflict resolves once.
    ///
    /// The strategies a conflict accepts depend on its category:
    /// `m2m` (two facts): keep_fact (--keep-fact-id; the other fact is
    /// forgotten) or dismiss. `m2d` (fact vs document): trust_fact,
    /// trust_document, or dismiss. `self` (one fact): edit_fact (--edit) or
    /// dismiss. `dismiss` marks a false alarm and changes no fact.
    Resolve {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Conflict id (`cfl-...`).
        #[arg(value_name = "CONFLICT_ID")]
        conflict_id: String,
        /// How to resolve it.
        #[arg(long, value_enum)]
        strategy: StrategyArg,
        /// The fact to keep. Required by, and only accepted with, keep_fact.
        #[arg(long, value_name = "FACT_ID")]
        keep_fact_id: Option<String>,
        /// Replacement text for one of the conflict's facts, as
        /// `FACT_ID=TEXT`. Repeatable, one per fact; required by, and only
        /// accepted with, edit_fact. Facts left out keep their text.
        #[arg(long = "edit", value_name = "FACT_ID=TEXT", value_parser = parse_edit)]
        edits: Vec<ConflictFactEdit>,
    },
}

/// Conflict category as spelled on the command line (the wire values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CategoryArg {
    /// Two facts contradict each other.
    #[value(name = "m2m")]
    M2m,
    /// A fact contradicts a document (projects only).
    #[value(name = "m2d")]
    M2d,
    /// One fact contradicts itself.
    #[value(name = "self")]
    SelfConflict,
}

impl From<CategoryArg> for ConflictCategory {
    fn from(arg: CategoryArg) -> Self {
        match arg {
            CategoryArg::M2m => Self::FactVsFact,
            CategoryArg::M2d => Self::FactVsDocument,
            CategoryArg::SelfConflict => Self::SelfContradiction,
        }
    }
}

/// Conflict type as spelled on the command line (the wire values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ConflictTypeArg {
    /// The statements cannot all be true.
    #[value(name = "logical")]
    Logical,
    /// A statement disagrees with known material.
    #[value(name = "knowledge")]
    Knowledge,
}

impl From<ConflictTypeArg> for ConflictType {
    fn from(arg: ConflictTypeArg) -> Self {
        match arg {
            ConflictTypeArg::Logical => Self::Logical,
            ConflictTypeArg::Knowledge => Self::Knowledge,
        }
    }
}

/// Resolution strategy as spelled on the command line (the wire values).
///
/// Validated locally because the server answers an unknown strategy with
/// `INTERNAL_ERROR` rather than a validation error (measured 2026-10-09).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StrategyArg {
    /// Keep one of two contradicting facts and forget the other.
    #[value(name = "keep_fact")]
    KeepFact,
    /// Settle a fact-vs-document conflict in the fact's favor.
    #[value(name = "trust_fact")]
    TrustFact,
    /// Settle a fact-vs-document conflict in the document's favor.
    #[value(name = "trust_document")]
    TrustDocument,
    /// Mark the conflict a false alarm.
    #[value(name = "dismiss")]
    Dismiss,
    /// Replace the text of a self-contradicting fact.
    #[value(name = "edit_fact")]
    EditFact,
}

/// Parse one `--edit FACT_ID=TEXT` value.
///
/// Splits on the first `=`, so the text may contain more of them. Neither side
/// may be blank: the server requires replacement text for every edit.
fn parse_edit(raw: &str) -> std::result::Result<ConflictFactEdit, String> {
    let Some((fact_id, text)) = raw.split_once('=') else {
        return Err(format!("expected `FACT_ID=TEXT`, found `{raw}`"));
    };
    let fact_id = fact_id.trim();
    if fact_id.is_empty() {
        return Err(format!("fact id must not be empty in `{raw}`"));
    }
    if text.trim().is_empty() {
        return Err(format!("replacement text must not be blank in `{raw}`"));
    }
    Ok(ConflictFactEdit {
        fact_id: fact_id.to_string(),
        new_fact_text: text.to_string(),
    })
}

/// Pair the strategy with exactly the inputs it takes.
///
/// A stray input is rejected rather than dropped: the server answered
/// `dismiss` plus `keep_fact_id` with `INTERNAL_ERROR` (measured 2026-10-09),
/// and a silently ignored flag would hide a mistyped strategy.
fn build_resolution(
    strategy: StrategyArg,
    keep_fact_id: Option<String>,
    edits: Vec<ConflictFactEdit>,
) -> Result<ConflictResolution> {
    if strategy != StrategyArg::KeepFact && keep_fact_id.is_some() {
        bail!("--keep-fact-id is only accepted with --strategy keep_fact");
    }
    if strategy != StrategyArg::EditFact && !edits.is_empty() {
        bail!("--edit is only accepted with --strategy edit_fact");
    }
    Ok(match strategy {
        StrategyArg::KeepFact => {
            let Some(keep_fact_id) = keep_fact_id else {
                bail!("--strategy keep_fact requires --keep-fact-id <FACT_ID>");
            };
            ConflictResolution::KeepFact { keep_fact_id }
        }
        StrategyArg::EditFact => {
            if edits.is_empty() {
                bail!("--strategy edit_fact requires at least one --edit FACT_ID=TEXT");
            }
            let mut seen = BTreeSet::new();
            for edit in &edits {
                if !seen.insert(edit.fact_id.as_str()) {
                    bail!(
                        "fact `{}` is edited more than once; pass one --edit per fact",
                        edit.fact_id
                    );
                }
            }
            ConflictResolution::EditFact { edits }
        }
        StrategyArg::TrustFact => ConflictResolution::TrustFact,
        StrategyArg::TrustDocument => ConflictResolution::TrustDocument,
        StrategyArg::Dismiss => ConflictResolution::Dismiss,
    })
}

/// Execute a `fact conflict` subcommand.
pub fn run(
    command: ConflictCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    match command {
        ConflictCommand::List {
            scope,
            resolved,
            category,
            conflict_type,
            stale,
            page_size,
            continuation_token,
        } => {
            let (workspace, scope) = scope.resolve()?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let params = ListConflictsParams {
                resolved,
                category: category.map(Into::into),
                conflict_type: conflict_type.map(Into::into),
                stale,
                page_size,
                continuation_token,
            };
            let data = list_conflicts(&session.client, &workspace, &scope, &params)
                .context("list memory conflicts")?;
            print_json(&data)?;
        }
        ConflictCommand::Get { scope, conflict_id } => {
            let (workspace, scope) = scope.resolve()?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let data = get_conflict(&session.client, &workspace, &scope, &conflict_id)
                .with_context(|| format!("get memory conflict `{conflict_id}`"))?;
            print_json(&data)?;
        }
        ConflictCommand::Resolve {
            scope,
            conflict_id,
            strategy,
            keep_fact_id,
            edits,
        } => {
            let (workspace, scope) = scope.resolve()?;
            let resolution = build_resolution(strategy, keep_fact_id, edits)?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let data = resolve_conflict(
                &session.client,
                &workspace,
                &scope,
                &conflict_id,
                &resolution,
            )
            .with_context(|| format!("resolve memory conflict `{conflict_id}`"))?;
            print_json(&data)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(fact_id: &str, text: &str) -> ConflictFactEdit {
        ConflictFactEdit {
            fact_id: fact_id.into(),
            new_fact_text: text.into(),
        }
    }

    #[test]
    fn an_edit_splits_on_the_first_equals_sign() {
        assert_eq!(
            parse_edit("fact-1=a = b").expect("valid"),
            edit("fact-1", "a = b")
        );
    }

    #[test]
    fn malformed_edits_are_rejected() {
        assert!(parse_edit("fact-1").is_err());
        assert!(parse_edit("=text").is_err());
        assert!(parse_edit("fact-1=  ").is_err());
    }

    #[test]
    fn keep_fact_needs_its_fact_id() {
        let err = build_resolution(StrategyArg::KeepFact, None, vec![]).expect_err("no id");
        assert!(err.to_string().contains("--keep-fact-id"), "{err}");
        assert_eq!(
            build_resolution(StrategyArg::KeepFact, Some("fact-1".into()), vec![]).expect("valid"),
            ConflictResolution::KeepFact {
                keep_fact_id: "fact-1".into()
            }
        );
    }

    #[test]
    fn edit_fact_needs_distinct_edits() {
        let err = build_resolution(StrategyArg::EditFact, None, vec![]).expect_err("no edits");
        assert!(err.to_string().contains("--edit"), "{err}");
        let err = build_resolution(
            StrategyArg::EditFact,
            None,
            vec![edit("fact-1", "a"), edit("fact-1", "b")],
        )
        .expect_err("duplicate");
        assert!(err.to_string().contains("more than once"), "{err}");
    }

    #[test]
    fn inputs_for_another_strategy_are_rejected() {
        let err = build_resolution(StrategyArg::Dismiss, Some("fact-1".into()), vec![])
            .expect_err("stray keep id");
        assert!(err.to_string().contains("only accepted with"), "{err}");
        let err = build_resolution(
            StrategyArg::KeepFact,
            Some("f".into()),
            vec![edit("f", "t")],
        )
        .expect_err("stray edit");
        assert!(err.to_string().contains("only accepted with"), "{err}");
    }

    #[test]
    fn plain_strategies_resolve_without_inputs() {
        for (arg, expected) in [
            (StrategyArg::Dismiss, ConflictResolution::Dismiss),
            (StrategyArg::TrustFact, ConflictResolution::TrustFact),
            (
                StrategyArg::TrustDocument,
                ConflictResolution::TrustDocument,
            ),
        ] {
            assert_eq!(
                build_resolution(arg, None, vec![]).expect("valid"),
                expected
            );
        }
    }
}
