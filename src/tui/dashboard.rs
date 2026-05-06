//! Top-level dashboard composition.
//!
//! Lays out the four bands of the screen — masthead, chart,
//! indicator panels, footer — and dispatches to the per-section
//! renderers in [`super::chart`] and [`super::help`].

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::AppState;
use crate::indicators::Bands;
use crate::ws::WsStatus;

use super::chart::render_price_chart;
use super::help::render_help_overlay;
use super::picker::render_symbol_picker;
use super::theme::*;
use super::UiState;

/// Render the full dashboard for the current frame.
pub fn render(frame: &mut Frame<'_>, app: &AppState, ui: &UiState) {
    let area = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .horizontal_margin(2)
        .vertical_margin(1)
        .constraints([
            Constraint::Length(4),                    // masthead
            Constraint::Min(8),                       // chart
            Constraint::Length(indicator_height(ui)), // indicators
            Constraint::Length(1),                    // footer
        ])
        .split(area);

    render_masthead(frame, outer[0], app);
    render_price_chart(frame, outer[1], app);
    render_indicator_panels(frame, outer[2], app, ui);
    render_footer(frame, outer[3]);

    if ui.show_help {
        render_help_overlay(frame, area);
    }
    if ui.picker.open {
        render_symbol_picker(frame, area, &ui.picker);
    }
}

fn indicator_height(ui: &UiState) -> u16 {
    match (ui.show_rsi(), ui.show_bollinger()) {
        (true, true) => 9,   // both panels stacked
        (true, false) => 4,  // RSI only
        (false, true) => 6,  // Bollinger only
        (false, false) => 1, // shouldn't happen, defensive
    }
}

// ------- masthead -------

fn render_masthead(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title rule
            Constraint::Length(1), // spacer
            Constraint::Length(1), // headline price line
            Constraint::Length(1), // spacer
        ])
        .split(area);

    render_title_rule(frame, rows[0], app);
    render_headline(frame, rows[2], app);
}

fn render_title_rule(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let symbol_upper = app.config.symbol.to_uppercase();
    let left = format!(" CRYPTOTUI · {symbol_upper} ");
    let (status_glyph, status_word, status_style) = match &app.status {
        WsStatus::Connected => (DOT, "live", gold()),
        WsStatus::Connecting => (DOT, "connecting", ink()),
        WsStatus::Reconnecting { attempt, .. } => {
            let _ = attempt;
            (DOT, "reconnecting", bearish())
        }
    };
    let right = format!(" {status_glyph} {status_word} · vol. 01 ");

    let total = area.width as usize;
    let used = left.chars().count() + right.chars().count();
    let mid_width = total.saturating_sub(used + 2);
    let mid: String = HRULE.repeat(mid_width);

    let line = Line::from(vec![
        Span::styled(HRULE.to_string(), rule()),
        Span::styled(left, cream().add_modifier(Modifier::BOLD)),
        Span::styled(mid, rule()),
        Span::styled(right, status_style),
        Span::styled(HRULE.to_string(), rule()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_headline(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let price = app
        .last_tick
        .as_ref()
        .map(|t| format_price(t.price))
        .unwrap_or_else(|| "—".to_string());

    let (change_glyph, change_text, change_style) = match app.session_change_pct() {
        Some(p) if p > 0.0001 => (TRIANGLE_UP, format!("+{p:.2} %"), bullish()),
        Some(p) if p < -0.0001 => (TRIANGLE_DOWN, format!("{p:.2} %"), bearish()),
        Some(_) => (MIDDLE_DOT, "0.00 %".to_string(), ink()),
        None => (MIDDLE_DOT, "—".to_string(), ink()),
    };
    let trades = format_count(app.tick_count);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Percentage(30),
            Constraint::Percentage(25),
        ])
        .split(area);

    let price_line = Line::from(vec![
        Span::styled("$ ", ink()),
        Span::styled(price, cream_bold()),
    ]);
    frame.render_widget(Paragraph::new(price_line), cols[0]);

    let change_line = Line::from(vec![
        Span::styled(format!("{change_glyph} "), change_style),
        Span::styled(change_text, change_style),
    ]);
    frame.render_widget(
        Paragraph::new(change_line).alignment(Alignment::Left),
        cols[1],
    );

    let trades_line = Line::from(vec![
        Span::styled(trades, cream()),
        Span::styled(" trades", ink()),
    ]);
    frame.render_widget(
        Paragraph::new(trades_line).alignment(Alignment::Right),
        cols[2],
    );
}

// ------- indicator panels -------

fn render_indicator_panels(frame: &mut Frame<'_>, area: Rect, app: &AppState, ui: &UiState) {
    match (ui.show_rsi(), ui.show_bollinger()) {
        (true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(4), Constraint::Length(5)])
                .split(area);
            render_rsi_panel(frame, rows[0], app);
            render_bollinger_panel(frame, rows[1], app);
        }
        (true, false) => render_rsi_panel(frame, area, app),
        (false, true) => render_bollinger_panel(frame, area, app),
        (false, false) => {}
    }
}

