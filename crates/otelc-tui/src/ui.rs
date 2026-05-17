//! Root rendering: the Norton Commander frame, bars, menus and modals.

use crate::app::{App, Menu, MenuAction, Modal, Side};
use crate::theme;
use crate::views;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};
use ratatui::Frame;

const FN_KEYS: [(&str, &str); 10] = [
    ("1", "Help"),
    ("2", "Menu"),
    ("3", "View"),
    ("4", "Edit"),
    ("5", "Push"),
    ("6", "Restart"),
    ("7", "Filter"),
    ("8", "Health"),
    ("9", "Menu"),
    ("10", "Quit"),
];

/// Draw the whole screen.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::new().style(theme::base()), area);

    let [menu_bar, body, status_bar, cmd_bar, key_bar] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    render_menubar(frame, menu_bar, app);

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(body);
    views::render_panel(frame, left, app, Side::Left);
    views::render_panel(frame, right, app, Side::Right);

    render_status(frame, status_bar, app);
    render_cmdline(frame, cmd_bar, app);
    render_fnbar(frame, key_bar);

    if let Some(menu) = &app.menu {
        render_menu(frame, area, menu);
    }
    if app.modal.is_some() {
        render_modal(frame, area, app);
    }
}

fn render_menubar(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![Span::styled(
        " otelc ",
        Style::default().add_modifier(Modifier::BOLD),
    )];
    for view in crate::app::ViewMode::ALL {
        spans.push(Span::raw(" "));
        spans.push(Span::raw(view.label()));
    }
    spans.push(Span::raw("   │  "));
    spans.push(Span::raw(format!(
        "{} {}  ·  {} agent(s)",
        app.control.mode(),
        app.control.endpoint(),
        app.visible().len()
    )));
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme::bar()), area);
}

