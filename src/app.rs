//! Application state and the tokio task that drives it.
//!
//! [`AppState`] owns the live data the rest of the binary cares about:
//! a bounded ring of recent (timestamp, price) samples for the chart,
//! the indicator instances, their most recent emitted readings, and the
//! current WebSocket lifecycle. It is renderer-agnostic — Phase 1c
//! prints a one-line summary per tick to stdout; Phase 1d will hand
//! the same state to the ratatui dashboard.

use std::collections::VecDeque;

use crate::config::Config;
use crate::error::Result;
use crate::indicators::{Bollinger, Indicator, IndicatorValue, Rsi};
use crate::ws::binance::{spawn_trade_stream, BinanceConfig};
use crate::ws::{Tick, WsStatus};

/// Live application state.
///
/// A bounded `VecDeque` retains the most recent ticks for the chart;
/// older samples are evicted from the front as new ones arrive at the
/// back. Indicator instances live in `indicators`; the latest non-`None`
/// reading per indicator is cached in `last_readings` so the renderer
/// can show a value even on warm-up ticks where this update returned
/// `None`.
pub struct AppState {
    /// Resolved configuration the app was started with.
    pub config: Config,
    /// `(timestamp_ms, price)` samples in arrival order.
    /// The tail is the most recent; capacity is `config.history_capacity`.
    pub history: VecDeque<(u64, f64)>,
    /// Streaming indicators, in display order.
    pub indicators: Vec<Box<dyn Indicator>>,
    /// Latest non-`None` reading per indicator, parallel to
    /// [`Self::indicators`].
    pub last_readings: Vec<Option<IndicatorValue>>,
    /// Most recent tick observed, if any.
    pub last_tick: Option<Tick>,
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
            indicators,
            last_readings: vec![None; n],
            last_tick: None,
            status: WsStatus::Connecting,
        })
    }

    /// Convenience: number of registered indicators.
    pub fn indicator_count(&self) -> usize {
        self.indicators.len()
    }

    /// Apply a tick: extend history, advance every indicator, refresh
    /// `last_readings` (only on a non-`None` emit), and store
    /// `last_tick`. O(N) where N is the number of indicators.
    pub fn ingest_tick(&mut self, tick: Tick) {
        self.history.push_back((tick.timestamp_ms, tick.price));
        while self.history.len() > self.config.history_capacity {
            self.history.pop_front();
        }
        for (i, ind) in self.indicators.iter_mut().enumerate() {
            if let Some(v) = ind.update(tick.price) {
                self.last_readings[i] = Some(v);
            }
        }
        self.last_tick = Some(tick);
    }

    /// Record a transport-layer status update.
    pub fn ingest_status(&mut self, status: WsStatus) {
        self.status = status;
    }

    /// Format `last_readings[i]` as a short label like
    /// `"rsi=68.03"` or `"bollinger=warm"`. Used by the Phase 1c
    /// stdout demo and as a fallback in the Phase 1d indicator panel
    /// for very narrow terminals.
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

/// Run the Phase 1c streaming pipeline: connect to Binance, feed each
/// tick into [`AppState`], and print a one-line summary per event to
/// stdout. Returns when the process receives Ctrl+C or the WS worker
/// permanently exits.
///
/// This is the "no TUI yet" entry point listed in Phase 1c. Phase 1d
/// replaces it with [`crate::tui`]-driven rendering while reusing the
/// same [`AppState`] and channel topology.
pub async fn run_print_pipeline(config: Config) -> Result<()> {
    let mut app = AppState::new(config)?;
    let bin_cfg = BinanceConfig::new(&app.config.symbol)?;
    let (handle, mut tick_rx, mut status_rx) = spawn_trade_stream(bin_cfg);

    let mut interrupt = Box::pin(tokio::signal::ctrl_c());

    loop {
        tokio::select! {
            biased;
            _ = &mut interrupt => {
                eprintln!("\n[cryptotui] interrupt received, shutting down");
                break;
            }
            maybe_tick = tick_rx.recv() => match maybe_tick {
                Some(tick) => {
                    app.ingest_tick(tick);
                    print_tick_line(&app);
                }
                None => break,
            },
            maybe_status = status_rx.recv() => match maybe_status {
                Some(status) => {
                    app.ingest_status(status.clone());
                    print_status_line(&status);
                }
                None => break,
            },
        }
    }

    drop(tick_rx);
    drop(status_rx);
    let _ = handle.await;
    Ok(())
}

fn print_tick_line(app: &AppState) {
    if let Some(tick) = &app.last_tick {
        let mut line = format!(
            "{ts}  {sym} {price:.4}  qty={qty:.6}",
            ts = tick.timestamp_ms,
            sym = tick.symbol,
            price = tick.price,
            qty = tick.quantity,
        );
        for i in 0..app.indicator_count() {
            line.push_str("  ");
            line.push_str(&app.format_indicator(i));
        }
        println!("{line}");
    }
}

fn print_status_line(status: &WsStatus) {
    match status {
        WsStatus::Connecting => eprintln!("[cryptotui] connecting…"),
        WsStatus::Connected => eprintln!("[cryptotui] connected"),
        WsStatus::Reconnecting { after_ms, attempt } => {
            eprintln!("[cryptotui] reconnecting (attempt {attempt}) in {after_ms} ms");
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
        assert_eq!(app.last_readings, vec![None, None]);
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
        let prices: Vec<f64> = app.history.iter().map(|(_, p)| *p).collect();
        assert_eq!(prices, vec![7.0, 8.0, 9.0]);
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
