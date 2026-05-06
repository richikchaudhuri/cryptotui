//! Parser tests against on-disk Binance fixture messages.
//!
//! Live WebSocket integration is intentionally out of scope here —
//! `cargo test` must run hermetically. The Phase 1c task lists fixture
//! tests as the deliverable; a real connection is verified manually via
//! `cargo run`.

use std::path::PathBuf;

use cryptotui::app::AppState;
use cryptotui::config::Config;
use cryptotui::ws::binance::parse_trade_message;

fn fixture(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "fixtures", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).expect("could not read fixture")
}

#[test]
fn fixture_trade_parses_into_tick() {
    let body = fixture("binance_trade.json");
    let tick = parse_trade_message(&body).expect("fixture trade parses");
    assert_eq!(tick.symbol, "btcusdt");
    assert!((tick.price - 67250.42).abs() < 1e-9);
    assert_eq!(tick.timestamp_ms, 1729171200000);
    assert!(!tick.is_buyer_maker);
}

#[test]
fn fixture_buyer_maker_trade_preserves_flag() {
    let body = fixture("binance_trade_buyer_maker.json");
    let tick = parse_trade_message(&body).expect("fixture trade parses");
    assert!(tick.is_buyer_maker, "buyer-maker fixture must set the flag");
}

#[test]
fn fixture_kline_event_is_rejected() {
    // A kline payload lacks the top-level `p`, `q`, `T`, `m` fields
    // that `WireTrade` requires, so deserialization itself fails. We
    // only assert that *some* error surfaces — the parser refuses to
    // synthesise a fake tick from a non-trade event.
    let body = fixture("binance_kline_event.json");
    assert!(parse_trade_message(&body).is_err());
}

#[test]
fn ingesting_a_fixture_tick_advances_indicators_through_warmup() {
    let body = fixture("binance_trade.json");
    let tick = parse_trade_message(&body).expect("fixture parses");

    // Use a tiny RSI period so we can warm up within a handful of synthetic ticks.
    let cfg = Config {
        indicators: cryptotui::config::IndicatorsConfig {
            rsi_period: 3,
            bollinger_period: 3,
            ..Default::default()
        },
        ..Config::default()
    };
    let mut app = AppState::new(cfg).expect("app builds");

    // Inject the fixture tick, then a synthetic walk derived from its price.
    let base = tick.price;
    let prices = [base, base + 1.0, base - 0.5, base + 2.0, base + 1.5];
    for (i, p) in prices.iter().enumerate() {
        let mut t = tick.clone();
        t.price = *p;
        t.timestamp_ms = tick.timestamp_ms + i as u64;
        app.ingest_tick(t);
    }

    assert_eq!(app.history.len(), 5);
    assert!(app.last_readings.iter().all(|r| r.is_some()));
    assert!(app.last_tick.is_some());
}
