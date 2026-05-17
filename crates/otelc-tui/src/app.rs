//! Application state, the event loop, and key handling.

use crate::control::{AgentDetail, ControlEvent, ControlPlane, TelemetrySnapshot};
use crate::ui;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;

/// What a panel is currently showing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Fleet,
    Config,
    Pipeline,
    Metrics,
    Logs,
    Health,
}

impl ViewMode {
    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Fleet => "Fleet",
            ViewMode::Config => "Config",
            ViewMode::Pipeline => "Pipeline",
            ViewMode::Metrics => "Metrics",
            ViewMode::Logs => "Logs",
            ViewMode::Health => "Health",
        }
    }

    pub const ALL: [ViewMode; 6] = [
        ViewMode::Fleet,
        ViewMode::Config,
        ViewMode::Pipeline,
        ViewMode::Metrics,
        ViewMode::Logs,
        ViewMode::Health,
    ];

    fn from_digit(c: char) -> Option<ViewMode> {
        let idx = c.to_digit(10)? as usize;
        if (1..=6).contains(&idx) {
            Some(ViewMode::ALL[idx - 1])
        } else {
            None
        }
    }

    fn next(self) -> ViewMode {
        let idx = ViewMode::ALL.iter().position(|v| *v == self).unwrap_or(0);
        ViewMode::ALL[(idx + 1) % ViewMode::ALL.len()]
    }
}

/// Which panel is which.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// One of the two Norton Commander panels.
pub struct Panel {
    pub view: ViewMode,
    pub scroll: u16,
}

/// A modal dialog drawn over the panels.
pub enum Modal {
    Help,
    Message {
        title: String,
        body: String,
    },
    Confirm {
        title: String,
        body: String,
        uid: String,
    },
    Editor(Editor),
}

/// The inline remote-config editor.
pub struct Editor {
    pub uid: String,
    pub agent: String,
    pub lines: Vec<String>,
    pub cx: usize,
    pub cy: usize,
    pub scroll: usize,
}

