//! ratatui dashboard for cryptotui.
//!
//! [`run_tui_pipeline`] is the main entry point: it boots the
//! WebSocket worker (Phase 1c), takes over the terminal, then drives a
//! select-loop over ticks, status events, and keyboard input — handing
//! the resulting [`AppState`] to [`dashboard::render`] on every change.
//!
//! The terminal lifecycle is hardened with a panic hook so a crash
//! anywhere in the program still leaves the user with a usable shell.

pub mod chart;
pub mod dashboard;
pub mod help;
pub mod picker;
pub mod theme;

use std::io::{self, Stdout};
use std::panic;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::time::{interval, MissedTickBehavior};

use crate::app::AppState;
use crate::config::Config;
use crate::error::{CryptoTuiError, Result};
use crate::ws::binance::{spawn_trade_stream, BinanceConfig};

use picker::SymbolPicker;

/// Ephemeral UI-only state: which view the user has selected, whether
/// any overlay is open. Kept separate from [`AppState`] so the data
/// layer never has to know about the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UiState {
    /// Whether the floating help overlay is visible.
    pub show_help: bool,
    /// Indicator visibility cycle: 0 = both, 1 = RSI only, 2 = Bollinger only.
    pub indicator_focus: u8,
    /// Symbol-switcher overlay state.
    pub picker: SymbolPicker,
}

impl UiState {
    /// Cycle through `{both, rsi-only, bollinger-only}`.
    pub fn cycle_focus(&mut self) {
        self.indicator_focus = (self.indicator_focus + 1) % 3;
    }
    /// Whether the RSI panel should be drawn given the current focus.
    pub fn show_rsi(&self) -> bool {
        matches!(self.indicator_focus, 0 | 1)
    }
    /// Whether the Bollinger panel should be drawn given the current focus.
    pub fn show_bollinger(&self) -> bool {
        matches!(self.indicator_focus, 0 | 2)
    }
}

/// RAII guard that owns the terminal handle and restores cooked mode
/// on drop (or on panic).
pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    /// Take over the terminal: enable raw mode, switch to the
    /// alternate screen, install a panic hook that calls
    /// [`restore_terminal`] before any payload prints.
    pub fn enter() -> Result<Self> {
        enable_raw_mode().map_err(io_to_tui)?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).map_err(io_to_tui)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(io_to_tui)?;
        install_panic_hook();
        Ok(Self { terminal })
    }

    /// Mutable handle to the underlying terminal.
    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore_terminal();
    }
}

fn io_to_tui(e: io::Error) -> CryptoTuiError {
    CryptoTuiError::Tui(e.to_string())
}

fn install_panic_hook() {
    let original = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        original(info);
    }));
}

