//! Renderers for the six panel view-modes.

use crate::app::{App, Panel, Side, ViewMode};
use crate::control::{AgentDetail, HealthNode};
use crate::pipeline;
use crate::theme;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};
use ratatui::Frame;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Render one of the two panels.
pub fn render_panel(frame: &mut Frame, area: Rect, app: &App, side: Side) {
    let panel = match side {
        Side::Left => &app.left,
        Side::Right => &app.right,
    };
    let active = app.active == side;
    let subject = match panel.view {
        ViewMode::Fleet => format!("{} agent(s)", app.visible().len()),
        _ => app
            .selected_agent()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "no agent".to_string()),
    };
    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(panel.view.label(), theme::title(active)),
        Span::raw(" · "),
        Span::styled(subject, theme::dim()),
        Span::raw(" "),
    ]);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(theme::border(active))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match panel.view {
        ViewMode::Fleet => render_fleet(frame, inner, app),
        ViewMode::Config => render_config(frame, inner, app, panel),
        ViewMode::Pipeline => render_pipeline(frame, inner, app, panel),
        ViewMode::Metrics => render_metrics(frame, inner, app, panel),
        ViewMode::Logs => render_logs(frame, inner, app, panel),
        ViewMode::Health => render_health(frame, inner, app, panel),
    }
}

fn render_fleet(frame: &mut Frame, area: Rect, app: &App) {
    let visible = app.visible();
    let width = area.width as usize;
    let selected_uid = app.selected_agent().map(|a| a.uid.clone());

    let mut lines = vec![Line::from(Span::styled(
        pad(
            " STATE  NAME                VERSION  STATUS        SEEN",
            width,
        ),
        theme::accent(),
    ))];

    if visible.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no agents connected — start the mock-agent or point a collector here)",
            theme::dim(),
        )));
    }

    let height = area.height.saturating_sub(1) as usize;
    let selected_idx = selected_uid
        .as_ref()
        .and_then(|uid| visible.iter().position(|a| &a.uid == uid))
        .unwrap_or(0);
    let offset = selected_idx.saturating_sub(height.saturating_sub(1));

    for agent in visible.iter().skip(offset) {
        let selected = Some(&agent.uid) == selected_uid.as_ref();
        let dot_color = theme::health(agent.healthy);
        let row = format!(
            "   {}  {:<18} {:>7}  {:<12} {:>6}",
            if agent.healthy { "up " } else { "DOWN" },
            truncate(&agent.name, 18),
            truncate(&agent.version, 7),
            truncate(status_text(agent), 12),
            ago(agent.last_seen),
        );
        if selected {
            lines.push(Line::from(Span::styled(
                pad(&row, width),
                theme::selection(),
            )));
        } else {
            lines.push(Line::from(vec![
                Span::styled(" ●", Style::default().fg(dot_color)),
                Span::styled(row, theme::text()),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_config(frame: &mut Frame, area: Rect, app: &App, panel: &Panel) {
    let text = match app.selected_agent() {
        Some(agent) if !agent.effective_config.is_empty() => agent.effective_config.clone(),
        Some(_) => "(agent has not reported an effective config yet)".to_string(),
        None => "(no agent selected)".to_string(),
    };
    let lines: Vec<Line> = text
        .lines()
        .map(|l| Line::from(Span::styled(l.to_string(), theme::text())))
        .collect();
    frame.render_widget(
        Paragraph::new(Text::from(lines)).scroll((panel.scroll, 0)),
        area,
    );
}

fn render_pipeline(frame: &mut Frame, area: Rect, app: &App, panel: &Panel) {
    let Some(agent) = app.selected_agent() else {
        render_placeholder(frame, area, "(no agent selected)");
        return;
    };
    let graph = match pipeline::parse(&agent.effective_config) {
        Ok(graph) => graph,
        Err(e) => {
            render_placeholder(frame, area, &format!("config parse error: {e}"));
            return;
        }
    };

    let mut lines: Vec<Line> = vec![Line::from(vec![
        Span::styled("receivers", Style::default().fg(theme::NODE_RECEIVER)),
        Span::raw("  "),
        Span::styled("processors", Style::default().fg(theme::NODE_PROCESSOR)),
        Span::raw("  "),
        Span::styled("exporters", Style::default().fg(theme::NODE_EXPORTER)),
        Span::raw("  "),
        Span::styled("connectors ⇄", Style::default().fg(theme::NODE_CONNECTOR)),
    ])];
    lines.push(Line::raw(""));

    if graph.pipelines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no service.pipelines found in config)",
            theme::dim(),
        )));
    }

    for pl in &graph.pipelines {
        lines.push(Line::from(Span::styled(
            format!("▣ {}", pl.name),
            theme::accent(),
        )));
        let mut flow: Vec<Span> = vec![Span::raw("   ")];
        flow.extend(node_spans(&pl.receivers, &graph, theme::NODE_RECEIVER));
        flow.push(Span::styled("  ──▶  ", theme::dim()));
        if pl.processors.is_empty() {
            flow.push(Span::styled("(none)", theme::dim()));
        } else {
            flow.extend(node_spans(&pl.processors, &graph, theme::NODE_PROCESSOR));
        }
        flow.push(Span::styled("  ──▶  ", theme::dim()));
        flow.extend(node_spans(&pl.exporters, &graph, theme::NODE_EXPORTER));
        lines.push(Line::from(flow));
        lines.push(Line::raw(""));
    }

    let bridges = graph.bridges();
    if !bridges.is_empty() {
        lines.push(Line::from(Span::styled(
            "connector bridges",
            theme::accent(),
        )));
        for (connector, from, to) in bridges {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(connector, Style::default().fg(theme::NODE_CONNECTOR)),
                Span::styled(format!("  {from} ▶ {to}"), theme::dim()),
            ]));
        }
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines)).scroll((panel.scroll, 0)),
        area,
    );
}

