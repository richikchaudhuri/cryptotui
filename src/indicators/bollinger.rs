//! Bollinger Bands — streaming implementation.
//!
//! ## Math
//!
//! Bollinger Bands describe a window of `N` recent prices using three
//! lines: the simple moving average and two offset bands.
//!
//! ```text
//! middle = (1 / N) * Σ price_i             (simple moving average)
//! σ      = sqrt( (1 / N) * Σ (price_i - middle)² )   (population stddev)
//! upper  = middle + k * σ
//! lower  = middle − k * σ
//! ```
//!
//! Defaults are `N = 20` and `k = 2`, matching the original 1980s
//! formulation. Population stddev (divisor `N`, not `N − 1`) matches
//! TradingView and most charting platforms.
//!
//! Each [`Bollinger::update`] is O(N) — the fixed-size sliding window
//! is small enough that recomputing the mean and variance from scratch
//! avoids the catastrophic cancellation an incremental sum-of-squares
//! algorithm would suffer at f64 precision over long runs.

use crate::error::{CryptoTuiError, Result};
use crate::indicators::ring_buffer::RingBuffer;

use super::{Bands, Indicator, IndicatorValue};

/// Default lookback window (in ticks).
pub const DEFAULT_PERIOD: usize = 20;
/// Default standard-deviation multiplier.
pub const DEFAULT_K: f64 = 2.0;

/// Streaming Bollinger Bands calculator.
///
/// Construct with [`Bollinger::new`] (custom period and multiplier)
/// or [`Bollinger::default`] (period = [`DEFAULT_PERIOD`], k =
/// [`DEFAULT_K`]). Feed ticks via [`Bollinger::update`], which returns
/// `None` until the window is full and `Some(Bands)` thereafter.
#[derive(Debug, Clone)]
pub struct Bollinger {
    period: usize,
    k: f64,
    window: RingBuffer<f64>,
}

impl Bollinger {
    /// Create a new Bollinger calculator with the given window size and
    /// stddev multiplier.
    ///
    /// Returns [`CryptoTuiError::InvalidConfig`] if `period == 0` or if
    /// `k` is not finite (NaN or ±∞).
    pub fn new(period: usize, k: f64) -> Result<Self> {
        if period == 0 {
            return Err(CryptoTuiError::InvalidConfig(
                "Bollinger period must be > 0".into(),
            ));
        }
        if !k.is_finite() {
            return Err(CryptoTuiError::InvalidConfig(
                "Bollinger k must be finite".into(),
            ));
        }
        Ok(Self {
            period,
            k,
            window: RingBuffer::new(period)?,
        })
    }

    /// Configured lookback period.
    pub fn period(&self) -> usize {
        self.period
    }

    /// Configured stddev multiplier.
    pub fn k(&self) -> f64 {
        self.k
    }

    /// Reset the sliding window. Configuration is preserved.
    pub fn reset(&mut self) {
        self.window.clear();
    }

    /// Ingest a new price tick.
    ///
    /// Returns `None` until the window has accumulated `period` ticks,
    /// then [`Bands`] computed from the current window on every tick.
    pub fn update(&mut self, price: f64) -> Option<Bands> {
        self.window.push(price);
        if !self.window.is_full() {
            return None;
        }
        let n = self.period as f64;
        let sum: f64 = self.window.iter().sum();
        let middle = sum / n;
        let variance: f64 = self
            .window
            .iter()
            .map(|p| {
                let d = p - middle;
                d * d
            })
            .sum::<f64>()
            / n;
        let sigma = variance.sqrt();
        let spread = self.k * sigma;
        Some(Bands {
            upper: middle + spread,
            middle,
            lower: middle - spread,
        })
    }
}

impl Default for Bollinger {
    fn default() -> Self {
        // DEFAULT_PERIOD is non-zero and DEFAULT_K is finite, so
        // construction is infallible.
        match Self::new(DEFAULT_PERIOD, DEFAULT_K) {
            Ok(b) => b,
            Err(_) => unreachable!("DEFAULT_PERIOD and DEFAULT_K are valid"),
        }
    }
}

impl Indicator for Bollinger {
    fn name(&self) -> &str {
        "bollinger"
    }

    fn update(&mut self, price: f64) -> Option<IndicatorValue> {
        Bollinger::update(self, price).map(IndicatorValue::Bands)
    }

