//! In-app symbol picker.
//!
//! Toggled with `s`. Renders a centred overlay listing a curated set
//! of streamable Binance pairs grouped by asset class (crypto, then
//! tokenised metals). Arrow keys / j-k navigate, Enter confirms,
//! Escape cancels. Confirming surfaces the new symbol up to the run
//! loop, which tears down the current WebSocket session and starts a
//! fresh one.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState};
use ratatui::Frame;

use super::theme::*;

/// Asset class shown in the picker — used to draw a divider between
/// crypto pairs and tokenised metals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetClass {
    /// Native crypto pair on Binance.
    Crypto,
    /// Tokenised precious metal traded as a crypto pair (e.g. PAXG).
    Metal,
}

/// One row in the picker.
#[derive(Debug, Clone, Copy)]
pub struct PresetSymbol {
    /// Lowercase Binance symbol, e.g. `"btcusdt"`.
    pub symbol: &'static str,
    /// Human-readable label shown next to the ticker.
    pub label: &'static str,
    /// Asset class (drives the divider above metals).
    pub class: AssetClass,
}

/// Curated list of streamable pairs. Ordered: top crypto by liquidity,
/// then tokenised gold. Add to this list rather than letting the user
/// type free-form symbols — keeps mistyped tickers out.
pub const PRESETS: &[PresetSymbol] = &[
    PresetSymbol {
        symbol: "btcusdt",
        label: "Bitcoin",
        class: AssetClass::Crypto,
    },
    PresetSymbol {
        symbol: "ethusdt",
        label: "Ethereum",
        class: AssetClass::Crypto,
    },
    PresetSymbol {
        symbol: "solusdt",
        label: "Solana",
        class: AssetClass::Crypto,
    },
    PresetSymbol {
        symbol: "bnbusdt",
        label: "BNB",
        class: AssetClass::Crypto,
    },
    PresetSymbol {
        symbol: "xrpusdt",
        label: "XRP",
        class: AssetClass::Crypto,
    },
    PresetSymbol {
        symbol: "dogeusdt",
        label: "Dogecoin",
        class: AssetClass::Crypto,
    },
    PresetSymbol {
        symbol: "paxgusdt",
        label: "PAX Gold (1 oz)",
        class: AssetClass::Metal,
    },
    PresetSymbol {
        symbol: "xautusdt",
        label: "Tether Gold (1 oz)",
        class: AssetClass::Metal,
    },
];

/// Picker state: whether it's visible and which preset is highlighted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SymbolPicker {
    /// Whether the overlay is currently rendered.
    pub open: bool,
    /// Index into [`PRESETS`].
    pub selected: usize,
}

impl SymbolPicker {
    /// Open the overlay with `current` highlighted (if it appears in
    /// [`PRESETS`]) or the first row otherwise.
    pub fn open(&mut self, current: &str) {
        self.open = true;
        self.selected = PRESETS
            .iter()
            .position(|p| p.symbol.eq_ignore_ascii_case(current))
            .unwrap_or(0);
    }

    /// Close the overlay without committing a selection.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Move the highlight up one row (saturating).
    pub fn cursor_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move the highlight down one row (saturating).
    pub fn cursor_down(&mut self) {
        if self.selected + 1 < PRESETS.len() {
            self.selected += 1;
        }
    }

    /// The symbol currently under the cursor.
    pub fn current_symbol(&self) -> &'static str {
        PRESETS
            .get(self.selected)
            .map(|p| p.symbol)
            .unwrap_or("btcusdt")
    }
}

/// Draw the picker overlay over `area`.
pub fn render_symbol_picker(frame: &mut Frame<'_>, area: Rect, picker: &SymbolPicker) {
    let popup = centred_rect(area, 50, 60);
    frame.render_widget(Clear, popup);

    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled("SWITCH SYMBOL", gold()),
        Span::raw(" "),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(rule())
        .title(title);

    let mut items: Vec<ListItem<'_>> = Vec::with_capacity(PRESETS.len() + 2);
    let mut last_class: Option<AssetClass> = None;
    for preset in PRESETS {
        if let Some(prev) = last_class {
            if prev != preset.class {
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "  ─ commodities ─",
                    rule(),
                )])));
            }
        }
        let symbol_span = Span::styled(format!("  {:<10}", preset.symbol), cream());
        let label_span = Span::styled(preset.label, ink());
        items.push(ListItem::new(Line::from(vec![symbol_span, label_span])));
        last_class = Some(preset.class);
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(GOLD)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        )
        .highlight_symbol(" ▸ ");

    let mut state = ListState::default();
    // The list contains a divider row inserted between crypto and metals;
    // shift the highlight index past it so the visible cursor matches.
    state.select(Some(visual_index(picker.selected)));

    frame.render_stateful_widget(list, popup, &mut state);
}

/// The picker inserts a one-row divider between asset classes; map the
/// preset index to its row index in the rendered list.
fn visual_index(selected: usize) -> usize {
    let mut idx = 0;
    let mut last_class: Option<AssetClass> = None;
    for (i, preset) in PRESETS.iter().enumerate() {
        if let Some(prev) = last_class {
            if prev != preset.class {
                idx += 1;
            }
        }
        if i == selected {
            return idx;
        }
        idx += 1;
        last_class = Some(preset.class);
    }
    idx
}

fn centred_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_highlights_current_symbol() {
        let mut p = SymbolPicker::default();
        p.open("ethusdt");
        assert!(p.open);
        assert_eq!(p.current_symbol(), "ethusdt");
    }

    #[test]
    fn open_falls_back_to_first_preset_for_unknown_symbol() {
        let mut p = SymbolPicker::default();
        p.open("unknown");
        assert_eq!(p.current_symbol(), PRESETS[0].symbol);
    }

    #[test]
    fn cursor_up_saturates_at_zero() {
        let mut p = SymbolPicker::default();
        p.cursor_up();
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn cursor_down_saturates_at_last_preset() {
        let mut p = SymbolPicker::default();
        for _ in 0..(PRESETS.len() + 5) {
            p.cursor_down();
        }
        assert_eq!(p.selected, PRESETS.len() - 1);
    }

    #[test]
    fn presets_include_gold_pairs() {
        let symbols: Vec<&str> = PRESETS.iter().map(|p| p.symbol).collect();
        assert!(symbols.contains(&"paxgusdt"));
        assert!(symbols.contains(&"xautusdt"));
    }

    #[test]
    fn visual_index_accounts_for_divider() {
        // First crypto: visual 0.
        assert_eq!(visual_index(0), 0);
        // First metal entry (index 6 in PRESETS) sits one row past the divider.
        let first_metal = PRESETS
            .iter()
            .position(|p| p.class == AssetClass::Metal)
            .unwrap();
        assert_eq!(visual_index(first_metal), first_metal + 1);
    }
}