fn restore_terminal() -> io::Result<()> {
    execute!(io::stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

/// Hard cap on screen refreshes per second. Indicator math runs at
/// the full WS tick rate; only the redraw is throttled, so high-traffic
/// pairs like BTCUSDT (often 50–200 trades/sec) don't drown the
/// terminal in ANSI escapes and starve the event loop.
const REDRAW_INTERVAL: Duration = Duration::from_millis(50);

/// Run the full Phase 1d pipeline: connect to Binance, take over the
/// terminal, and render the live dashboard until the user quits with
/// `q`, presses Ctrl-C, or the WebSocket worker permanently dies.
///
/// The outer loop owns the symbol session: when the user switches
/// symbols via the picker, the current WebSocket task is torn down,
/// [`AppState`] is rebuilt fresh, and a new task spawns. The terminal
/// guard and event stream are reused across sessions so the screen
/// never blanks during a swap.
pub async fn run_tui_pipeline(config: Config) -> Result<()> {
    let mut guard = TerminalGuard::enter()?;
    let mut events = EventStream::new();
    let mut interrupt = Box::pin(tokio::signal::ctrl_c());
    let mut ui = UiState::default();
    let mut current_config = config;

    let mut redraw_timer = interval(REDRAW_INTERVAL);
    redraw_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

    'session: loop {
        let mut app = AppState::new(current_config.clone())?;
        let bin_cfg = BinanceConfig::new(&app.config.symbol)?;
        let (handle, mut tick_rx, mut status_rx) = spawn_trade_stream(bin_cfg);

        // First paint of the new session before any event arrives.
        guard
            .terminal_mut()
            .draw(|f| dashboard::render(f, &app, &ui))
            .map_err(io_to_tui)?;

        let mut next_symbol: Option<String> = None;
        let mut dirty = false;

        let outcome = loop {
            tokio::select! {
                biased;
                _ = &mut interrupt => break SessionOutcome::Quit,
                ev = events.next() => match ev {
                    Some(Ok(event)) => match handle_event(event, &mut ui, &app) {
                        EventOutcome::Continue => dirty = true,
                        EventOutcome::Quit => break SessionOutcome::Quit,
                        EventOutcome::SwitchSymbol(s) => {
                            next_symbol = Some(s);
                            break SessionOutcome::Switch;
                        }
                        EventOutcome::Ignore => {}
                    },
                    Some(Err(e)) => return Err(CryptoTuiError::Tui(e.to_string())),
                    None => break SessionOutcome::Quit,
                },
                maybe_tick = tick_rx.recv() => match maybe_tick {
                    Some(tick) => {
                        app.ingest_tick(tick);
                        // Drain anything that piled up while we were last
                        // redrawing; one redraw will reflect the whole batch.
                        while let Ok(more) = tick_rx.try_recv() {
                            app.ingest_tick(more);
                        }
                        dirty = true;
                    }
                    None => break SessionOutcome::Quit,
                },
                maybe_status = status_rx.recv() => match maybe_status {
                    Some(status) => { app.ingest_status(status); dirty = true; }
                    None => break SessionOutcome::Quit,
                },
                _ = redraw_timer.tick() => {
                    if dirty {
                        guard
                            .terminal_mut()
                            .draw(|f| dashboard::render(f, &app, &ui))
                            .map_err(io_to_tui)?;
                        dirty = false;
                    }
                }
            }
        };

        // Tear down the current session before either restarting or returning.
        drop(tick_rx);
        drop(status_rx);
        let _ = handle.await;

        match outcome {
            SessionOutcome::Quit => break 'session,
            SessionOutcome::Switch => {
                if let Some(s) = next_symbol {
                    current_config.symbol = s;
                    continue 'session;
                }
                break 'session;
            }
        }
    }

    Ok(())
}

enum SessionOutcome {
    Quit,
    Switch,
}

enum EventOutcome {
    Continue,
    Quit,
    Ignore,
    SwitchSymbol(String),
}

fn handle_event(event: Event, ui: &mut UiState, app: &AppState) -> EventOutcome {
    let key = match event {
        Event::Key(k) if k.kind == KeyEventKind::Press => k,
        Event::Resize(_, _) => return EventOutcome::Continue,
        _ => return EventOutcome::Ignore,
    };

    // Picker has highest priority — when it's open it owns the keyboard.
    if ui.picker.open {
        return handle_picker_key(key.code, ui);
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('q') | KeyCode::Esc, _) => EventOutcome::Quit,
        (KeyCode::Char('c') | KeyCode::Char('C'), KeyModifiers::CONTROL) => EventOutcome::Quit,
        (KeyCode::Char('?') | KeyCode::Char('h'), _) => {
            ui.show_help = !ui.show_help;
            EventOutcome::Continue
        }
        (KeyCode::Char('i') | KeyCode::Char('I'), _) => {
            ui.cycle_focus();
            EventOutcome::Continue
        }
        (KeyCode::Char('s') | KeyCode::Char('S'), _) => {
            ui.show_help = false;
            ui.picker.open(&app.config.symbol);
            EventOutcome::Continue
        }
        _ => EventOutcome::Ignore,
    }
}

fn handle_picker_key(code: KeyCode, ui: &mut UiState) -> EventOutcome {
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            ui.picker.close();
            EventOutcome::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            ui.picker.cursor_up();
            EventOutcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            ui.picker.cursor_down();
            EventOutcome::Continue
        }
        KeyCode::Enter => {
            let chosen = ui.picker.current_symbol().to_string();
            ui.picker.close();
            EventOutcome::SwitchSymbol(chosen)
        }
        _ => EventOutcome::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_focus_cycle() {
        let mut ui = UiState::default();
        assert!(ui.show_rsi() && ui.show_bollinger());
        ui.cycle_focus();
        assert!(ui.show_rsi() && !ui.show_bollinger());
        ui.cycle_focus();
        assert!(!ui.show_rsi() && ui.show_bollinger());
        ui.cycle_focus();
        assert!(ui.show_rsi() && ui.show_bollinger());
    }

    #[test]
    #[allow(clippy::panic)]
    fn picker_enter_returns_switch_symbol_and_closes_overlay() {
        let mut ui = UiState::default();
        ui.picker.open("btcusdt");
        ui.picker.cursor_down(); // ethusdt
        match handle_picker_key(KeyCode::Enter, &mut ui) {
            EventOutcome::SwitchSymbol(s) => assert_eq!(s, "ethusdt"),
            _ => panic!("Enter should yield SwitchSymbol"),
        }
        assert!(!ui.picker.open);
    }

    #[test]
    #[allow(clippy::panic)]
    fn picker_esc_closes_without_switching() {
        let mut ui = UiState::default();
        ui.picker.open("btcusdt");
        ui.picker.cursor_down();
        match handle_picker_key(KeyCode::Esc, &mut ui) {
            EventOutcome::Continue => {}
            _ => panic!("Esc should close without emitting SwitchSymbol"),
        }
        assert!(!ui.picker.open);
    }
}
