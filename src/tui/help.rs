//! Floating help overlay.
//!
//! Toggled with `?` (or `h`). Uses [`ratatui::widgets::Clear`] so the
//! underlying dashboard is wiped beneath the panel, then renders a
//! framed paragraph centred in the viewport.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::theme::*;

/// Draw the help panel over `area`, sized to a sensible centred rect.
pub fn render_help_overlay(frame: &mut Frame<'_>, area: Rect) {
    let popup = centred_rect(area, 50, 40);
    frame.render_widget(Clear, popup);

    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled("KEYBINDINGS", gold()),
        Span::raw(" "),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(rule())
        .title(title);

    let lines = vec![
        Line::from(""),
        binding(" q · esc ", "quit cryptotui"),
        binding(" ? · h   ", "toggle this panel"),
        binding(" s       ", "switch symbol (BTC, ETH, gold, ...)"),
        binding(" i       ", "cycle indicator focus"),
        binding(" ctrl-c  ", "interrupt and exit"),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Streaming live trade data from Binance.", ink()),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Indicators warm up after the configured period.", ink()),
        ]),
    ];
    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left)
        .style(Style::default().fg(CREAM));
    frame.render_widget(paragraph, popup);
}

fn binding<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(key, gold()),
        Span::styled("  ", Style::default()),
        Span::styled(desc, cream()),
    ])
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
