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

    /// A configuration file could not be read, parsed, or validated.
    /// The message describes the problem in user-facing terms; raw
    /// secrets are never embedded.
    #[error("config error: {0}")]
    Config(String),

    /// A WebSocket transport error: handshake failure, protocol error,
    /// or unexpected disconnect.
    #[error("websocket error: {0}")]
    WebSocket(String),

    /// A WebSocket message could not be parsed into a [`crate::ws::Tick`].
    /// Malformed JSON, missing fields, or non-numeric price/quantity.
    #[error("malformed exchange message: {0}")]
    MalformedMessage(String),
}

/// Convenience alias for results returning [`CryptoTuiError`].
pub type Result<T> = std::result::Result<T, CryptoTuiError>;