impl Editor {
    fn new(uid: String, agent: String, content: &str) -> Self {
        let mut lines: Vec<String> = content.split('\n').map(str::to_string).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self {
            uid,
            agent,
            lines,
            cx: 0,
            cy: 0,
            scroll: 0,
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn line_len(&self, row: usize) -> usize {
        self.lines.get(row).map(String::len).unwrap_or(0)
    }

    fn insert_char(&mut self, c: char) {
        let line = &mut self.lines[self.cy];
        let idx = byte_index(line, self.cx);
        line.insert(idx, c);
        self.cx += 1;
    }

    fn newline(&mut self) {
        let line = &mut self.lines[self.cy];
        let idx = byte_index(line, self.cx);
        let rest = line.split_off(idx);
        self.lines.insert(self.cy + 1, rest);
        self.cy += 1;
        self.cx = 0;
    }

    fn backspace(&mut self) {
        if self.cx > 0 {
            let line = &mut self.lines[self.cy];
            let idx = byte_index(line, self.cx - 1);
            line.remove(idx);
            self.cx -= 1;
        } else if self.cy > 0 {
            let current = self.lines.remove(self.cy);
            self.cy -= 1;
            self.cx = self.lines[self.cy].chars().count();
            self.lines[self.cy].push_str(&current);
        }
    }

    fn move_cursor(&mut self, dx: i32, dy: i32) {
        if dy != 0 {
            let new_y = (self.cy as i32 + dy).clamp(0, self.lines.len() as i32 - 1);
            self.cy = new_y as usize;
            self.cx = self.cx.min(self.lines[self.cy].chars().count());
        }
        if dx != 0 {
            let new_x = self.cx as i32 + dx;
            if new_x < 0 {
                if self.cy > 0 {
                    self.cy -= 1;
                    self.cx = self.line_len(self.cy);
                }
            } else if new_x as usize > self.line_len(self.cy) {
                if self.cy + 1 < self.lines.len() {
                    self.cy += 1;
                    self.cx = 0;
                }
            } else {
                self.cx = new_x as usize;
            }
        }
    }
}

fn byte_index(line: &str, char_idx: usize) -> usize {
    line.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}

/// A pull-down menu action.
#[derive(Clone)]
pub enum MenuAction {
    SetView(ViewMode),
    Edit,
    Restart,
    Filter,
    Help,
    Quit,
    Separator,
}

/// One menu row.
pub struct MenuEntry {
    pub label: String,
    pub action: MenuAction,
}

/// The open pull-down menu.
pub struct Menu {
    pub entries: Vec<MenuEntry>,
    pub selected: usize,
}

impl Menu {
    fn new() -> Self {
        let entries = vec![
            entry(
                "Fleet view          1",
                MenuAction::SetView(ViewMode::Fleet),
            ),
            entry(
                "Config view         2",
                MenuAction::SetView(ViewMode::Config),
            ),
            entry(
                "Pipeline view       3",
                MenuAction::SetView(ViewMode::Pipeline),
            ),
            entry(
                "Metrics view        4",
                MenuAction::SetView(ViewMode::Metrics),
            ),
            entry("Logs view           5", MenuAction::SetView(ViewMode::Logs)),
            entry(
                "Health view         6",
                MenuAction::SetView(ViewMode::Health),
            ),
            entry("", MenuAction::Separator),
            entry("Edit remote config F4", MenuAction::Edit),
            entry("Restart agent      F6", MenuAction::Restart),
            entry("Filter fleet       F7", MenuAction::Filter),
            entry("", MenuAction::Separator),
            entry("Help               F1", MenuAction::Help),
            entry("Quit              F10", MenuAction::Quit),
        ];
        Self {
            entries,
            selected: 0,
        }
    }

    fn step(&mut self, delta: i32) {
        let len = self.entries.len() as i32;
        let mut idx = self.selected as i32;
        loop {
            idx = (idx + delta).rem_euclid(len);
            if !matches!(self.entries[idx as usize].action, MenuAction::Separator) {
                break;
            }
        }
        self.selected = idx as usize;
    }
}

fn entry(label: &str, action: MenuAction) -> MenuEntry {
    MenuEntry {
        label: label.to_string(),
        action,
    }
}

/// The whole application.
pub struct App {
    pub control: Box<dyn ControlPlane>,
    pub agents: HashMap<String, AgentDetail>,
    pub telemetry: HashMap<String, TelemetrySnapshot>,
    pub order: Vec<String>,
    pub selected_uid: Option<String>,
    pub left: Panel,
    pub right: Panel,
    pub active: Side,
    pub modal: Option<Modal>,
    pub menu: Option<Menu>,
    pub filter: String,
    pub filtering: bool,
    pub status: String,
    pub should_quit: bool,
}

impl App {
    fn new(control: Box<dyn ControlPlane>) -> Self {
        let status = format!(
            "{} mode · {} · waiting for agents to connect…",
            control.mode(),
            control.endpoint()
        );
        Self {
            control,
            agents: HashMap::new(),
            telemetry: HashMap::new(),
            order: Vec::new(),
            selected_uid: None,
            left: Panel {
                view: ViewMode::Fleet,
                scroll: 0,
            },
            right: Panel {
                view: ViewMode::Config,
                scroll: 0,
            },
            active: Side::Left,
            modal: None,
            menu: None,
            filter: String::new(),
            filtering: false,
            status,
            should_quit: false,
        }
    }

    /// Agents matching the current filter, sorted by name.
    pub fn visible(&self) -> Vec<&AgentDetail> {
        let needle = self.filter.to_lowercase();
        let mut list: Vec<&AgentDetail> = self
            .order
            .iter()
            .filter_map(|uid| self.agents.get(uid))
            .filter(|a| {
                needle.is_empty()
                    || a.name.to_lowercase().contains(&needle)
                    || a.uid.to_lowercase().contains(&needle)
            })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// The currently selected agent.
    pub fn selected_agent(&self) -> Option<&AgentDetail> {
        let visible = self.visible();
        if let Some(uid) = &self.selected_uid {
            if let Some(found) = visible.iter().find(|a| &a.uid == uid) {
                return Some(found);
            }
        }
        visible.first().copied()
    }

    /// Telemetry for the selected agent.
    pub fn selected_telemetry(&self) -> Option<&TelemetrySnapshot> {
        let uid = &self.selected_agent()?.uid;
        self.telemetry.get(uid)
    }

    pub fn active_panel(&self) -> &Panel {
        match self.active {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    fn active_panel_mut(&mut self) -> &mut Panel {
        match self.active {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    fn handle_control(&mut self, event: ControlEvent) {
        match event {
            ControlEvent::AgentUpserted(detail) => {
                if !self.agents.contains_key(&detail.uid) {
                    self.order.push(detail.uid.clone());
                    if self.selected_uid.is_none() {
                        self.selected_uid = Some(detail.uid.clone());
                    }
                    self.status = format!("agent {} connected", detail.name);
                }
                self.agents.insert(detail.uid.clone(), *detail);
            }
            ControlEvent::AgentDisconnected(uid) => {
                self.agents.remove(&uid);
                self.order.retain(|u| u != &uid);
                self.telemetry.remove(&uid);
                if self.selected_uid.as_ref() == Some(&uid) {
                    self.selected_uid = self.order.first().cloned();
                }
                self.status = "an agent disconnected".to_string();
            }
            ControlEvent::Telemetry(uid, snapshot) => {
                self.telemetry.insert(uid, snapshot);
            }
            ControlEvent::Notice(msg) => {
                self.status = msg;
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if self.modal.is_some() {
            self.handle_modal_key(key);
        } else if self.menu.is_some() {
            self.handle_menu_key(key);
        } else if self.filtering {
            self.handle_filter_key(key);
        } else {
            self.handle_global_key(key);
        }
    }

    fn handle_global_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::F(10) => self.should_quit = true,
            KeyCode::F(1) => self.modal = Some(Modal::Help),
            KeyCode::F(2) | KeyCode::F(9) => self.menu = Some(Menu::new()),
            KeyCode::F(3) => {
                let panel = self.active_panel_mut();
                panel.view = panel.view.next();
                panel.scroll = 0;
            }
            KeyCode::F(4) | KeyCode::F(5) => self.open_editor(),
            KeyCode::F(6) => self.confirm_restart(),
            KeyCode::F(7) => {
                self.filtering = true;
                self.status = "filter: type to match, Enter to apply, Esc to clear".into();
            }
            KeyCode::F(8) => {
                self.right.view = ViewMode::Health;
                self.right.scroll = 0;
                self.active = Side::Right;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.active = match self.active {
                    Side::Left => Side::Right,
                    Side::Right => Side::Left,
                };
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                if let Some(view) = ViewMode::from_digit(c) {
                    let panel = self.active_panel_mut();
                    panel.view = view;
                    panel.scroll = 0;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.navigate(-1),
            KeyCode::Down | KeyCode::Char('j') => self.navigate(1),
            KeyCode::PageUp => self.navigate(-10),
            KeyCode::PageDown => self.navigate(10),
            KeyCode::Enter => {
                if self.active_panel().view == ViewMode::Fleet {
                    self.right.view = ViewMode::Config;
                    self.right.scroll = 0;
                    self.active = Side::Right;
                }
            }
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.status = "filter cleared".into();
                }
            }
            _ => {}
        }
    }

    fn navigate(&mut self, delta: i32) {
        if self.active_panel().view == ViewMode::Fleet {
            self.move_selection(delta);
        } else {
            let panel = self.active_panel_mut();
            let next = panel.scroll as i32 + delta;
            panel.scroll = next.max(0) as u16;
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let visible = self.visible();
        if visible.is_empty() {
            return;
        }
        let current = self
            .selected_uid
            .as_ref()
            .and_then(|uid| visible.iter().position(|a| &a.uid == uid))
            .unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, visible.len() as i32 - 1) as usize;
        self.selected_uid = Some(visible[next].uid.clone());
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => self.filter.push(c),
            KeyCode::Backspace => {
                self.filter.pop();
            }
            KeyCode::Enter => {
                self.filtering = false;
                self.status = format!("filter applied: '{}'", self.filter);
            }
            KeyCode::Esc => {
                self.filtering = false;
                self.filter.clear();
                self.status = "filter cleared".into();
            }
            _ => {}
        }
    }

    fn handle_menu_key(&mut self, key: KeyEvent) {
        let Some(menu) = self.menu.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => menu.step(-1),
            KeyCode::Down | KeyCode::Char('j') => menu.step(1),
            KeyCode::Esc | KeyCode::F(2) | KeyCode::F(9) => self.menu = None,
            KeyCode::Enter => {
                let action = menu.entries[menu.selected].action.clone();
                self.menu = None;
                self.apply_menu_action(action);
            }
            _ => {}
        }
    }

    fn apply_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::SetView(view) => {
                let panel = self.active_panel_mut();
                panel.view = view;
                panel.scroll = 0;
            }
            MenuAction::Edit => self.open_editor(),
            MenuAction::Restart => self.confirm_restart(),
            MenuAction::Filter => self.filtering = true,
            MenuAction::Help => self.modal = Some(Modal::Help),
            MenuAction::Quit => self.should_quit = true,
            MenuAction::Separator => {}
        }
    }

    fn handle_modal_key(&mut self, key: KeyEvent) {
        match self.modal.as_mut() {
            Some(Modal::Help) | Some(Modal::Message { .. }) => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char(_)) {
                    self.modal = None;
                }
            }
            Some(Modal::Confirm { uid, .. }) => match key.code {
                KeyCode::Enter | KeyCode::Char('y') => {
                    let uid = uid.clone();
                    self.modal = None;
                    self.do_restart(&uid);
                }
                KeyCode::Esc | KeyCode::Char('n') => self.modal = None,
                _ => {}
            },
            Some(Modal::Editor(editor)) => match key.code {
                KeyCode::Esc => {
                    self.modal = None;
                    self.status = "edit cancelled".into();
                }
                KeyCode::F(5) => {
                    let (uid, text) = (editor.uid.clone(), editor.text());
                    self.modal = None;
                    self.do_push(&uid, &text);
                }
                KeyCode::Char(c) => editor.insert_char(c),
                KeyCode::Enter => editor.newline(),
                KeyCode::Backspace => editor.backspace(),
                KeyCode::Left => editor.move_cursor(-1, 0),
                KeyCode::Right => editor.move_cursor(1, 0),
                KeyCode::Up => editor.move_cursor(0, -1),
                KeyCode::Down => editor.move_cursor(0, 1),
                KeyCode::Home => editor.cx = 0,
                KeyCode::End => editor.cx = editor.line_len(editor.cy),
                _ => {}
            },
            None => {}
        }
    }

    fn open_editor(&mut self) {
        let Some(agent) = self.selected_agent() else {
            self.status = "no agent selected".into();
            return;
        };
        if !agent_supports(agent, "AcceptsRemoteConfig") {
            self.status = format!("{} does not accept remote config", agent.name);
            return;
        }
        self.modal = Some(Modal::Editor(Editor::new(
            agent.uid.clone(),
            agent.name.clone(),
            &agent.effective_config,
        )));
    }

    fn confirm_restart(&mut self) {
        let Some(agent) = self.selected_agent() else {
            self.status = "no agent selected".into();
            return;
        };
        if !agent_supports(agent, "AcceptsRestartCommand") {
            self.status = format!("{} does not accept restart commands", agent.name);
            return;
        }
        self.modal = Some(Modal::Confirm {
            title: "Restart agent".into(),
            body: format!("Send a restart command to {}?", agent.name),
            uid: agent.uid.clone(),
        });
    }

    fn do_restart(&mut self, uid: &str) {
        let name = self.agent_name(uid);
        match self.control.restart(uid) {
            Ok(()) => self.status = format!("restart command sent to {name}"),
            Err(e) => self.modal = Some(error_modal("Restart failed", &e)),
        }
    }

    fn do_push(&mut self, uid: &str, yaml: &str) {
        let name = self.agent_name(uid);
        match self.control.push_config(uid, yaml) {
            Ok(()) => self.status = format!("remote config pushed to {name}"),
            Err(e) => self.modal = Some(error_modal("Config push failed", &e)),
        }
    }

    fn agent_name(&self, uid: &str) -> String {
        self.agents
            .get(uid)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| uid.to_string())
    }
}

