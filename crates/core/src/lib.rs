//! Core library for the MemoryLake CLI.

pub mod config;
pub mod error;

pub use config::Config;
pub use error::{Error, Result};
