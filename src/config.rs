//! Application configuration: file-based settings plus environment-based
//! secrets.
//!
//! User-facing settings (which symbol to stream, indicator periods, how
//! many ticks of history to keep) live in a TOML file at
//! `~/.config/cryptotui/config.toml`. Missing file or missing fields
//! fall back to [`Config::default`] — running without a config is the
//! happy path.
//!
//! Secrets (Binance API keys) are loaded separately via [`load_dotenv`]
//! from a `.env` file in the working directory. The keys are *never*
//! embedded in [`Config`] and never appear in error messages.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CryptoTuiError, Result};
use crate::indicators::{bollinger, rsi};

/// Default symbol streamed when nothing is configured.
pub const DEFAULT_SYMBOL: &str = "btcusdt";
/// Default number of recent ticks the [`crate::app::AppState`] keeps for
/// rendering the price chart.
pub const DEFAULT_HISTORY_CAPACITY: usize = 200;

/// Top-level application configuration.
///
/// Construct via [`Config::default`] for sensible defaults, or
/// [`Config::load`] to read from disk (returns the default on
/// [`std::io::ErrorKind::NotFound`]).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Lowercase trading symbol streamed from Binance, e.g. `"btcusdt"`.
    #[serde(default = "default_symbol")]
    pub symbol: String,

    /// Number of most-recent ticks to retain in memory for chart rendering.
    #[serde(default = "default_history_capacity")]
    pub history_capacity: usize,

    /// Indicator settings.
    #[serde(default)]
    pub indicators: IndicatorsConfig,
}

/// Indicator-specific tuning that sits inside [`Config`].
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IndicatorsConfig {
    /// Wilder period for RSI (warm-up = `period + 1` ticks).
    #[serde(default = "default_rsi_period")]
    pub rsi_period: usize,

    /// Sliding-window size for Bollinger Bands.
    #[serde(default = "default_bollinger_period")]
    pub bollinger_period: usize,

    /// Standard-deviation multiplier for Bollinger Bands.
    #[serde(default = "default_bollinger_k")]
    pub bollinger_k: f64,
}

fn default_symbol() -> String {
    DEFAULT_SYMBOL.to_string()
}
fn default_history_capacity() -> usize {
    DEFAULT_HISTORY_CAPACITY
}
fn default_rsi_period() -> usize {
    rsi::DEFAULT_PERIOD
}
fn default_bollinger_period() -> usize {
    bollinger::DEFAULT_PERIOD
}
fn default_bollinger_k() -> f64 {
    bollinger::DEFAULT_K
}

impl Default for Config {
    fn default() -> Self {
        Self {
            symbol: default_symbol(),
            history_capacity: default_history_capacity(),
            indicators: IndicatorsConfig::default(),
        }
    }
}

impl Default for IndicatorsConfig {
    fn default() -> Self {
        Self {
            rsi_period: default_rsi_period(),
            bollinger_period: default_bollinger_period(),
            bollinger_k: default_bollinger_k(),
        }
    }
}

impl Config {
    /// Read configuration from `path`.
    ///
    /// A missing file is *not* an error: the function returns
    /// [`Config::default`]. Anything else (permission denied, malformed
    /// TOML, unknown fields) bubbles up as [`CryptoTuiError::Config`].
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(e) => {
                return Err(CryptoTuiError::Config(format!(
                    "could not read {}: {e}",
                    path.display()
                )));
            }
        };
        let cfg: Config = toml::from_str(&bytes).map_err(|e| {
            CryptoTuiError::Config(format!("malformed TOML in {}: {e}", path.display()))
        })?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Return the conventional config path: `~/.config/cryptotui/config.toml`.
    ///
    /// Returns [`CryptoTuiError::Config`] if the OS cannot supply a home
    /// directory (rare; happens on broken Windows profiles).
    pub fn default_path() -> Result<PathBuf> {
        let base = dirs::config_dir().ok_or_else(|| {
            CryptoTuiError::Config("could not locate user config directory".into())
        })?;
        Ok(base.join("cryptotui").join("config.toml"))
    }

    /// Validate field invariants (non-zero periods, finite multiplier,
    /// well-formed symbol). Called by [`Config::load`] but exposed for
    /// callers that build a `Config` programmatically.
    pub fn validate(&self) -> Result<()> {
        if !is_valid_symbol(&self.symbol) {
            return Err(CryptoTuiError::Config(format!(
                "symbol {:?} must be lowercase alphanumeric (e.g. \"btcusdt\")",
                self.symbol
            )));
        }
        if self.history_capacity == 0 {
            return Err(CryptoTuiError::Config(
                "history_capacity must be > 0".into(),
            ));
        }
        if self.indicators.rsi_period == 0 {
            return Err(CryptoTuiError::Config(
                "indicators.rsi_period must be > 0".into(),
            ));
        }
        if self.indicators.bollinger_period == 0 {
            return Err(CryptoTuiError::Config(
                "indicators.bollinger_period must be > 0".into(),
            ));
        }
        if !self.indicators.bollinger_k.is_finite() {
            return Err(CryptoTuiError::Config(
                "indicators.bollinger_k must be finite".into(),
            ));
        }
        Ok(())
    }
}

