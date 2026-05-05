//! Streaming technical indicators built on fixed-capacity ring buffers.
//!
//! Every indicator in this module ingests one price tick at a time via
//! `update(price)` and returns `None` until enough warm-up data has
//! accumulated. After warm-up each tick yields a fresh value computed
//! in O(1) (or O(period) for stats requiring a window pass) — never by
//! recomputing over the full history.
//!
//! A unifying `Indicator` trait will land alongside the second
//! indicator (Bollinger Bands).

pub mod ring_buffer;
pub mod rsi;

pub use ring_buffer::RingBuffer;
pub use rsi::Rsi;
