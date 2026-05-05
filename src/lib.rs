//! cryptotui — terminal dashboard for live crypto markets.
//!
//! This crate provides streaming technical indicators (a generic
//! [`indicators::RingBuffer`] and Wilder's [`indicators::Rsi`] today,
//! with Bollinger Bands and friends to follow). The Binance WebSocket
//! pipeline and ratatui rendering live in the binary and arrive in
//! subsequent commits.
//!
//! ## Why ring buffers, not DataFrames?
//!
//! Batch-analytics libraries (Polars, pandas) shine on DataFrames
//! loaded at rest; our hot path is per-tick streaming data, where
//! constructing and tearing down a DataFrame on every tick is the
//! wrong shape. A fixed-capacity ring buffer with O(1) updates is
//! the right primitive. Batch tooling may re-enter the project later
//! for historical reconciliation, where it actually fits.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![warn(missing_docs)]

pub mod error;
pub mod indicators;