fn agent_supports(agent: &AgentDetail, capability: &str) -> bool {
    agent
        .capabilities
        .iter()
        .any(|(name, enabled)| name == capability && *enabled)
}

fn error_modal(title: &str, body: &str) -> Modal {
    Modal::Message {
        title: title.to_string(),
        body: body.to_string(),
    }
}

/// Run the TUI event loop until the user quits.
pub async fn run(
    control: Box<dyn ControlPlane>,
    mut control_rx: mpsc::Receiver<ControlEvent>,
) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let (input_tx, mut input_rx) = mpsc::channel::<Event>(128);
    std::thread::spawn(move || {
        while let Ok(event) = event::read() {
            if input_tx.blocking_send(event).is_err() {
                break;
            }
        }
    });

    let mut app = App::new(control);
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let result = loop {
        if let Err(e) = terminal.draw(|frame| ui::draw(frame, &mut app)) {
            break Err(e.into());
        }
        if app.should_quit {
            break Ok(());
        }
        tokio::select! {
            maybe_event = input_rx.recv() => {
                match maybe_event {
                    Some(Event::Key(key)) if key.kind == KeyEventKind::Press => app.handle_key(key),
                    Some(_) => {}
                    None => break Ok(()),
                }
            }
            maybe_control = control_rx.recv() => {
                if let Some(event) = maybe_control {
                    app.handle_control(event);
                }
            }
            _ = tick.tick() => {}
        }
    };

    ratatui::restore();
    result
}
