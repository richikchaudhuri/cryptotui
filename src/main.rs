//! cryptotui binary entry point.
//!
//! Loads `~/.config/cryptotui/config.toml` (falling back to defaults if
//! absent), reads any `.env` for secrets, then hands control to the
//! ratatui dashboard at [`cryptotui::tui::run_tui_pipeline`]. Press
//! `q`, `esc`, or Ctrl-C to exit.

use anyhow::{Context, Result};

use cryptotui::config::{load_dotenv, Config};
use cryptotui::tui::run_tui_pipeline;

#[tokio::main]
async fn main() -> Result<()> {
    // rustls 0.23 dropped the implicit default crypto provider; pick one
    // here once for the whole process. `install_default` returns Err if a
    // provider is already installed (for example, by a test harness), so
    // it's safe to discard.
    let _ = rustls::crypto::ring::default_provider().install_default();

    load_dotenv().context("loading .env")?;

    let cfg_path = Config::default_path().context("locating config path")?;
    let config = Config::load(&cfg_path)
        .with_context(|| format!("loading config from {}", cfg_path.display()))?;

    run_tui_pipeline(config)
        .await
        .context("running tui pipeline")?;
    Ok(())
}
