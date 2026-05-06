//! Application state and the tokio task that drives it.
//!
//! [`AppState`] owns the live data the rest of the binary cares about:
//! a bounded ring of recent (timestamp, price) samples for the chart,
//! a parallel ring of Bollinger-band readings (so the chart can overlay
//! the bands at each historical point), the indicator instances, their
//! most recent emitted readings, and the current WebSocket lifecycle.
//! It is renderer-agnostic — the [`crate::tui`] module is the only
//! caller today, but a print-only pipeline could trivially reuse the
//! same struct.

use std::collections::VecDeque;

use crate::config::Config;
use crate::error::Result;
use crate::indicators::{Bands, Bollinger, Indicator, IndicatorValue, Rsi};
use crate::ws::{Tick, WsStatus};

/// Position of the RSI indicator inside [`AppState::indicators`].
pub const RSI_INDEX: usize = 0;
/// Position of the Bollinger indicator inside [`AppState::indicators`].
pub const BOLLINGER_INDEX: usize = 1;

/// Live application state.
///
/// A bounded `VecDeque` retains the most recent ticks for the chart;
/// older samples are evicted from the front as new ones arrive at the
/// back. [`Self::bollinger_history`] is kept in lock-step with
/// [`Self::history`] so the chart can pair every price point with the
/// band reading observed on the same tick (or `None` during warm-up).
pub struct AppState {
    /// Resolved configuration the app was started with.
    pub config: Config,
    /// `(timestamp_ms, price)` samples in arrival order.
    /// The tail is the most recent; capacity is `config.history_capacity`.
    pub history: VecDeque<(u64, f64)>,
    /// Bollinger reading observed on the same tick as the corresponding
    /// entry of [`Self::history`]. Same length as `history` at all times.
    pub bollinger_history: VecDeque<Option<Bands>>,
    /// Streaming indicators, in display order.
    pub indicators: Vec<Box<dyn Indicator>>,
    /// Latest non-`None` reading per indicator, parallel to
    /// [`Self::indicators`].
    pub last_readings: Vec<Option<IndicatorValue>>,
    /// Most recent tick observed, if any.
    pub last_tick: Option<Tick>,
    /// First price observed this session — anchor for the
    /// since-start percentage shown in the masthead.
    pub session_start_price: Option<f64>,
    /// Total number of ticks ingested since process start.
    pub tick_count: u64,
    /// Most recent WebSocket lifecycle event observed.
    pub status: WsStatus,
}

impl AppState {
    /// Build state from a validated [`Config`].
    ///
    /// Returns [`crate::error::CryptoTuiError`] if any indicator
    /// constructor rejects the configured periods (should not happen
    /// after [`Config::validate`] but we propagate cleanly anyway).
    pub fn new(config: Config) -> Result<Self> {
        config.validate()?;
        let rsi = Rsi::new(config.indicators.rsi_period)?;
        let bollinger = Bollinger::new(
            config.indicators.bollinger_period,
            config.indicators.bollinger_k,
        )?;
        let indicators: Vec<Box<dyn Indicator>> = vec![Box::new(rsi), Box::new(bollinger)];
        let n = indicators.len();
        let cap = config.history_capacity;
        Ok(Self {
            config,
            history: VecDeque::with_capacity(cap),
            bollinger_history: VecDeque::with_capacity(cap),
            indicators,
            last_readings: vec![None; n],
            last_tick: None,
            session_start_price: None,
            tick_count: 0,
            status: WsStatus::Connecting,
        })
    }

    /// Convenience: number of registered indicators.
    pub fn indicator_count(&self) -> usize {
        self.indicators.len()
    }

    /// Apply a tick: extend the price + Bollinger histories, advance
    /// every indicator, refresh `last_readings` (only on a non-`None`
    /// emit), and store `last_tick`. O(N) where N is the number of
    /// indicators.
    pub fn ingest_tick(&mut self, tick: Tick) {
        self.history.push_back((tick.timestamp_ms, tick.price));
        while self.history.len() > self.config.history_capacity {
            self.history.pop_front();
        }

        let mut bb_this_tick: Option<Bands> = None;
        for (i, ind) in self.indicators.iter_mut().enumerate() {
            let result = ind.update(tick.price);
            if let Some(v) = result {
                self.last_readings[i] = Some(v);
            }
            if i == BOLLINGER_INDEX {
                if let Some(IndicatorValue::Bands(b)) = result {
                    bb_this_tick = Some(b);
                }
            }
        }
        self.bollinger_history.push_back(bb_this_tick);
        while self.bollinger_history.len() > self.config.history_capacity {
            self.bollinger_history.pop_front();
        }

        if self.session_start_price.is_none() {
            self.session_start_price = Some(tick.price);
        }
        self.tick_count = self.tick_count.saturating_add(1);
        self.last_tick = Some(tick);
    }

