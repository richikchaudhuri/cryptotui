//! cryptotui binary entry point.
//!
//! Today this is a smoke-test driver: streams a hard-coded price series
//! through the streaming RSI and prints the resulting table, so
//! `cargo run` produces visible end-to-end output without a live
//! exchange connection. The Binance WebSocket pipeline and ratatui
//! dashboard arrive in subsequent commits.

use cryptotui::indicators::Rsi;

fn main() -> anyhow::Result<()> {
    let prices = [
        10.0, 11.0, 10.5, 11.5, 12.0, 11.0, 12.5, 13.0, 12.0, 13.5, 14.0, 13.0, 14.5, 15.0, 14.0,
        15.0, 14.5, 16.0, 17.0, 16.5, 18.0, 17.5, 17.0, 16.0, 16.5,
    ];
    let mut rsi = Rsi::new(14)?;
    println!("streaming RSI demo (period = 14)");
    println!("{:>4}  {:>8}  {:>8}", "tick", "price", "rsi");
    for (i, p) in prices.iter().enumerate() {
        match rsi.update(*p) {
            Some(v) => println!("{:>4}  {:>8.2}  {:>8.2}", i + 1, p, v),
            None => println!("{:>4}  {:>8.2}  {:>8}", i + 1, p, "warm-up"),
        }
    }
    Ok(())
}