fn node_spans<'a>(
    names: &'a [String],
    graph: &pipeline::PipelineGraph,
    color: ratatui::style::Color,
) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", theme::dim()));
        }
        if graph.is_connector(name) {
            spans.push(Span::styled(
                format!("{name}⇄"),
                Style::default().fg(theme::NODE_CONNECTOR),
            ));
        } else {
            spans.push(Span::styled(name.as_str(), Style::default().fg(color)));
        }
    }
    spans
}

fn render_metrics(frame: &mut Frame, area: Rect, app: &App, panel: &Panel) {
    let Some(telemetry) = app.selected_telemetry() else {
        render_placeholder(
            frame,
            area,
            "(no own-telemetry received yet — agents push metrics over OTLP)",
        );
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                " {} metric series · {} spans observed",
                telemetry.metrics.len(),
                telemetry.span_count
            ),
            theme::accent(),
        )),
        Line::raw(""),
    ];
    for metric in &telemetry.metrics {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:<46}", truncate(&metric.name, 46)),
                theme::text(),
            ),
            Span::styled(format!("{:>14}", fmt_num(metric.value)), theme::accent()),
            Span::styled(format!("  {}", metric.unit), theme::dim()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines)).scroll((panel.scroll, 0)),
        area,
    );
}

fn render_logs(frame: &mut Frame, area: Rect, app: &App, panel: &Panel) {
    let Some(telemetry) = app.selected_telemetry() else {
        render_placeholder(
            frame,
            area,
            "(no own-telemetry received yet — agents push logs over OTLP)",
        );
        return;
    };
    if telemetry.logs.is_empty() {
        render_placeholder(frame, area, "(no log records received yet)");
        return;
    }
    let lines: Vec<Line> = telemetry
        .logs
        .iter()
        .map(|log| {
            Line::from(vec![
                Span::styled(format!(" {} ", fmt_time(log.time_unix_nano)), theme::dim()),
                Span::styled(
                    format!("{:<6}", truncate(&log.severity, 6)),
                    Style::default().fg(severity_color(&log.severity)),
                ),
                Span::styled(format!(" {}", log.body), theme::text()),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(Text::from(lines)).scroll((panel.scroll, 0)),
        area,
    );
}

fn render_health(frame: &mut Frame, area: Rect, app: &App, panel: &Panel) {
    let Some(agent) = app.selected_agent() else {
        render_placeholder(frame, area, "(no agent selected)");
        return;
    };
    let mut lines = Vec::new();

    lines.push(section("Identity"));
    for (k, v) in &agent.identifying {
        lines.push(kv_line(k, v));
    }
    for (k, v) in &agent.non_identifying {
        lines.push(kv_line(k, v));
    }
    lines.push(kv_line("instance.uid", &agent.uid));
    lines.push(kv_line("uptime", &uptime(agent.start_time_unix_nano)));
    lines.push(kv_line(
        "seen by otelc",
        &format!("{} ago", ago(agent.connected_at)),
    ));
    lines.push(kv_line("sequence", &agent.sequence_num.to_string()));
    if let Some(remote) = &agent.remote_status {
        let detail = if remote.error.is_empty() {
            remote.state.clone()
        } else {
            format!("{} — {}", remote.state, remote.error)
        };
        lines.push(kv_line("remote config", &detail));
    }

    lines.push(Line::raw(""));
    lines.push(section("Capabilities"));
    for (name, enabled) in &agent.capabilities {
        let mark = if *enabled { "[x]" } else { "[ ]" };
        let color = if *enabled { theme::OK } else { theme::DIM };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(mark, Style::default().fg(color)),
            Span::styled(format!(" {name}"), theme::text()),
        ]));
    }

    lines.push(Line::raw(""));
    lines.push(section("Component health"));
    match &agent.health {
        Some(node) => health_lines(node, 0, &mut lines),
        None => lines.push(Line::from(Span::styled(
            "  (no health reported)",
            theme::dim(),
        ))),
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines)).scroll((panel.scroll, 0)),
        area,
    );
}