fn render_status(frame: &mut Frame, area: Rect, app: &App) {
    let line = match app.selected_agent() {
        Some(agent) => {
            let caps: Vec<&str> = agent
                .capabilities
                .iter()
                .filter(|(_, on)| *on)
                .map(|(n, _)| short_cap(n))
                .collect();
            Line::from(vec![
                Span::styled(" instance-uid ", theme::dim()),
                Span::styled(agent.uid.clone(), theme::text()),
                Span::styled("  health ", theme::dim()),
                Span::styled(
                    if agent.healthy { "OK" } else { "DEGRADED" },
                    Style::default().fg(theme::health(agent.healthy)),
                ),
                Span::styled("  caps ", theme::dim()),
                Span::styled(caps.join("·"), theme::text()),
            ])
        }
        None => Line::from(Span::styled(" no agent selected", theme::dim())),
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn render_cmdline(frame: &mut Frame, area: Rect, app: &App) {
    let line = if app.filtering {
        Line::from(vec![
            Span::styled(" filter> ", theme::accent()),
            Span::styled(format!("{}\u{2588}", app.filter), theme::text()),
        ])
    } else {
        Line::from(vec![
            Span::styled(" otelc> ", theme::accent()),
            Span::styled(app.status.clone(), theme::text()),
        ])
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn render_fnbar(frame: &mut Frame, area: Rect) {
    let mut spans = Vec::new();
    for (num, label) in FN_KEYS {
        spans.push(Span::styled(
            format!(" {num}"),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!("{label} ")));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme::bar()), area);
}

fn render_menu(frame: &mut Frame, area: Rect, menu: &Menu) {
    let width = 30u16.min(area.width);
    let height = (menu.entries.len() as u16 + 2).min(area.height);
    let rect = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(theme::border(true))
        .title(Span::styled(" Menu ", theme::title(true)))
        .style(theme::base());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let lines: Vec<Line> = menu
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            if matches!(entry.action, MenuAction::Separator) {
                Line::from(Span::styled("─".repeat(inner.width as usize), theme::dim()))
            } else if i == menu.selected {
                Line::from(Span::styled(
                    pad(&format!(" {}", entry.label), inner.width as usize),
                    theme::selection(),
                ))
            } else {
                Line::from(Span::styled(format!(" {}", entry.label), theme::text()))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_modal(frame: &mut Frame, area: Rect, app: &mut App) {
    match app.modal.as_mut() {
        Some(Modal::Help) => render_box(frame, area, "Help — otelc", 66, 18, help_text()),
        Some(Modal::Message { title, body }) => {
            let t = title.clone();
            let lines = vec![
                Line::raw(""),
                Line::from(Span::styled(body.clone(), theme::text())),
                Line::raw(""),
                Line::from(Span::styled("  press any key to dismiss", theme::dim())),
            ];
            render_box(frame, area, &t, 60, 8, lines);
        }
        Some(Modal::Confirm { title, body, .. }) => {
            let t = title.clone();
            let lines = vec![
                Line::raw(""),
                Line::from(Span::styled(body.clone(), theme::text())),
                Line::raw(""),
                Line::from(Span::styled(
                    "  [ Enter = Yes ]    [ Esc = No ]",
                    theme::accent(),
                )),
            ];
            render_box(frame, area, &t, 56, 8, lines);
        }
        Some(Modal::Editor(editor)) => {
            let rect = centered(
                area.width.saturating_sub(6).max(24),
                area.height.saturating_sub(4).max(10),
                area,
            );
            frame.render_widget(Clear, rect);
            let block = Block::bordered()
                .border_type(BorderType::Double)
                .border_style(theme::border(true))
                .title(Span::styled(
                    format!(" Edit remote config — {} ", editor.agent),
                    theme::title(true),
                ))
                .style(theme::base());
            let inner = block.inner(rect);
            frame.render_widget(block, rect);

            let [text_area, hint] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
            let viewport = text_area.height as usize;
            if viewport > 0 {
                if editor.cy < editor.scroll {
                    editor.scroll = editor.cy;
                } else if editor.cy >= editor.scroll + viewport {
                    editor.scroll = editor.cy + 1 - viewport;
                }
            }
            let lines: Vec<Line> = editor
                .lines
                .iter()
                .skip(editor.scroll)
                .take(viewport)
                .map(|l| Line::from(Span::styled(l.clone(), theme::text())))
                .collect();
            frame.render_widget(Paragraph::new(Text::from(lines)), text_area);
            frame.render_widget(
                Paragraph::new(Span::raw(
                    " F5 Push   Esc Cancel   arrows move   Enter newline ",
                ))
                .style(theme::bar()),
                hint,
            );
            let cursor_x =
                text_area.x + (editor.cx.min(text_area.width.saturating_sub(1) as usize)) as u16;
            let cursor_y = text_area.y + (editor.cy - editor.scroll) as u16;
            frame.set_cursor_position((cursor_x, cursor_y));
        }
        None => {}
    }
}

fn render_box(frame: &mut Frame, area: Rect, title: &str, w: u16, h: u16, lines: Vec<Line>) {
    let rect = centered(w, h, area);
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(theme::border(true))
        .title(Span::styled(format!(" {title} "), theme::title(true)))
        .style(theme::base());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn help_text() -> Vec<Line<'static>> {
    let rows = [
        ("Tab", "switch active panel"),
        (
            "1-6",
            "set active panel view (Fleet/Config/Pipeline/Metrics/Logs/Health)",
        ),
        ("Up/Down", "move fleet selection, or scroll a view"),
        ("Enter", "open the selected agent's config"),
        ("F1", "this help"),
        ("F2 / F9", "open the menu"),
        ("F3", "cycle the active panel's view"),
        ("F4 / F5", "edit and push remote config"),
        ("F6", "restart the selected agent"),
        ("F7", "filter the fleet"),
        ("F10 / q", "quit"),
    ];
    let mut lines = vec![Line::raw("")];
    for (key, desc) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("  {key:<10}"), theme::accent()),
            Span::styled(desc, theme::text()),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  otelc manages OpenTelemetry Collectors over OpAMP.",
        theme::dim(),
    )));
    lines
}

fn short_cap(name: &str) -> &str {
    match name {
        "AcceptsRemoteConfig" => "CFG",
        "ReportsEffectiveConfig" => "EFFCFG",
        "ReportsHealth" => "HEALTH",
        "ReportsOwnMetrics" => "METRICS",
        "ReportsOwnLogs" => "LOGS",
        "ReportsOwnTraces" => "TRACES",
        "AcceptsRestartCommand" => "RESTART",
        "ReportsRemoteConfig" => "RCFG",
        "ReportsStatus" => "STATUS",
        _ => name,
    }
}

fn pad(text: &str, width: usize) -> String {
    let mut s = text.to_string();
    let len = s.chars().count();
    if len < width {
        s.extend(std::iter::repeat_n(' ', width - len));
    }
    s
}

fn centered(w: u16, h: u16, area: Rect) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}
