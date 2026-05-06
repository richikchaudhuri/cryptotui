//! Binance trade-stream client: connects to
//! `wss://stream.binance.com:9443/ws/{symbol}@trade`, parses each
//! incoming `trade` event, and forwards [`Tick`]s through an mpsc
//! channel.
//!
//! The worker is resilient: on disconnect or handshake failure it sleeps
//! with exponential backoff (capped) and reconnects. It exits cleanly
//! when its `tick_tx` receiver is dropped — no explicit cancellation
//! token is needed.
//!
//! ## Why parse JSON manually?
//!
//! Binance encodes price and quantity as JSON *strings* to preserve
//! arbitrary precision. We deserialize the message into intermediate
//! string fields and only then parse to `f64`, so a malformed price
//! surfaces as [`CryptoTuiError::MalformedMessage`] instead of a silent
//! type error.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message;

use crate::config::is_valid_symbol;
use crate::error::{CryptoTuiError, Result};

use super::{Tick, WsStatus};

/// Binance combined-stream host. The per-symbol path is appended at
/// connect time; the host is parameterised so tests/proxies can swap it.
pub const BINANCE_WS_HOST: &str = "wss://stream.binance.com:9443/ws";

/// Default initial backoff after a failed connection attempt.
pub const DEFAULT_INITIAL_BACKOFF_MS: u64 = 250;

/// Default cap on the exponential backoff. Binance encourages clients
/// to reconnect within ~60s; 30s strikes a balance between responsiveness
/// and avoiding aggressive reconnection during a wide outage.
pub const DEFAULT_MAX_BACKOFF_MS: u64 = 30_000;

/// Channel buffer size between the WS worker and the application loop.
/// Big enough to absorb a brief consumer stall without dropping ticks
/// during a bursty market open.
pub const DEFAULT_TICK_CHANNEL_CAPACITY: usize = 1024;

/// Buffer size for the status channel. Status events are small and rare
/// (one per lifecycle transition), so a tiny buffer suffices.
pub const DEFAULT_STATUS_CHANNEL_CAPACITY: usize = 16;

/// Configuration for [`spawn_trade_stream`].
#[derive(Debug, Clone)]
pub struct BinanceConfig {
    /// Lowercase symbol; validated against [`is_valid_symbol`].
    pub symbol: String,
    /// Initial sleep before the first reconnect attempt.
    pub initial_backoff_ms: u64,
    /// Upper bound on the exponential backoff.
    pub max_backoff_ms: u64,
    /// Override the WebSocket host. `None` → [`BINANCE_WS_HOST`].
    /// Useful for tests against a local mock server.
    pub host_override: Option<String>,
}

impl BinanceConfig {
    /// Build a config for `symbol` using all other defaults.
    ///
    /// Returns [`CryptoTuiError::Config`] if the symbol does not match
    /// the lowercase-alphanumeric format expected by Binance streams.
    pub fn new(symbol: impl Into<String>) -> Result<Self> {
        let symbol = symbol.into();
        if !is_valid_symbol(&symbol) {
            return Err(CryptoTuiError::Config(format!(
                "symbol {symbol:?} must be lowercase alphanumeric"
            )));
        }
        Ok(Self {
            symbol,
            initial_backoff_ms: DEFAULT_INITIAL_BACKOFF_MS,
            max_backoff_ms: DEFAULT_MAX_BACKOFF_MS,
            host_override: None,
        })
    }
}

/// Spawn the Binance trade-stream worker as a tokio task.
///
/// Returns a [`JoinHandle`] (the caller normally ignores it) plus two
/// receivers: `tick_rx` for parsed [`Tick`]s and `status_rx` for
/// [`WsStatus`] lifecycle events. The worker exits when *both*
/// receivers are dropped (technically: when the next send to either
/// channel fails).
pub fn spawn_trade_stream(
    cfg: BinanceConfig,
) -> (
    JoinHandle<()>,
    mpsc::Receiver<Tick>,
    mpsc::Receiver<WsStatus>,
) {
    let (tick_tx, tick_rx) = mpsc::channel(DEFAULT_TICK_CHANNEL_CAPACITY);
    let (status_tx, status_rx) = mpsc::channel(DEFAULT_STATUS_CHANNEL_CAPACITY);
    let handle = tokio::spawn(run_worker(cfg, tick_tx, status_tx));
    (handle, tick_rx, status_rx)
}