fn render_rsi_panel(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    let title = section_rule(
        rows[0].width,
        format!("RSI · {}", app.config.indicators.rsi_period),
    );
    frame.render_widget(Paragraph::new(title), rows[0]);

    let inner_area = rows[1];
    let value = app.latest_rsi();
    let gauge_width = inner_area.width.saturating_sub(28).max(8) as usize;
    let (gauge, value_text, regime_text, regime_style) = match value {
        Some(v) => {
            let filled = ((v / 100.0) * gauge_width as f64)
                .round()
                .clamp(0.0, gauge_width as f64) as usize;
            let mut bar = String::with_capacity(gauge_width * 3);
            for _ in 0..filled {
                bar.push_str(BLOCK_FILLED);
            }
            for _ in filled..gauge_width {
                bar.push_str(BLOCK_EMPTY);
            }
            let (regime, style) = if v >= 70.0 {
                ("OVERBOUGHT", bearish())
            } else if v <= 30.0 {
                ("OVERSOLD", bullish())
            } else {
                ("NEUTRAL", ink())
            };
            (bar, format!("{v:6.2}"), regime, style)
        }
        None => {
            let bar: String = BLOCK_EMPTY.repeat(gauge_width);
            (bar, "  · ·".to_string(), "WARMING UP", ink())
        }
    };

    let line = Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(gauge, gold()),
        Span::raw("  "),
        Span::styled(value_text, cream_bold()),
        Span::raw("   "),
        Span::styled(regime_text, regime_style),
    ]);
    frame.render_widget(Paragraph::new(line), inner_area);
}

fn render_bollinger_panel(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    let title = section_rule(
        rows[0].width,
        format!(
            "BOLLINGER · {} / {}σ",
            app.config.indicators.bollinger_period, app.config.indicators.bollinger_k as u32
        ),
    );
    frame.render_widget(Paragraph::new(title), rows[0]);

    let bands = app.latest_bollinger();
    let (upper, middle, lower) = match bands {
        Some(Bands {
            upper,
            middle,
            lower,
        }) => (
            format_price(upper),
            format_price(middle),
            format_price(lower),
        ),
        None => ("—".to_string(), "—".to_string(), "—".to_string()),
    };

    frame.render_widget(
        Paragraph::new(band_line(CHEVRON_UP, "upper", &upper, gold())),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(band_line(MIDDLE_DOT, "middle", &middle, cream())),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(band_line(CHEVRON_DOWN, "lower", &lower, gold())),
        rows[3],
    );
}

fn band_line<'a>(glyph: &'a str, label: &'a str, value: &'a str, value_style: Style) -> Line<'a> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(glyph, ink()),
        Span::raw("  "),
        Span::styled(format!("{label:<8}"), ink()),
        Span::styled(value.to_string(), value_style),
    ])
}

// ------- footer -------

fn render_footer(frame: &mut Frame<'_>, area: Rect) {
    let left_text = " q quit · ? help · s switch · i focus ";
    let right_text = format!(" © {COPYRIGHT_YEAR} · cryptotui ");
    let total = area.width as usize;
    let used = left_text.chars().count() + right_text.chars().count();
    let pad = total.saturating_sub(used + 2);
    let mid: String = HRULE.repeat(pad);

    let line = Line::from(vec![
        Span::styled(HRULE.to_string(), rule()),
        Span::styled(left_text, ink()),
        Span::styled(mid, rule()),
        Span::styled(right_text, ink()),
        Span::styled(HRULE.to_string(), rule()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

// ------- helpers -------

fn section_rule(width: u16, label: String) -> Line<'static> {
    let label_with_padding = format!(" {label} ");
    let total = width as usize;
    let used = label_with_padding.chars().count() + 1;
    let trailing = total.saturating_sub(used);
    let trail: String = HRULE.repeat(trailing);
    Line::from(vec![
        Span::styled(HRULE.to_string(), rule()),
        Span::styled(label_with_padding, gold()),
        Span::styled(trail, rule()),
    ])
}

fn format_price(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let sign = if rounded < 0.0 { "-" } else { "" };
    let abs = rounded.abs();
    let int_part = abs.trunc() as u64;
    let frac = (abs.fract() * 100.0).round() as u64;
    let mut int_str = int_part.to_string();
    let mut out = String::new();
    while int_str.len() > 3 {
        let split = int_str.len() - 3;
        let tail = int_str.split_off(split);
        out = if out.is_empty() {
            tail
        } else {
            format!("{tail},{out}")
        };
    }
    out = if out.is_empty() {
        int_str
    } else {
        format!("{int_str},{out}")
    };
    format!("{sign}{out}.{frac:02}")
}

fn format_count(value: u64) -> String {
    let mut digits = value.to_string();
    let mut out = String::new();
    while digits.len() > 3 {
        let split = digits.len() - 3;
        let tail = digits.split_off(split);
        out = if out.is_empty() {
            tail
        } else {
            format!("{tail},{out}")
        };
    }
    if out.is_empty() {
        digits
    } else {
        format!("{digits},{out}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_price_inserts_thousands_separator() {
        assert_eq!(format_price(67250.42), "67,250.42");
        assert_eq!(format_price(1234567.0), "1,234,567.00");
        assert_eq!(format_price(7.5), "7.50");
        assert_eq!(format_price(0.99), "0.99");
    }

    #[test]
    fn format_count_inserts_thousands_separator() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_247), "1,247");
        assert_eq!(format_count(12_345_678), "12,345,678");
    }

    #[test]
    fn indicator_height_matches_focus() {
        let ui = UiState {
            indicator_focus: 0,
            ..Default::default()
        };
        assert_eq!(indicator_height(&ui), 9);
        let ui = UiState {
            indicator_focus: 1,
            ..Default::default()
        };
        assert_eq!(indicator_height(&ui), 4);
        let ui = UiState {
            indicator_focus: 2,
            ..Default::default()
        };
        assert_eq!(indicator_height(&ui), 6);
    }
}
