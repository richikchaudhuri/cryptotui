//! Top-level error type for the cryptotui library.
//!
//! Library code returns `Result<T, CryptoTuiError>`. The binary entry
//! point converts these into [`anyhow::Error`] at the boundary so it
//! can mix freely with errors from third-party crates.

use thiserror::Error;

/// Top-level error variants produced by the cryptotui library.
#[derive(Debug, Error)]
pub enum CryptoTuiError {
    /// An indicator or buffer was constructed with an invalid setting
    /// (for example, a zero period or zero capacity).
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Convenience alias for results returning [`CryptoTuiError`].
pub type Result<T> = std::result::Result<T, CryptoTuiError>;
