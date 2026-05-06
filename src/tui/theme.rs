//! Visual identity for the cryptotui dashboard.
//!
//! The design language is "private banker": dark canvas, restrained
//! warm palette (cream, brushed gold, oxblood), typography spaced for
//! breathing room. Every colour and glyph the dashboard uses lives
//! here so the look can be retuned in one place.
//!
//! All colours are 24-bit RGB. On terminals without truecolor support
//! crossterm degrades them gracefully to the nearest 256-colour cell.

use ratatui::style::{Color, Modifier, Style};

// ------- palette -------

/// Headline ink — warm cream, used for prices and primary text.
pub const CREAM: Color = Color::Rgb(238, 230, 210);
/// Brushed gold — the live accent, used for active values and the
/// "live" indicator.
pub const GOLD: Color = Color::Rgb(212, 175, 99);
/// Oxblood — bearish/error tone.
pub const BORDEAUX: Color = Color::Rgb(176, 80, 80);
/// Muted sage — bullish accent, deliberately subdued.
pub const SAGE: Color = Color::Rgb(132, 168, 122);
/// Mid-grey ink for secondary text.
pub const INK: Color = Color::Rgb(150, 145, 130);
/// Soft grey for borders and rules — visible without shouting.
pub const RULE: Color = Color::Rgb(95, 92, 80);
/// Faint background lines for filled gauges.
pub const FAINT: Color = Color::Rgb(55, 53, 48);

// ------- styles -------

/// Cream foreground, no decoration.
pub fn cream() -> Style {
    Style::default().fg(CREAM)
}
/// Bold cream — used for the headline price.
pub fn cream_bold() -> Style {
    Style::default().fg(CREAM).add_modifier(Modifier::BOLD)
}
/// Gold foreground — accents and live indicators.
pub fn gold() -> Style {
    Style::default().fg(GOLD)
}
/// Dim grey — secondary labels.
pub fn ink() -> Style {
    Style::default().fg(INK)
}
/// Border / rule colour.
pub fn rule() -> Style {
    Style::default().fg(RULE)
}
/// Bullish (sage).
pub fn bullish() -> Style {
    Style::default().fg(SAGE)
}
/// Bearish (oxblood).
pub fn bearish() -> Style {
    Style::default().fg(BORDEAUX)
}
/// Faint fill (gauge background trough).
pub fn faint() -> Style {
    Style::default().fg(FAINT)
}

// ------- glyphs -------

/// Live indicator — a filled circle pulsing in gold.
pub const DOT: &str = "●";
/// Bullish triangle (price up).
pub const TRIANGLE_UP: &str = "▲";
/// Bearish triangle (price down).
pub const TRIANGLE_DOWN: &str = "▼";
/// Horizontal rule used in section dividers.
pub const HRULE: &str = "─";
/// Filled gauge segment.
pub const BLOCK_FILLED: &str = "▰";
/// Empty gauge segment.
pub const BLOCK_EMPTY: &str = "▱";
/// Above-band marker, used in the Bollinger panel.
pub const CHEVRON_UP: &str = "⌃";
/// Below-band marker.
pub const CHEVRON_DOWN: &str = "⌄";
/// Centred mid-band marker.
pub const MIDDLE_DOT: &str = "·";

/// Roman numeral spelling of the current copyright year. Bumping it is
/// a deliberate manual act so I never accidentally backdate the notice.
pub const COPYRIGHT_YEAR: &str = "MMXXVI";