    fn reset(&mut self) {
        Bollinger::reset(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(Bollinger::new(0, 2.0).is_err());
    }

    #[test]
    fn new_rejects_non_finite_k() {
        assert!(Bollinger::new(20, f64::NAN).is_err());
        assert!(Bollinger::new(20, f64::INFINITY).is_err());
        assert!(Bollinger::new(20, f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn default_uses_period_20_and_k_2() {
        let b = Bollinger::default();
        assert_eq!(b.period(), DEFAULT_PERIOD);
        assert_relative_eq!(b.k(), DEFAULT_K, max_relative = 1e-12);
    }

    #[test]
    fn returns_none_during_warmup() {
        let mut b = Bollinger::new(20, 2.0).unwrap();
        for i in 1..=19 {
            assert_eq!(b.update(i as f64), None, "tick {i} should be None");
        }
        assert!(b.update(20.0).is_some(), "tick 20 should yield bands");
    }

    #[test]
    fn bands_collapse_on_constant_prices() {
        // With zero variance, σ = 0 ⇒ upper = middle = lower = price.
        let mut b = Bollinger::new(20, 2.0).unwrap();
        for _ in 0..20 {
            b.update(100.0);
        }
        let bands = b.update(100.0).unwrap();
        assert_relative_eq!(bands.upper, 100.0, max_relative = 1e-12);
        assert_relative_eq!(bands.middle, 100.0, max_relative = 1e-12);
        assert_relative_eq!(bands.lower, 100.0, max_relative = 1e-12);
    }

    #[test]
    fn known_sequence_matches_hand_computation() {
        // Window: prices 1.0, 2.0, …, 20.0.
        //   mean      = (1 + 2 + … + 20) / 20 = 210 / 20 = 10.5
        //   variance  = Σ (i − 10.5)² / 20
        //             = 2 · (0.25 + 2.25 + 6.25 + … + 90.25) / 20
        //             = 665 / 20
        //             = 33.25 = 133 / 4
        //   σ         = √(133) / 2
        //   upper     = 10.5 + 2 · σ = 10.5 + √133
        //   lower     = 10.5 − √133
        let mut b = Bollinger::new(20, 2.0).unwrap();
        let mut last = None;
        for i in 1..=20 {
            last = b.update(i as f64);
        }
        let bands = last.expect("first bands at tick 20");
        let sqrt133 = 133.0_f64.sqrt();
        assert_relative_eq!(bands.middle, 10.5, max_relative = 1e-12);
        assert_relative_eq!(bands.upper, 10.5 + sqrt133, max_relative = 1e-12);
        assert_relative_eq!(bands.lower, 10.5 - sqrt133, max_relative = 1e-12);
    }

    #[test]
    fn window_slides_after_warmup() {
        // After the warm-up tick, push another value: the window
        // evicts the oldest. Fresh bands should reflect that.
        let mut b = Bollinger::new(20, 2.0).unwrap();
        for i in 1..=20 {
            b.update(i as f64);
        }
        // Push 21.0: window is now 2..=21, mean = 11.5, σ = √(133)/2.
        let bands = b.update(21.0).unwrap();
        let sqrt133 = 133.0_f64.sqrt();
        assert_relative_eq!(bands.middle, 11.5, max_relative = 1e-12);
        assert_relative_eq!(bands.upper, 11.5 + sqrt133, max_relative = 1e-12);
        assert_relative_eq!(bands.lower, 11.5 - sqrt133, max_relative = 1e-12);
    }

    #[test]
    fn reset_clears_window() {
        let mut b = Bollinger::new(5, 2.0).unwrap();
        for i in 1..=10 {
            b.update(i as f64);
        }
        b.reset();
        for i in 1..=4 {
            assert_eq!(b.update(i as f64), None, "post-reset tick {i}");
        }
        assert!(b.update(5.0).is_some(), "5th tick should yield bands");
    }

    #[test]
    fn implements_indicator_trait() {
        let mut b: Box<dyn Indicator> = Box::new(Bollinger::new(20, 2.0).unwrap());
        assert_eq!(b.name(), "bollinger");

        for i in 1..=19 {
            assert_eq!(b.update(i as f64), None, "tick {i} via trait");
        }
        let v = b.update(20.0).expect("bands at tick 20 via trait");
        let bands = v.as_bands().expect("expected Bands variant");
        assert_relative_eq!(bands.middle, 10.5, max_relative = 1e-12);

        b.reset();
        assert_eq!(b.update(1.0), None, "post-reset trait dispatch");
    }
}