async fn run_worker(
    cfg: BinanceConfig,
    tick_tx: mpsc::Sender<Tick>,
    status_tx: mpsc::Sender<WsStatus>,
) {
    let host = cfg
        .host_override
        .as_deref()
        .unwrap_or(BINANCE_WS_HOST)
        .trim_end_matches('/')
        .to_string();
    let url = format!("{host}/{}@trade", cfg.symbol);

    let mut backoff_ms = cfg.initial_backoff_ms.max(1);
    let mut attempt: u32 = 0;

    loop {
        if status_tx.send(WsStatus::Connecting).await.is_err() && tick_tx.is_closed() {
            return;
        }

        let connect_result = tokio_tungstenite::connect_async(&url).await;
        match connect_result {
            Ok((mut ws, _resp)) => {
                attempt = 0;
                backoff_ms = cfg.initial_backoff_ms.max(1);
                if status_tx.send(WsStatus::Connected).await.is_err() && tick_tx.is_closed() {
                    return;
                }

                loop {
                    tokio::select! {
                        biased;
                        _ = tick_tx.closed() => return,
                        msg = ws.next() => match msg {
                            Some(Ok(Message::Text(text))) => {
                                match parse_trade_message(&text) {
                                    Ok(tick) => {
                                        if tick_tx.send(tick).await.is_err() {
                                            return;
                                        }
                                    }
                                    // Malformed messages are ignored
                                    // silently — Binance occasionally
                                    // sends control frames or non-trade
                                    // events on this stream.
                                    Err(_) => continue,
                                }
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                if ws.send(Message::Pong(payload)).await.is_err() {
                                    break;
                                }
                            }
                            Some(Ok(Message::Close(_))) => break,
                            Some(Ok(_)) => continue,
                            Some(Err(_)) | None => break,
                        }
                    }
                }
            }
            Err(_) => {
                // Connection or handshake failed; fall through to backoff.
            }
        }

        attempt = attempt.saturating_add(1);
        let event = WsStatus::Reconnecting {
            after_ms: backoff_ms,
            attempt,
        };
        if status_tx.send(event).await.is_err() && tick_tx.is_closed() {
            return;
        }

        tokio::select! {
            _ = tick_tx.closed() => return,
            _ = sleep(Duration::from_millis(backoff_ms)) => {}
        }

        backoff_ms = backoff_ms.saturating_mul(2).min(cfg.max_backoff_ms);
    }
}

/// Wire-format Binance trade-event message.
///
/// Field names match the exchange's single-character JSON keys. We only
/// pull the fields we need; serde ignores the rest.
#[derive(Debug, Deserialize)]
struct WireTrade {
    /// Event type. Must equal `"trade"` for this stream.
    e: String,
    /// Symbol (uppercase on the wire, lowercased before construction).
    s: String,
    /// Price as a string (preserves precision).
    p: String,
    /// Quantity as a string.
    q: String,
    /// Trade time in Unix milliseconds.
    #[serde(rename = "T")]
    trade_time: u64,
    /// Whether the buyer was the maker.
    m: bool,
}

