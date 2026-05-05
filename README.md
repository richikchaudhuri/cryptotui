# cryptotui

A terminal dashboard for live crypto markets, built in Rust.

The pipeline streams ticks from Binance, runs them through
hand-rolled streaming indicators (RSI, Bollinger, …), and renders a
ratatui dashboard. Today the streaming primitives are in place; the
network layer and UI follow.

## Status

- [x] Generic ring buffer
- [x] Wilder's RSI
- [ ] Bollinger Bands
- [ ] `Indicator` trait + registry
- [ ] Binance WebSocket pipeline
- [ ] ratatui dashboard

## Build

```sh
cargo build --release
cargo test
cargo run    # streaming RSI demo over a hard-coded price series
```

## Notes

**Ring buffers, not DataFrames.** Streaming indicators maintain
incremental state with O(1) per-tick updates; a batch-analytics
library would be the wrong primitive for the hot path.

**Wilder's smoothing for RSI.** Two warm-up ring buffers fill during
the first N changes, then recursive smoothing takes over. Edge cases
handled explicitly (flat = 50, all gains = 100, all losses = 0).

**No `.unwrap()` in library code.** Lib-level `deny` lint, gated by
`cfg_attr(not(test))` so tests stay readable.

## License

MIT
