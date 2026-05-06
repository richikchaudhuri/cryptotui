//! Streaming technical indicators built on fixed-capacity ring buffers.
//!
//! Every indicator in this module ingests one price tick at a time via
//! `update(price)` and returns `None` until enough warm-up data has
//! accumulated. After warm-up each tick yields a fresh value computed
//! in O(1) (or O(period) for stats requiring a window pass) — never by
//! recomputing over the full history.
//!
//! The [`Indicator`] trait unifies these concrete types so the binary
//! and TUI can dispatch over `Vec<Box<dyn Indicator>>` without caring
//! which kind of indicator each entry is.

pub mod bollinger;
pub mod ring_buffer;
pub mod rsi;

pub use bollinger::Bollinger;
pub use ring_buffer::RingBuffer;
pub use rsi::Rsi;

/// Three-band output produced by indicators like Bollinger Bands.
///
/// `middle` is the central tendency (typically a simple moving
/// average); `upper` and `lower` are offsets above and below by some
/// multiple of the rolling standard deviation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bands {
    /// Upper band (e.g. SMA + k·σ).
    pub upper: f64,
    /// Middle band (e.g. SMA over the window).
    pub middle: f64,
    /// Lower band (e.g. SMA − k·σ).
    pub lower: f64,
}

/// A value emitted by a streaming indicator on a given tick.
///
/// Indicators that produce a single number (RSI, SMA, EMA, …) yield
/// [`IndicatorValue::Single`]. Indicators that produce three numbers
/// at once (Bollinger Bands, Keltner Channels, …) yield
/// [`IndicatorValue::Bands`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IndicatorValue {
    /// A scalar reading.
    Single(f64),
    /// A three-band reading.
    Bands(Bands),
}

impl IndicatorValue {
    /// Returns the inner scalar if this is a [`IndicatorValue::Single`],
    /// otherwise `None`.
    pub fn as_single(&self) -> Option<f64> {
        match self {
            IndicatorValue::Single(v) => Some(*v),
            IndicatorValue::Bands(_) => None,
        }
    }

    /// Returns the inner [`Bands`] if this is a [`IndicatorValue::Bands`],
    /// otherwise `None`.
    pub fn as_bands(&self) -> Option<Bands> {
        match self {
            IndicatorValue::Single(_) => None,
            IndicatorValue::Bands(b) => Some(*b),
        }
    }
}

/// A streaming technical indicator: ingests one price at a time and
/// emits an optional reading.
///
/// Implementors must be safe to send across threads (`Send + Sync`)
/// so the binary can hold them in a `Vec<Box<dyn Indicator>>` and
/// drive them from any task.
pub trait Indicator: Send + Sync {
    /// Short, lowercase name suitable for labels and config keys
    /// (e.g. `"rsi"`, `"bollinger"`).
    fn name(&self) -> &str;

    /// Ingest one price tick. Returns `None` during warm-up, then a
    /// fresh value on every subsequent tick.
    fn update(&mut self, price: f64) -> Option<IndicatorValue>;

    /// Reset all internal state. Configuration (period, multiplier,
    /// …) is preserved; the indicator behaves like a fresh instance
    /// after this call.
    fn reset(&mut self);
}
