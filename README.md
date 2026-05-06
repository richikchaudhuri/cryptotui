# cryptotui

A terminal dashboard for crypto prices. Streams trades from Binance over
WebSocket, runs them through hand-rolled streaming indicators (RSI,
Bollinger Bands), and will eventually draw a live ratatui chart.

I built this to actually learn async Rust instead of just watching
videos about it, and because alt-tabbing to TradingView every time I
want to glance at BTC got old.

## Status

- [x] Ring buffer with circular indexing
- [x] Wilder's RSI (with all the warm-up edge cases handled)
- [x] Bollinger Bands
- [x] `Indicator` trait + registry — adding a new indicator is one file
- [x] Binance WebSocket pipeline with exponential-backoff reconnects
- [ ] ratatui dashboard (next)

## Run it

```sh
cargo run --release
```

By default it streams `btcusdt` trades from Binance and prints a line
per tick with the live indicator readings. The streaming endpoints are
public, so no API keys needed.

Want a different pair or different periods? Drop a TOML file at
`~/.config/cryptotui/config.toml`:

```toml
symbol = "ethusdt"
history_capacity = 500

[indicators]
rsi_period = 14
bollinger_period = 20
bollinger_k = 2.0
```

Anything you leave out falls back to the default. If you ever do need
authenticated Binance endpoints, copy `.env.example` to `.env` and fill
in your keys — the `.env` is gitignored, secrets never make it into a
commit or an error message.

## Why the design looks like it does

**Ring buffers, not DataFrames.** Streaming indicators are O(1) per
tick. Building a DataFrame on every trade would burn a lot of cycles
to compute the same thing. Polars is the right tool for backtesting,
not the hot path — that's a Phase 2 thing.

**Wilder's smoothing for RSI.** Two warm-up buffers fill during the
first N changes, then recursive smoothing takes over. Flat prices give
50, all gains give 100, all losses give 0 — handled explicitly so you
never get a `0/0` `NaN` in your face.

**The socket reconnects itself.** If Binance drops the connection the
worker backs off (250 ms doubling up to 30 s) and tries again. The rest
of the app doesn't notice except for a status update on the side
channel.

**No `.unwrap()` in library code.** Enforced via
`#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]`.
Tests stay readable.

## Layout

```
src/
  indicators/   ring buffer, RSI, Bollinger, the Indicator trait
  ws/           Binance WebSocket worker, parsed Tick events
  config.rs     TOML config loader, .env handling
  app.rs        state, tick ingestion, the streaming pipeline
  main.rs       entry point
tests/          integration tests + on-disk Binance fixtures
```

## Tests

```sh
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

The live WebSocket isn't part of `cargo test` (it would be flaky and
need network). The parser is covered by fixture messages in
`tests/fixtures/`; the live path is verified manually with `cargo run`.

## License

MIT — see [LICENSE](LICENSE).

By [Richik Chaudhuri](https://github.com/richikchaudhuri).
