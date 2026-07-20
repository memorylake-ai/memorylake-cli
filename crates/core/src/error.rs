//! Public error types for `memorylake-core`.

use thiserror::Error;

/// Convenient `Result` alias for core APIs.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by core library operations.
#[derive(Debug, Error)]
pub enum Error {
    /// A generic placeholder until domain errors are defined.
    #[error("{0}")]
    Message(String),
}
