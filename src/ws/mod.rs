//! WebSocket transport layer for live exchange data.
//!
//! Today this exposes a single Binance trade-stream client
//! ([`binance`]). The shared shape — [`Tick`] for per-trade events and
//! [`WsStatus`] for connection lifecycle — is defined here so future
//! exchanges can plug in behind the same channel-based interface.

pub mod binance;

/// One executed trade observed on a streaming exchange feed.
///
/// Prices and quantities arrive as JSON strings from Binance to preserve
/// arbitrary precision; this crate parses them into `f64` because every
/// downstream consumer (indicators, chart) is float-based. Symbols are
/// kept lowercase to match Binance's stream naming convention.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    /// Lowercase exchange symbol, e.g. `"btcusdt"`.
    pub symbol: String,
    /// Trade price, parsed from the exchange's string-encoded number.
    pub price: f64,
    /// Trade quantity (base asset), parsed from the string-encoded number.
    pub quantity: f64,
    /// Exchange-reported trade time in Unix milliseconds.
    pub timestamp_ms: u64,
    /// Whether the buyer was the market maker. Useful for inferring
    /// taker direction (false ⇒ buyer was taker ⇒ aggressor up).
    pub is_buyer_maker: bool,
}

/// Lifecycle events reported by a WebSocket worker task.
///
/// Workers send these on a separate channel from price ticks so the UI
/// can render connection state without slowing the tick path. Workers
/// are expected to drive themselves through the lifecycle continuously
/// (no terminal `Disconnected` state — they always retry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsStatus {
    /// A connection attempt is in flight (handshake in progress).
    Connecting,
    /// The socket is open and forwarding ticks.
    Connected,
    /// The previous connection failed; the worker is sleeping
    /// `after_ms` milliseconds before retry attempt `attempt` (1-based).
    Reconnecting {
        /// Backoff duration before the next attempt, in milliseconds.
        after_ms: u64,
        /// 1-based count of retry attempts since the last successful
        /// connection (capped at `u32::MAX`).
        attempt: u32,
    },
}
