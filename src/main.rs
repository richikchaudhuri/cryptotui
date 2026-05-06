//! cryptotui binary entry point — Phase 1c streaming pipeline.
//!
//! `cargo run` resolves configuration (TOML at `~/.config/cryptotui/config.toml`,
//! falling back to defaults if absent), loads any `.env` secrets, and
//! drives [`cryptotui::app::run_print_pipeline`] which streams trades
//! from Binance and prints one summary line per tick to stdout. The
//! ratatui dashboard takes over here in Phase 1d.

use anyhow::{Context, Result};

use cryptotui::app::run_print_pipeline;
use cryptotui::config::{load_dotenv, Config};

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv().context("loading .env")?;

    let cfg_path = Config::default_path().context("locating config path")?;
    let config = Config::load(&cfg_path)
        .with_context(|| format!("loading config from {}", cfg_path.display()))?;

    eprintln!(
        "[cryptotui] streaming {symbol} (RSI period {rsi}, Bollinger {bp}/{bk})",
        symbol = config.symbol,
        rsi = config.indicators.rsi_period,
        bp = config.indicators.bollinger_period,
        bk = config.indicators.bollinger_k,
    );

    run_print_pipeline(config).await?;
    Ok(())
}
