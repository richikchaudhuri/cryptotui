//! Wilder's Relative Strength Index — streaming implementation.
//!
//! ## Math
//!
//! For each tick after the first, compute `change = price - prev_price`
//! and split it into a non-negative gain and loss:
//! `gain = max(change, 0)`, `loss = max(-change, 0)`.
//!
//! For the first `period` changes we accumulate the simple mean of
//! gains and losses (the "warm-up" using two ring buffers). After
//! warm-up, Wilder's smoothing applies on every tick:
//!
//! ```text
//! avg_gain_t = (avg_gain_{t-1} * (N - 1) + gain_t) / N
//! avg_loss_t = (avg_loss_{t-1} * (N - 1) + loss_t) / N
//! RS         = avg_gain / avg_loss
//! RSI        = 100 - 100 / (1 + RS)
//! ```
//!
//! The warm-up consumes the first `period + 1` ticks (one to anchor
//! `prev_price`, then `period` more to fill the gain/loss buffers).
//!
//! Edge cases:
//! - both averages zero → RSI = 50 (no movement, neutral)
//! - only `avg_loss` zero → RSI = 100 (all gains)
//! - only `avg_gain` zero → RSI = 0 (all losses)

use crate::error::{CryptoTuiError, Result};
use crate::indicators::ring_buffer::RingBuffer;

/// Default RSI period used by Welles Wilder.
pub const DEFAULT_PERIOD: usize = 14;

/// Streaming Wilder RSI calculator.
///
/// Construct with [`Rsi::new`] (custom period) or [`Rsi::default`]
/// (period = [`DEFAULT_PERIOD`]). Feed ticks in via [`Rsi::update`],
/// which returns `None` during warm-up and `Some(rsi)` thereafter.
#[derive(Debug, Clone)]
pub struct Rsi {
    period: usize,
    prev_price: Option<f64>,
    gain_warmup: RingBuffer<f64>,
    loss_warmup: RingBuffer<f64>,
    avg_gain: Option<f64>,
    avg_loss: Option<f64>,
}

impl Rsi {
    /// Create an RSI calculator with the given Wilder period.
    ///
    /// Returns [`CryptoTuiError::InvalidConfig`] if `period == 0`.
    pub fn new(period: usize) -> Result<Self> {
        if period == 0 {
            return Err(CryptoTuiError::InvalidConfig(
                "RSI period must be > 0".into(),
            ));
        }
        Ok(Self {
            period,
            prev_price: None,
            gain_warmup: RingBuffer::new(period)?,
            loss_warmup: RingBuffer::new(period)?,
            avg_gain: None,
            avg_loss: None,
        })
    }

    /// Configured Wilder period.
    pub fn period(&self) -> usize {
        self.period
    }

    /// Reset all internal state, preserving the configured period.
    pub fn reset(&mut self) {
        self.prev_price = None;
        self.gain_warmup.clear();
        self.loss_warmup.clear();
        self.avg_gain = None;
        self.avg_loss = None;
    }

    /// Ingest a new price tick.
    ///
    /// Returns `None` until at least `period + 1` ticks have been seen,
    /// then `Some(rsi)` on every subsequent tick.
    pub fn update(&mut self, price: f64) -> Option<f64> {
        let prev = match self.prev_price {
            None => {
                self.prev_price = Some(price);
                return None;
            }
            Some(p) => p,
        };
        self.prev_price = Some(price);

        let change = price - prev;
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };

        match (self.avg_gain, self.avg_loss) {
            (Some(g), Some(l)) => {
                let n = self.period as f64;
                let new_g = (g * (n - 1.0) + gain) / n;
                let new_l = (l * (n - 1.0) + loss) / n;
                self.avg_gain = Some(new_g);
                self.avg_loss = Some(new_l);
                Some(rsi_from(new_g, new_l))
            }
            _ => {
                self.gain_warmup.push(gain);
                self.loss_warmup.push(loss);
                if self.gain_warmup.is_full() {
                    let g = mean(&self.gain_warmup);
                    let l = mean(&self.loss_warmup);
                    self.gain_warmup.clear();
                    self.loss_warmup.clear();
                    self.avg_gain = Some(g);
                    self.avg_loss = Some(l);
                    Some(rsi_from(g, l))
                } else {
                    None
                }
            }
        }
    }
}

impl Default for Rsi {
    fn default() -> Self {
        // DEFAULT_PERIOD is a non-zero const, so construction is infallible.
        match Self::new(DEFAULT_PERIOD) {
            Ok(rsi) => rsi,
            Err(_) => unreachable!("DEFAULT_PERIOD is non-zero"),
        }
    }
}

fn mean(buf: &RingBuffer<f64>) -> f64 {
    let n = buf.len();
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = buf.iter().sum();
    sum / n as f64
}

