//! The Norton Commander colour palette.
//!
//! Classic DOS blue screen, cyan double-line panels, white body text, yellow
//! accents, and a black-on-cyan function-key bar.

use ratatui::style::{Color, Modifier, Style};

pub const BG: Color = Color::Blue;
pub const FG: Color = Color::White;
pub const DIM: Color = Color::Gray;
pub const ACCENT: Color = Color::Yellow;
pub const BORDER: Color = Color::Cyan;
pub const BORDER_ACTIVE: Color = Color::White;
pub const BAR_BG: Color = Color::Cyan;
pub const BAR_FG: Color = Color::Black;
pub const SEL_BG: Color = Color::Cyan;
pub const SEL_FG: Color = Color::Black;
pub const OK: Color = Color::Green;
pub const WARN: Color = Color::Yellow;
pub const ERR: Color = Color::Red;
pub const NODE_RECEIVER: Color = Color::Green;
pub const NODE_PROCESSOR: Color = Color::Cyan;
pub const NODE_EXPORTER: Color = Color::Magenta;
pub const NODE_CONNECTOR: Color = Color::Yellow;

/// The base blue background fill.
pub fn base() -> Style {
    Style::default().bg(BG).fg(FG)
}

/// White-on-blue body text.
pub fn text() -> Style {
    Style::default().fg(FG)
}

/// Dim secondary text.
pub fn dim() -> Style {
    Style::default().fg(DIM)
}

/// Bold yellow accent.
pub fn accent() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

/// Black-on-cyan bar (menu bar, function-key bar).
pub fn bar() -> Style {
    Style::default().bg(BAR_BG).fg(BAR_FG)
}

/// The selection highlight bar.
pub fn selection() -> Style {
    Style::default()
        .bg(SEL_BG)
        .fg(SEL_FG)
        .add_modifier(Modifier::BOLD)
}

/// Panel border colour, brighter when the panel is active.
pub fn border(active: bool) -> Style {
    Style::default().fg(if active { BORDER_ACTIVE } else { BORDER })
}

/// Panel title style.
pub fn title(active: bool) -> Style {
    if active {
        Style::default()
            .bg(BAR_BG)
            .fg(BAR_FG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(BORDER).add_modifier(Modifier::BOLD)
    }
}

/// Colour for a health indicator.
pub fn health(healthy: bool) -> Color {
    if healthy {
        OK
    } else {
        ERR
    }
}
