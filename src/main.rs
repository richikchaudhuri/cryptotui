//! cryptotui binary entry point.
//!
//! Today this is a smoke-test driver: streams a hard-coded price series
//! through the default indicator registry and prints each tick's
//! readings via the [`Indicator`] trait, so `cargo run` produces
//! visible end-to-end output without a live exchange connection. The
//! Binance WebSocket pipeline and ratatui dashboard arrive in
//! subsequent commits.
//!
//! [`Indicator`]: cryptotui::indicators::Indicator

use cryptotui::indicators::{default_indicators, IndicatorValue};

fn main() {
    let prices = [
        10.0, 11.0, 10.5, 11.5, 12.0, 11.0, 12.5, 13.0, 12.0, 13.5, 14.0, 13.0, 14.5, 15.0, 14.0,
        15.0, 14.5, 16.0, 17.0, 16.5, 18.0, 17.5, 17.0, 16.0, 16.5,
    ];
    let mut indicators = default_indicators();
    println!("streaming indicator demo (default registry)");
    for (i, p) in prices.iter().enumerate() {
        print!("tick {:>2}  price {:>6.2}", i + 1, p);
        for ind in indicators.iter_mut() {
            match ind.update(*p) {
                None => print!("  {}=warm", ind.name()),
                Some(IndicatorValue::Single(v)) => print!("  {}={:.2}", ind.name(), v),
                Some(IndicatorValue::Bands(b)) => print!(
                    "  {}=[{:.2}, {:.2}, {:.2}]",
                    ind.name(),
                    b.lower,
                    b.middle,
                    b.upper
                ),
            }
        }
        println!();
    }
}