fn health_lines(node: &HealthNode, depth: usize, out: &mut Vec<Line<'static>>) {
    let indent = "  ".repeat(depth + 1);
    let mut spans = vec![
        Span::raw(indent),
        Span::styled("●", Style::default().fg(theme::health(node.healthy))),
        Span::styled(format!(" {}", node.name), theme::text()),
    ];
    if !node.status.is_empty() {
        spans.push(Span::styled(format!("  [{}]", node.status), theme::dim()));
    }
    if !node.last_error.is_empty() {
        spans.push(Span::styled(
            format!("  {}", node.last_error),
            Style::default().fg(theme::ERR),
        ));
    }
    out.push(Line::from(spans));
    for child in &node.children {
        health_lines(child, depth + 1, out);
    }
}

fn render_placeholder(frame: &mut Frame, area: Rect, text: &str) {
    frame.render_widget(
        Paragraph::new(Span::styled(text.to_string(), theme::dim())).wrap(Wrap { trim: true }),
        area,
    );
}

fn section(name: &str) -> Line<'static> {
    Line::from(Span::styled(format!(" {name}"), theme::accent()))
}

fn kv_line(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key:<22}"), theme::dim()),
        Span::styled(value.to_string(), theme::text()),
    ])
}

fn status_text(agent: &AgentDetail) -> &str {
    if !agent.status.is_empty() {
        &agent.status
    } else if agent.healthy {
        "Healthy"
    } else {
        "Unhealthy"
    }
}

fn severity_color(severity: &str) -> ratatui::style::Color {
    match severity {
        "ERROR" | "FATAL" => theme::ERR,
        "WARN" => theme::WARN,
        "INFO" => theme::OK,
        _ => theme::DIM,
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max.saturating_sub(1)).collect();
        s.push('…');
        s
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

fn fmt_num(v: f64) -> String {
    if v.abs() >= 1e9 {
        format!("{:.2}G", v / 1e9)
    } else if v.abs() >= 1e6 {
        format!("{:.2}M", v / 1e6)
    } else if v.abs() >= 1e3 {
        format!("{:.1}k", v / 1e3)
    } else if (v.fract()).abs() < 0.01 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}

fn ago(instant: Instant) -> String {
    let secs = Instant::now().saturating_duration_since(instant).as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

fn uptime(start_unix_nano: u64) -> String {
    if start_unix_nano == 0 {
        return "unknown".to_string();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let secs = now.saturating_sub(start_unix_nano) / 1_000_000_000;
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn fmt_time(unix_nano: u64) -> String {
    let secs = unix_nano / 1_000_000_000;
    let tod = secs % 86_400;
    format!("{:02}:{:02}:{:02}", tod / 3600, (tod % 3600) / 60, tod % 60)
}