    /// Record a transport-layer status update.
    pub fn ingest_status(&mut self, status: WsStatus) {
        self.status = status;
    }

    /// Percentage change between the most recent tick and the first
    /// price observed this session. `None` until two ticks have arrived.
    pub fn session_change_pct(&self) -> Option<f64> {
        let start = self.session_start_price?;
        let last = self.last_tick.as_ref()?.price;
        if start == 0.0 {
            return None;
        }
        Some((last - start) / start * 100.0)
    }

    /// Latest RSI reading, if past warm-up.
    pub fn latest_rsi(&self) -> Option<f64> {
        match self.last_readings.get(RSI_INDEX).copied().flatten()? {
            IndicatorValue::Single(v) => Some(v),
            IndicatorValue::Bands(_) => None,
        }
    }

    /// Latest Bollinger reading, if past warm-up.
    pub fn latest_bollinger(&self) -> Option<Bands> {
        match self.last_readings.get(BOLLINGER_INDEX).copied().flatten()? {
            IndicatorValue::Bands(b) => Some(b),
            IndicatorValue::Single(_) => None,
        }
    }

    /// Format `last_readings[i]` as a short label like
    /// `"rsi=68.03"` or `"bollinger=warm"`. Convenience used by tests
    /// and any future text-only diagnostic surface.
    pub fn format_indicator(&self, i: usize) -> String {
        let name = self.indicators.get(i).map(|ind| ind.name()).unwrap_or("?");
        match self.last_readings.get(i).copied().flatten() {
            None => format!("{name}=warm"),
            Some(IndicatorValue::Single(v)) => format!("{name}={v:.4}"),
            Some(IndicatorValue::Bands(b)) => {
                format!("{name}=[{:.4}, {:.4}, {:.4}]", b.lower, b.middle, b.upper)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tick(price: f64, ts: u64) -> Tick {
        Tick {
            symbol: "btcusdt".into(),
            price,
            quantity: 1.0,
            timestamp_ms: ts,
            is_buyer_maker: false,
        }
    }

    #[test]
    fn new_uses_config_indicator_periods() {
        let cfg = Config {
            indicators: crate::config::IndicatorsConfig {
                rsi_period: 5,
                bollinger_period: 8,
                ..Default::default()
            },
            ..Config::default()
        };
        let app = AppState::new(cfg).unwrap();
        assert_eq!(app.indicator_count(), 2);
        assert_eq!(app.history.len(), 0);
        assert_eq!(app.bollinger_history.len(), 0);
        assert_eq!(app.last_readings, vec![None, None]);
        assert_eq!(app.tick_count, 0);
        assert!(app.session_start_price.is_none());
    }

    #[test]
    fn ingest_tick_updates_history_and_last_tick() {
        let mut app = AppState::new(Config::default()).unwrap();
        app.ingest_tick(sample_tick(100.0, 10));
        app.ingest_tick(sample_tick(101.0, 11));
        assert_eq!(app.history.len(), 2);
        assert_eq!(app.history[0], (10, 100.0));
        assert_eq!(app.history[1], (11, 101.0));
        assert_eq!(app.last_tick.as_ref().unwrap().price, 101.0);
        assert_eq!(app.tick_count, 2);
        assert_eq!(app.session_start_price, Some(100.0));
    }

    #[test]
    fn bollinger_history_aligned_with_price_history() {
        let mut app = AppState::new(Config::default()).unwrap();
        for i in 0..50 {
            app.ingest_tick(sample_tick(100.0 + i as f64, i as u64));
            assert_eq!(
                app.history.len(),
                app.bollinger_history.len(),
                "histories must stay the same length tick by tick"
            );
        }
    }

    #[test]
    fn bollinger_history_holds_none_during_warmup_then_some() {
        let cfg = Config {
            indicators: crate::config::IndicatorsConfig {
                bollinger_period: 5,
                ..Default::default()
            },
            ..Config::default()
        };
        let mut app = AppState::new(cfg).unwrap();
        for i in 0..4 {
            app.ingest_tick(sample_tick(100.0 + i as f64, i as u64));
        }
        assert!(app.bollinger_history.iter().all(|b| b.is_none()));
        // Fifth tick fills the window — Bollinger emits its first reading.
        app.ingest_tick(sample_tick(105.0, 4));
        assert!(app.bollinger_history.back().unwrap().is_some());
    }

    #[test]
    fn history_is_capped_at_configured_capacity() {
        let cfg = Config {
            history_capacity: 3,
            ..Config::default()
        };
        let mut app = AppState::new(cfg).unwrap();
        for i in 0..10 {
            app.ingest_tick(sample_tick(i as f64, i as u64));
        }
        assert_eq!(app.history.len(), 3);
        assert_eq!(app.bollinger_history.len(), 3);
        let prices: Vec<f64> = app.history.iter().map(|(_, p)| *p).collect();
        assert_eq!(prices, vec![7.0, 8.0, 9.0]);
    }

    #[test]
    fn session_change_pct_tracks_first_price() {
        let mut app = AppState::new(Config::default()).unwrap();
        assert!(app.session_change_pct().is_none());
        app.ingest_tick(sample_tick(100.0, 1));
        // Single tick: change is 0%, but session_start == last so it's defined.
        assert_eq!(app.session_change_pct(), Some(0.0));
        app.ingest_tick(sample_tick(110.0, 2));
        let pct = app.session_change_pct().unwrap();
        assert!((pct - 10.0).abs() < 1e-9, "expected 10%, got {pct}");
    }

    #[test]
    fn latest_rsi_and_bollinger_helpers() {
        let cfg = Config {
            indicators: crate::config::IndicatorsConfig {
                rsi_period: 3,
                bollinger_period: 3,
                ..Default::default()
            },
            ..Config::default()
        };
        let mut app = AppState::new(cfg).unwrap();
        for (i, p) in [10.0, 11.0, 12.0, 13.0, 14.0].iter().enumerate() {
            app.ingest_tick(sample_tick(*p, i as u64));
        }
        assert!(app.latest_rsi().is_some());
        assert!(app.latest_bollinger().is_some());
    }

    #[test]
    fn last_readings_persist_after_first_value() {
        let cfg = Config {
            indicators: crate::config::IndicatorsConfig {
                rsi_period: 3,
                bollinger_period: 3,
                ..Default::default()
            },
            ..Config::default()
        };
        let mut app = AppState::new(cfg).unwrap();
        // RSI warm-up = period + 1 ticks.
        for (i, p) in [10.0, 11.0, 12.0, 13.0, 14.0].iter().enumerate() {
            app.ingest_tick(sample_tick(*p, i as u64));
        }
        assert!(app.last_readings.iter().all(|r| r.is_some()));
        // A sixth tick must not erase prior readings even if the
        // indicator returns the same kind of value.
        app.ingest_tick(sample_tick(13.5, 5));
        assert!(app.last_readings.iter().all(|r| r.is_some()));
    }

    #[test]
    fn format_indicator_handles_warm_single_and_bands() {
        let mut app = AppState::new(Config::default()).unwrap();
        // RSI is index 0 (Single), Bollinger is index 1 (Bands).
        assert_eq!(app.format_indicator(0), "rsi=warm");
        assert_eq!(app.format_indicator(1), "bollinger=warm");
        app.last_readings[0] = Some(IndicatorValue::Single(50.0));
        assert_eq!(app.format_indicator(0), "rsi=50.0000");
        app.last_readings[1] = Some(IndicatorValue::Bands(crate::indicators::Bands {
            upper: 110.0,
            middle: 100.0,
            lower: 90.0,
        }));
        assert_eq!(
            app.format_indicator(1),
            "bollinger=[90.0000, 100.0000, 110.0000]"
        );
    }

    #[test]
    fn ingest_status_overwrites_previous() {
        let mut app = AppState::new(Config::default()).unwrap();
        assert_eq!(app.status, WsStatus::Connecting);
        app.ingest_status(WsStatus::Connected);
        assert_eq!(app.status, WsStatus::Connected);
        app.ingest_status(WsStatus::Reconnecting {
            after_ms: 250,
            attempt: 1,
        });
        assert!(matches!(app.status, WsStatus::Reconnecting { .. }));
    }
}
