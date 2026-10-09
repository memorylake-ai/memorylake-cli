//! Integration tests for the `memorylake` binary.
//!
//! Layout (add new command suites as sibling modules):
//!
//! ```text
//! tests/
//!   cli_commands.rs      # this harness
//!   common/              # shared process helpers
//!   admin/{offline,wire}.rs   # team / api-key / member / invitation / usage
//!   meta/                # version, --help, …
//!   actor/{offline,live}.rs
//!   library/{offline,live}.rs
//!   agent/{offline,live}.rs
//!   auth/{offline,live}.rs
//!   workspace/{offline,live}.rs
//!   project/{offline,live}.rs
//!   search/{offline,wire,live}.rs
//!   document/{offline,live}.rs
//!   fact/{offline,live}.rs
//!   conversation/{offline,wire,live}.rs
//!   db_connection/{offline,wire,live}.rs
//!   datasource/{offline,wire,live}.rs
//!   database/{offline,wire,live}.rs   # project database
//! ```
//!
//! Offline tests isolate config under a temporary `$HOME`.
//! Live API tests require `MEMORYLAKE_API_KEY` (env or repo-root `.env`) and
//! clean up the objects they create in the real workspace.

mod actor;
mod admin;
mod agent;
mod auth;
mod common;
mod conversation;
mod database;
mod datasource;
mod db_connection;
mod document;
mod fact;
mod library;

mod meta;
mod project;
mod search;
mod workspace;