/// Whether a string is a syntactically valid Binance symbol: non-empty,
/// 1–20 characters, `[a-z0-9]+`. We refuse uppercase rather than
/// silently lowercasing it so the user sees what they typed in errors.
pub fn is_valid_symbol(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 20
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// Load secrets from a `.env` file in the current directory if one exists.
///
/// Missing `.env` is silently ignored — the user may not have set up
/// secrets yet, and the streaming WebSocket endpoints we use in Phase 1
/// do not require authentication. Any parse error is surfaced as
/// [`CryptoTuiError::Config`] without including the file contents.
pub fn load_dotenv() -> Result<()> {
    match dotenvy::dotenv() {
        Ok(_) => Ok(()),
        Err(e) if e.not_found() => Ok(()),
        Err(e) => Err(CryptoTuiError::Config(format!(".env: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        Config::default()
            .validate()
            .expect("defaults must validate");
    }

    #[test]
    fn empty_toml_uses_defaults() {
        let parsed: Config = toml::from_str("").expect("empty is valid toml");
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let parsed: Config = toml::from_str(r#"symbol = "ethusdt""#).unwrap();
        assert_eq!(parsed.symbol, "ethusdt");
        assert_eq!(parsed.history_capacity, DEFAULT_HISTORY_CAPACITY);
        assert_eq!(parsed.indicators, IndicatorsConfig::default());
    }

    #[test]
    fn unknown_field_is_rejected() {
        let res: std::result::Result<Config, _> = toml::from_str("nonsense = 1");
        assert!(res.is_err(), "unknown_fields = deny should reject");
    }

    #[test]
    fn invalid_symbol_caught_by_validate() {
        for bad in ["BTCUSDT", "btc/usdt", "", "btc usdt"] {
            let cfg = Config {
                symbol: bad.into(),
                ..Config::default()
            };
            assert!(cfg.validate().is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn zero_period_caught_by_validate() {
        let cfg = Config {
            indicators: IndicatorsConfig {
                rsi_period: 0,
                ..IndicatorsConfig::default()
            },
            ..Config::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn missing_file_returns_default() {
        let path = std::env::temp_dir().join("cryptotui-nonexistent-config-xyz.toml");
        let _ = std::fs::remove_file(&path);
        let cfg = Config::load(&path).expect("missing file should fall back to default");
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn malformed_toml_surfaces_as_config_error() {
        let dir = std::env::temp_dir();
        let path = dir.join("cryptotui-malformed-test.toml");
        std::fs::write(&path, "this = is = not = toml").unwrap();
        let err = Config::load(&path).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("malformed TOML"), "got: {msg}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn symbol_validator_accepts_typical_cases() {
        assert!(is_valid_symbol("btcusdt"));
        assert!(is_valid_symbol("ethbtc"));
        assert!(is_valid_symbol("1000shibusdt"));
    }

    #[test]
    fn symbol_validator_rejects_garbage() {
        assert!(!is_valid_symbol(""));
        assert!(!is_valid_symbol("BTC"));
        assert!(!is_valid_symbol("btc usdt"));
        assert!(!is_valid_symbol("btc-usdt"));
        assert!(!is_valid_symbol(&"a".repeat(21)));
    }
}
