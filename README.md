# cryptotui

A terminal dashboard for crypto prices. Streams trades from Binance over
WebSocket, runs them through hand-rolled streaming indicators (RSI,
Bollinger Bands), and renders a live ratatui chart with a colour
palette stolen from a private banker's office (cream, brushed gold,
oxblood — restrained, opinionated, mine).

## Status

- [x] Ring buffer with circular indexing
- [x] Wilder's RSI (with all the warm-up edge cases handled)
- [x] Bollinger Bands
- [x] `Indicator` trait + registry — adding a new indicator is one file
- [x] Binance WebSocket pipeline with exponential-backoff reconnects
- [x] ratatui dashboard with live chart, RSI gauge, Bollinger panel,
      help overlay

## Run it

```sh
cargo run --release
```

Takes over your terminal, connects to Binance, and shows live
`btcusdt` trades. The streaming endpoints are public, so no API key
needed. Press `q` (or `esc`) to quit.

```
─ CRYPTOTUI · BTCUSDT ──────────────────────── ● live · vol. 01 ─

  $ 67,250.42                       ▲ +0.47 %        1,247 trades

  ╭─────────────────────────────────────────────────────────────╮
  │           [price line + Bollinger upper/mid/lower]          │
  ╰─────────────────────────────────────────────────────────────╯

  ─ RSI · 14 ────────────────────────────────────────────────────
    ▰▰▰▰▰▰▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱  68.03            NEUTRAL

  ─ BOLLINGER · 20 / 2σ ─────────────────────────────────────────
    ⌃  upper     67,300.00
    ·  middle    67,200.00
    ⌄  lower     67,100.00

──────────────── q quit · ? help · i focus ──── © MMXXVI ───────
```

### Keybindings

| Key       | Action                                      |
|-----------|---------------------------------------------|
| `q` / `esc` | quit                                       |
| `?` / `h` | toggle help overlay                          |
| `s`       | switch symbol (BTC, ETH, SOL, ..., PAX Gold) |
| `i`       | cycle indicator focus (both / RSI / Bollinger) |
| `ctrl-c`  | hard interrupt                               |

Inside the symbol picker: `↑/↓` (or `j/k`) to navigate, `enter` to
switch, `esc` to cancel. The presets cover the top crypto pairs plus
two tokenised gold pairs (PAXG, XAUT) — those are the closest thing
Binance offers to a real gold price.

### Configuration

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
in your keys, the `.env` is gitignored, secrets never make it into a
commit or an error message.

## Why the design looks like it does

**Ring buffers, not DataFrames.** Streaming indicators are O(1) per
tick. Building a DataFrame on every trade would burn a lot of cycles
to compute the same thing. Polars is the right tool for backtesting,
not the hot path — that's a Phase 2 thing.

**Wilder's smoothing for RSI.** Two warm-up buffers fill during the
first N changes, then recursive smoothing takes over. Flat prices give
50, all gains give 100, all losses give 0 which is handled explicitly so you
never get a `0/0` `NaN` in your face.

**The socket reconnects itself.** If Binance drops the connection the
worker backs off (250 ms doubling up to 30 s) and tries again. The
dashboard side just sees the masthead status pip change colour.

**The TUI lifecycle is panic-safe.** A panic anywhere in the program
restores cooked mode and leaves the alternate screen before printing
the payload, so you never end up in a wedged terminal.

**No `.unwrap()` in library code.** Enforced via
`#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]`.
Tests stay readable.

## Layout

```
src/
  indicators/   ring buffer, RSI, Bollinger, the Indicator trait
  ws/           Binance WebSocket worker, parsed Tick events
  tui/          theme, dashboard, chart, help overlay
  config.rs     TOML config loader, .env handling
  app.rs        state, tick ingestion, helper readouts
  main.rs       entry point
tests/          integration tests + on-disk Binance fixtures
```

## Tests

```sh
make test       # cargo test
make lint       # cargo clippy -- -D warnings
make fmt        # cargo fmt --check
make run        # cargo run --release
```

The live WebSocket isn't part of `cargo test` (it would be flaky and
need network). The parser is covered by fixture messages in
`tests/fixtures/`; the live path is verified manually with `cargo run`.

## License

MIT - see [LICENSE](LICENSE).

By [Richik Chaudhuri](https://github.com/richikchaudhuri).
