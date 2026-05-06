//! Live price chart with Bollinger-band overlay.
//!
//! Datasets are built fresh on every frame from the deques in
//! [`AppState`]: a price line in cream and three band lines in gold +
//! ink. The Y-range auto-scales to whatever is currently on screen,
//! padded a touch so the line never grazes the chart border.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::symbols;
use ratatui::text::Span;
use ratatui::widgets::{Axis, Block, BorderType, Borders, Chart, Dataset, GraphType};
use ratatui::Frame;

use crate::app::AppState;

use super::theme::*;

/// Render the centre chart band.
pub fn render_price_chart(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    if app.history.is_empty() {
        // Render an empty chart frame so the layout stays stable
        // before the first tick arrives.
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(rule());
        frame.render_widget(block, area);
        return;
    }

    let prices: Vec<(f64, f64)> = app
        .history
        .iter()
        .enumerate()
        .map(|(i, (_, p))| (i as f64, *p))
        .collect();

    let mut upper: Vec<(f64, f64)> = Vec::new();
    let mut middle: Vec<(f64, f64)> = Vec::new();
    let mut lower: Vec<(f64, f64)> = Vec::new();
    for (i, b_opt) in app.bollinger_history.iter().enumerate() {
        if let Some(b) = b_opt {
            upper.push((i as f64, b.upper));
            middle.push((i as f64, b.middle));
            lower.push((i as f64, b.lower));
        }
    }

    let (y_min, y_max) = visible_y_range(&prices, &upper, &lower);
    let x_max = (prices.len().saturating_sub(1)) as f64;

    let mut datasets = vec![Dataset::default()
        .name("price")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(CREAM))
        .data(&prices)];
    if !upper.is_empty() {
        datasets.push(
            Dataset::default()
                .name("BB upper")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(GOLD))
                .data(&upper),
        );
    }
    if !middle.is_empty() {
        datasets.push(
            Dataset::default()
                .name("BB mid")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(INK))
                .data(&middle),
        );
    }
    if !lower.is_empty() {
        datasets.push(
            Dataset::default()
                .name("BB lower")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(GOLD))
                .data(&lower),
        );
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(rule());

    let y_labels = y_axis_labels(y_min, y_max);
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(
            Axis::default()
                .style(rule())
                .bounds([0.0, x_max.max(1.0)])
                .labels::<Vec<Span<'_>>>(vec![]),
        )
        .y_axis(
            Axis::default()
                .style(rule())
                .bounds([y_min, y_max])
                .labels(y_labels),
        );

    frame.render_widget(chart, area);
}

fn visible_y_range(
    prices: &[(f64, f64)],
    upper: &[(f64, f64)],
    lower: &[(f64, f64)],
) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for (_, v) in prices.iter().chain(upper.iter()).chain(lower.iter()) {
        if *v < min {
            min = *v;
        }
        if *v > max {
            max = *v;
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    if (max - min).abs() < f64::EPSILON {
        // Constant series — pad symmetrically so the line is centred.
        let pad = (min.abs() * 0.001).max(0.5);
        return (min - pad, max + pad);
    }
    let span = max - min;
    let pad = span * 0.05;
    (min - pad, max + pad)
}

fn y_axis_labels(y_min: f64, y_max: f64) -> Vec<Span<'static>> {
    let mid = (y_min + y_max) / 2.0;
    vec![
        Span::styled(format_axis(y_min), Style::default().fg(INK)),
        Span::styled(format_axis(mid), Style::default().fg(INK)),
        Span::styled(format_axis(y_max), Style::default().fg(CREAM)),
    ]
}

fn format_axis(v: f64) -> String {
    if v.abs() >= 1000.0 {
        let int_part = v.round() as i64;
        let mut s = int_part.abs().to_string();
        let mut out = String::new();
        while s.len() > 3 {
            let split = s.len() - 3;
            let tail = s.split_off(split);
            out = if out.is_empty() {
                tail
            } else {
                format!("{tail},{out}")
            };
        }
        let prefix = if int_part < 0 { "-" } else { "" };
        if out.is_empty() {
            format!("{prefix}{s}")
        } else {
            format!("{prefix}{s},{out}")
        }
    } else if v.abs() >= 1.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.4}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_y_range_pads_a_constant_series() {
        let prices: Vec<(f64, f64)> = (0..5).map(|i| (i as f64, 100.0)).collect();
        let (lo, hi) = visible_y_range(&prices, &[], &[]);
        assert!(lo < 100.0 && hi > 100.0);
    }

    #[test]
    fn visible_y_range_includes_bands() {
        let prices = vec![(0.0, 100.0), (1.0, 101.0)];
        let upper = vec![(0.0, 110.0), (1.0, 111.0)];
        let lower = vec![(0.0, 90.0), (1.0, 89.0)];
        let (lo, hi) = visible_y_range(&prices, &upper, &lower);
        assert!(lo < 89.0 && hi > 111.0);
    }

    #[test]
    fn format_axis_thousands_for_large_numbers() {
        assert_eq!(format_axis(67250.0), "67,250");
        assert_eq!(format_axis(1_234_567.0), "1,234,567");
    }

    #[test]
    fn format_axis_uses_two_decimals_in_mid_range() {
        assert_eq!(format_axis(12.5), "12.50");
    }

    #[test]
    fn format_axis_uses_four_decimals_for_sub_unit() {
        assert_eq!(format_axis(0.1234), "0.1234");
    }
}