fn rsi_from(avg_gain: f64, avg_loss: f64) -> f64 {
    match (avg_gain == 0.0, avg_loss == 0.0) {
        (true, true) => 50.0,
        (true, false) => 0.0,
        (false, true) => 100.0,
        (false, false) => {
            let rs = avg_gain / avg_loss;
            100.0 - 100.0 / (1.0 + rs)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn new_rejects_zero_period() {
        assert!(Rsi::new(0).is_err());
    }

    #[test]
    fn returns_none_during_warmup() {
        // period = 14: ticks 1..=14 must all return None,
        // tick 15 yields the first value.
        let mut rsi = Rsi::new(14).unwrap();
        for i in 1..=14 {
            assert_eq!(rsi.update(i as f64), None, "tick {i} should be None");
        }
        assert!(rsi.update(15.0).is_some(), "tick 15 should yield a value");
    }

    #[test]
    fn flat_prices_yield_50() {
        // No movement at all ⇒ both averages are zero ⇒ neutral 50.
        let mut rsi = Rsi::new(14).unwrap();
        for _ in 0..14 {
            assert_eq!(rsi.update(100.0), None);
        }
        let v = rsi.update(100.0).unwrap();
        assert_relative_eq!(v, 50.0, max_relative = 1e-12);
    }

    #[test]
    fn monotonically_rising_yields_100() {
        // 15 strictly increasing closes ⇒ 14 positive changes, 0 losses.
        let mut rsi = Rsi::new(14).unwrap();
        for i in 1..=14 {
            assert_eq!(rsi.update(i as f64), None);
        }
        let v = rsi.update(15.0).unwrap();
        assert_relative_eq!(v, 100.0, max_relative = 1e-12);
    }

    #[test]
    fn monotonically_falling_yields_0() {
        let mut rsi = Rsi::new(14).unwrap();
        for i in (2..=15).rev() {
            assert_eq!(rsi.update(i as f64), None);
        }
        let v = rsi.update(1.0).unwrap();
        assert_relative_eq!(v, 0.0, max_relative = 1e-12);
    }

    #[test]
    fn known_mixed_sequence_matches_hand_computation() {
        // 15 closes, 14 changes:
        //   gains  +1.0 +1.0 +0.5 +1.5 +0.5 +1.5 +0.5 +1.5 +0.5  =  8.5
        //   losses 0.5  1.0  1.0  1.0  1.0                       =  4.5
        // avg_gain = 8.5 / 14 = 17/28
        // avg_loss = 4.5 / 14 =  9/28
        // RS = 17/9
        // RSI = 100 - 100/(1 + 17/9) = 850/13 ≈ 65.384615...
        let prices = [
            10.0, 11.0, 10.5, 11.5, 12.0, 11.0, 12.5, 13.0, 12.0, 13.5, 14.0, 13.0, 14.5, 15.0,
            14.0,
        ];
        let mut rsi = Rsi::new(14).unwrap();
        let mut last = None;
        for p in prices {
            last = rsi.update(p);
        }
        let v = last.expect("first RSI must arrive after 15 ticks");
        assert_relative_eq!(v, 850.0 / 13.0, max_relative = 1e-12);
    }

    #[test]
    fn smoothing_kicks_in_after_warmup() {
        // Continue from the mixed sequence, then push +1.0 at tick 16.
        // Hand computation:
        //   prev avg_gain = 17/28, prev avg_loss = 9/28, N = 14
        //   gain_16 = 1.0, loss_16 = 0
        //   new_g = (17/28 * 13 + 1) / 14 = 249/392
        //   new_l = (9/28  * 13)     / 14 = 117/392
        //   RS    = 249/117
        //   RSI   = 100 - 11700/366 = 4150/61 ≈ 68.032786...
        let prices = [
            10.0, 11.0, 10.5, 11.5, 12.0, 11.0, 12.5, 13.0, 12.0, 13.5, 14.0, 13.0, 14.5, 15.0,
            14.0,
        ];
        let mut rsi = Rsi::new(14).unwrap();
        for p in prices {
            rsi.update(p);
        }
        let v = rsi.update(15.0).expect("smoothing should yield a value");
        assert_relative_eq!(v, 4150.0 / 61.0, max_relative = 1e-12);
    }

    #[test]
    fn reset_clears_state_and_re_runs_warmup() {
        let mut rsi = Rsi::new(14).unwrap();
        for i in 1..=20 {
            rsi.update(i as f64);
        }
        rsi.reset();
        for i in 1..=14 {
            assert_eq!(rsi.update(i as f64), None, "post-reset tick {i}");
        }
        assert!(rsi.update(15.0).is_some());
    }

    #[test]
    fn default_uses_period_14() {
        let rsi = Rsi::default();
        assert_eq!(rsi.period(), DEFAULT_PERIOD);
    }

    #[test]
    fn custom_period_warmup_length() {
        let mut rsi = Rsi::new(5).unwrap();
        for i in 1..=5 {
            assert_eq!(rsi.update(i as f64), None, "tick {i} during 5-warmup");
        }
        assert!(rsi.update(6.0).is_some(), "6th tick should yield a value");
    }

    #[test]
    fn rsi_stays_in_unit_interval() {
        // Random-ish walk: every produced value must be in [0, 100].
        let mut rsi = Rsi::new(14).unwrap();
        let walk: [f64; 60] = [
            100.0, 101.5, 100.7, 99.8, 102.3, 103.1, 102.5, 101.9, 100.4, 99.2, 98.7, 99.9, 101.0,
            102.6, 103.4, 102.1, 100.5, 99.0, 98.4, 97.6, 98.1, 99.3, 100.8, 102.0, 103.7, 104.2,
            103.5, 102.9, 101.4, 100.1, 99.5, 98.0, 96.8, 97.4, 98.9, 100.2, 101.7, 103.0, 104.5,
            105.1, 104.3, 103.0, 101.5, 100.0, 98.5, 97.0, 96.0, 97.5, 99.0, 100.5, 102.0, 103.5,
            105.0, 104.0, 102.5, 101.0, 100.0, 99.0, 98.5, 99.5,
        ];
        for p in walk {
            if let Some(v) = rsi.update(p) {
                assert!((0.0..=100.0).contains(&v), "RSI {v} out of range");
            }
        }
    }
}