/// Parse a single text frame from the trade stream into a [`Tick`].
///
/// Validates the event type, parses price and quantity from string,
/// and lowercases the symbol. Any deviation surfaces as
/// [`CryptoTuiError::MalformedMessage`].
pub fn parse_trade_message(text: &str) -> Result<Tick> {
    let raw: WireTrade = serde_json::from_str(text)
        .map_err(|e| CryptoTuiError::MalformedMessage(format!("json: {e}")))?;
    if raw.e != "trade" {
        return Err(CryptoTuiError::MalformedMessage(format!(
            "expected event type \"trade\", got {:?}",
            raw.e
        )));
    }
    let price: f64 = raw
        .p
        .parse()
        .map_err(|e| CryptoTuiError::MalformedMessage(format!("price {:?}: {e}", raw.p)))?;
    let quantity: f64 = raw
        .q
        .parse()
        .map_err(|e| CryptoTuiError::MalformedMessage(format!("quantity {:?}: {e}", raw.q)))?;
    if !price.is_finite() || price <= 0.0 {
        return Err(CryptoTuiError::MalformedMessage(format!(
            "non-positive price: {price}"
        )));
    }
    if !quantity.is_finite() || quantity < 0.0 {
        return Err(CryptoTuiError::MalformedMessage(format!(
            "negative quantity: {quantity}"
        )));
    }
    Ok(Tick {
        symbol: raw.s.to_ascii_lowercase(),
        price,
        quantity,
        timestamp_ms: raw.trade_time,
        is_buyer_maker: raw.m,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TRADE: &str = r#"{
        "e": "trade",
        "E": 1729171200001,
        "s": "BTCUSDT",
        "t": 12345,
        "p": "67250.42",
        "q": "0.00153",
        "T": 1729171200000,
        "m": false,
        "M": true
    }"#;

    #[test]
    fn parses_well_formed_trade() {
        let tick = parse_trade_message(SAMPLE_TRADE).expect("well-formed");
        assert_eq!(tick.symbol, "btcusdt");
        assert!((tick.price - 67250.42).abs() < 1e-9);
        assert!((tick.quantity - 0.00153).abs() < 1e-12);
        assert_eq!(tick.timestamp_ms, 1729171200000);
        assert!(!tick.is_buyer_maker);
    }

    #[test]
    fn rejects_non_trade_event() {
        let msg = r#"{"e":"kline","s":"BTCUSDT","p":"1","q":"1","T":1,"m":true}"#;
        let err = parse_trade_message(msg).unwrap_err();
        assert!(format!("{err}").contains("trade"));
    }

    #[test]
    fn rejects_unparseable_price() {
        let msg = r#"{"e":"trade","s":"BTCUSDT","p":"not-a-number","q":"1","T":1,"m":true}"#;
        assert!(parse_trade_message(msg).is_err());
    }

    #[test]
    fn rejects_zero_price() {
        let msg = r#"{"e":"trade","s":"BTCUSDT","p":"0","q":"1","T":1,"m":true}"#;
        assert!(parse_trade_message(msg).is_err());
    }

    #[test]
    fn rejects_negative_quantity() {
        let msg = r#"{"e":"trade","s":"BTCUSDT","p":"1","q":"-0.5","T":1,"m":true}"#;
        assert!(parse_trade_message(msg).is_err());
    }

    #[test]
    fn rejects_garbage_json() {
        assert!(parse_trade_message("not json").is_err());
        assert!(parse_trade_message("{").is_err());
    }

    #[test]
    fn binance_config_validates_symbol() {
        assert!(BinanceConfig::new("btcusdt").is_ok());
        assert!(BinanceConfig::new("BTCUSDT").is_err());
        assert!(BinanceConfig::new("").is_err());
        assert!(BinanceConfig::new("btc/usdt").is_err());
    }

    #[test]
    fn binance_config_keeps_defaults() {
        let cfg = BinanceConfig::new("ethusdt").unwrap();
        assert_eq!(cfg.symbol, "ethusdt");
        assert_eq!(cfg.initial_backoff_ms, DEFAULT_INITIAL_BACKOFF_MS);
        assert_eq!(cfg.max_backoff_ms, DEFAULT_MAX_BACKOFF_MS);
        assert!(cfg.host_override.is_none());
    }
}
