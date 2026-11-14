//! Cross-platform full-screen terminal client.
//! The model worker never reads from the terminal: every mutation is approved
//! through a typed UI event with default-deny on disconnect.
use anyhow::{Context, Result};
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    io::{self, IsTerminal},
    sync::mpsc::{self, Sender},
    thread,
    time::Duration,
};

use crate::{
    agent::{Agent, AgentEvent, ApprovalMode},
    config::Settings,
    tools::ApprovalRequest,
};

const BLUE: Color = Color::Rgb(25, 170, 232);
const SURFACE: Color = Color::Rgb(9, 20, 34);
const GREEN: Color = Color::Rgb(111, 218, 178);
const MUTED: Color = Color::Rgb(139, 170, 191);
const MAX_INPUT_CHARS: usize = 4096;
const MAX_LOG_LINES: usize = 600;

enum Work {
    Prompt(String),
    Slash(String),
}

struct ModelPicker {
    choices: Vec<crate::models::ModelChoice>,
    query: String,
    selected: usize,
    source: String,
}
impl ModelPicker {
    fn filtered(&self) -> Vec<&crate::models::ModelChoice> {
        let query = self.query.to_lowercase();
        self.choices
            .iter()
            .filter(|c| {
                c.id.to_lowercase().contains(&query) || c.name.to_lowercase().contains(&query)
            })
            .collect()
    }
    fn current(&self) -> Option<String> {
        self.filtered().get(self.selected).map(|c| c.id.clone())
    }
    fn move_by(&mut self, down: bool) {
        let count = self.filtered().len();
        if count == 0 {
            self.selected = 0;
        } else if down {
            self.selected = (self.selected + 1).min(count - 1);
        } else {
            self.selected = self.selected.saturating_sub(1);
        }
    }
}

struct Screen {
    lines: Vec<String>,
    input: String,
    busy: bool,
    approval: Option<ApprovalRequest>,
    picker: Option<ModelPicker>,
    approval_scroll: u16,
    scroll_up: usize,
    status: String,
    model: String,
    workspace: String,
    cursor: usize,
    draft: String,
    plan: String,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Screen {
    fn new(model: String, workspace: String) -> Self {
        Self {
            lines: vec![
                "Blue Dragon interactive terminal".into(),
                "Enter a coding task, or use /help. Nothing is modified without approval.".into(),
            ],
            input: String::new(),
            busy: false,
            approval: None,
            picker: None,
            approval_scroll: 0,
            scroll_up: 0,
            status: "Ready".into(),
            model,
            workspace,
            cursor: 0,
            draft: String::new(),
            plan: "Use /plan or ask for a multi-step task.".into(),
            cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    fn log(&mut self, label: &str, message: &str) {
        let clean = message
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .take(30_000)
            .collect::<String>();
        for (i, line) in clean.lines().enumerate().take(200) {
            if i == 0 {
                self.lines.push(format!("{label} {line}"));
            } else {
                self.lines.push(format!("  {line}"));
            }
        }
        if self.lines.len() > MAX_LOG_LINES {
            self.lines.drain(..self.lines.len() - MAX_LOG_LINES);
        }
        self.scroll_up = 0;
    }

    fn submit(&mut self, tx: &Sender<Work>) {
        if self.busy {
            return;
        }
        let text = self.input.trim().to_string();
        self.input.clear();
        self.cursor = 0;
        if text.is_empty() {
            return;
        }
        if text == "/help" {
            self.log(
                "System:",
                "Commands: /help /browser [status|start|pair|attach PORT|launch|stop] /rotation /context /findings /memory [query|forget ID] /checks /models /model [PROVIDER MODEL] /clear /exit /map /status /diff /session /skills /skill <name> /plan /report /mcp /connect <name> /doctor. Ctrl+C cancels; Esc exits.",
            );
            return;
        }
        if text == "/clear" {
            self.lines.clear();
            self.status = "History view cleared; session is preserved".into();
            return;
        }
        let command = if text.starts_with('/') {
            Work::Slash(text.clone())
        } else {
            Work::Prompt(text.clone())
        };
        self.log("You:", &text);
        self.cancelled
            .store(false, std::sync::atomic::Ordering::Relaxed);
        if tx.send(command).is_ok() {
            self.busy = true;
            self.status = "Working (approval may be requested)".into();
        } else {
            self.status = "Agent worker stopped".into();
        }
    }

    fn approve(&mut self, accepted: bool) {
        if let Some(request) = self.approval.take() {
            self.approval_scroll = 0;
            let _ = request.respond_to.send(accepted);
            self.log(
                "Approval:",
                if accepted {
                    "Approved one action"
                } else {
                    "Denied action"
                },
            );
            self.status = if accepted {
                "Action approved; working".into()
            } else {
                "Action denied; working".into()
            };
        }
    }
}

struct TerminalGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen);
    }
}

pub fn run(settings: Settings, read_only: bool, resume: Option<&str>) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        anyhow::bail!("TUI needs an interactive terminal. Use 'blue-dragon chat' or 'run' instead");
    }
    let mut app = Screen::new(settings.model.clone(), settings.root.display().to_string());

    let (work_tx, work_rx) = mpsc::channel::<Work>();
    let (message_tx, message_rx) = mpsc::sync_channel::<AgentEvent>(256);
    let (approval_tx, approval_rx) = mpsc::channel::<ApprovalRequest>();
    let saved_resume = resume.map(str::to_owned);
    let cancellation = app.cancelled.clone();

    thread::spawn(move || {
        let approval = if read_only {
            ApprovalMode::ReadOnly
        } else {
            ApprovalMode::Tui(approval_tx)
        };
        let mut agent = match Agent::new(settings, approval, saved_resume.as_deref()) {
            Ok(agent) => agent,
            Err(error) => {
                let _ = message_tx.send(AgentEvent::Finished(Err(format!(
                    "Unable to start agent: {error:#}"
                ))));
                return;
            }
        };
        agent.set_cancellation(cancellation);
        agent.set_output(message_tx.clone());
        let _ = message_tx.send(AgentEvent::Model(agent.model_status()));
        let _ = message_tx.send(AgentEvent::Plan(agent.plan_text()));
        for work in work_rx {
            let result = match work {
                Work::Prompt(prompt) => agent.turn(&prompt),
                Work::Slash(line) => run_slash(&mut agent, &message_tx, &line),
            };
            let status = result.map_err(|error| format!("{error:#}"));
            if message_tx.send(AgentEvent::Finished(status)).is_err() {
                break;
            }
        }
    });

    enable_raw_mode().context("Could not enable terminal raw mode")?;
    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableBracketedPaste) {
        let _ = disable_raw_mode();
        return Err(error.into());
    }
    let _guard = TerminalGuard(app.cancelled.clone());
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Could not initialize TUI")?;

    loop {
        for _ in 0..256 {
            let Ok(message) = message_rx.try_recv() else {
                break;
            };
            match message {
                AgentEvent::Answer(text) => {
                    app.draft.clear();
                    app.log("Dragon:", &text);
                }
                AgentEvent::Delta(text) => {
                    app.draft.extend(
                        text.chars()
                            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t'),
                    );
                    if app.draft.len() > 100_000 {
                        app.draft = app
                            .draft
                            .chars()
                            .rev()
                            .take(30_000)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                    }
                }
                AgentEvent::ModelChoices(choices, source) => {
                    app.picker = Some(ModelPicker {
                        choices,
                        source,
                        query: String::new(),
                        selected: 0,
                    });
                    app.status =
                        "Choose free model: arrows, type to filter, Enter select, Esc close".into();
                }
                AgentEvent::Notice(text) => app.log("System:", &text),
                AgentEvent::Model(text) => app.model = text,
                AgentEvent::Plan(text) => app.plan = text,
                AgentEvent::ToolStart(name) => {
                    app.log("Tool:", &name);
                    app.status = "Tool running".into();
                }
                AgentEvent::Finished(result) => {
                    if !app.draft.is_empty() {
                        let partial = std::mem::take(&mut app.draft);
                        app.log("Partial:", &partial);
                    }
                    app.busy = false;
                    match result {
                        Ok(()) => app.status = "Ready".into(),
                        Err(error) => {
                            app.log("Error:", &error);
                            app.status = "Task failed; see transcript".into();
                        }
                    }
                }
                AgentEvent::ToolResult(ok) => {
                    app.log("Result:", if ok { "Completed" } else { "Failed or denied" });
                }
            }
        }
        while let Ok(request) = approval_rx.try_recv() {
            if !app.busy || app.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = request.respond_to.send(false);
                continue;
            }
            if app.approval.is_some() {
                let _ = request.respond_to.send(false);
                app.log("System:", "Concurrent approval request rejected");
            } else {
                app.status = "Approval required (Y/N, arrows to scroll)".into();
                app.approval_scroll = 0;
                app.approval = Some(request);
            }
        }

        terminal.draw(|frame| draw(frame, &app))?;

        if !event::poll(Duration::from_millis(60))? {
            continue;
        }
        let event = event::read()?;
        if let Event::Paste(text) = &event {
            if let Some(picker) = &mut app.picker {
                picker.query.extend(
                    text.chars()
                        .filter(|c| !c.is_control())
                        .take(100usize.saturating_sub(picker.query.chars().count())),
                );
                picker.selected = 0;
            } else if app.approval.is_none() {
                for ch in text
                    .chars()
                    .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                    .take(MAX_INPUT_CHARS.saturating_sub(app.input.chars().count()))
                {
                    let byte = app
                        .input
                        .char_indices()
                        .nth(app.cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(app.input.len());
                    app.input.insert(byte, ch);
                    app.cursor += 1;
                }
            }
            continue;
        }
        let Event::Key(key) = event else {
            continue;
        };
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if app.busy {
                app.cancelled
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                app.approve(false);
                app.status = "Cancellation requested; waiting for current I/O".into();
                continue;
            }
            break;
        }
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if let Some(picker) = &mut app.picker {
            match key.code {
                KeyCode::Esc => {
                    app.picker = None;
                    app.status = "Ready".into();
                }
                KeyCode::Up => picker.move_by(false),
                KeyCode::Down => picker.move_by(true),
                KeyCode::Backspace => {
                    picker.query.pop();
                    picker.selected = 0;
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && picker.query.chars().count() < 100 =>
                {
                    picker.query.push(c);
                    picker.selected = 0;
                }
                KeyCode::Enter if !app.busy => {
                    if let Some(id) = picker.current() {
                        app.picker = None;
                        app.cancelled
                            .store(false, std::sync::atomic::Ordering::Relaxed);
                        if work_tx
                            .send(Work::Slash(format!("/model kilo {id}")))
                            .is_ok()
                        {
                            app.busy = true;
                            app.status = "Selecting model".into();
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        if app.approval.is_some() {
            match key.code {
                KeyCode::Char('y' | 'Y') => app.approve(true),
                KeyCode::Char('n' | 'N') | KeyCode::Esc => app.approve(false),
                KeyCode::PageDown | KeyCode::Down => {
                    app.approval_scroll = app.approval_scroll.saturating_add(3);
                }
                KeyCode::PageUp | KeyCode::Up => {
                    app.approval_scroll = app.approval_scroll.saturating_sub(3);
                }
                _ => {}
            }
            continue;
        }
        let can_type = !key.modifiers.contains(KeyModifiers::CONTROL)
            && app.input.chars().count() < MAX_INPUT_CHARS;
        match key.code {
            KeyCode::Esc => {
                app.cancelled
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                break;
            }
            KeyCode::PageUp => app.scroll_up = app.scroll_up.saturating_add(5),
            KeyCode::PageDown => app.scroll_up = app.scroll_up.saturating_sub(5),
            KeyCode::Enter => {
                if app.input.trim() == "/exit" || app.input.trim() == "/quit" {
                    break;
                }
                app.submit(&work_tx);
            }
            KeyCode::Left => app.cursor = app.cursor.saturating_sub(1),
            KeyCode::Right => app.cursor = (app.cursor + 1).min(app.input.chars().count()),
            KeyCode::Home => app.cursor = 0,
            KeyCode::End => app.cursor = app.input.chars().count(),
            KeyCode::Backspace => {
                if app.cursor > 0 {
                    app.cursor -= 1;
                    let byte = app
                        .input
                        .char_indices()
                        .nth(app.cursor)
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    app.input.remove(byte);
                }
            }
            KeyCode::Delete => {
                if let Some((byte, _)) = app.input.char_indices().nth(app.cursor) {
                    app.input.remove(byte);
                }
            }
            KeyCode::Char(ch) if can_type => {
                let byte = app
                    .input
                    .char_indices()
                    .nth(app.cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(app.input.len());
                app.input.insert(byte, ch);
                app.cursor += 1;
            }
            _ => {}
        }
    }
    app.cancelled
        .store(true, std::sync::atomic::Ordering::Relaxed);
    app.approve(false);
    drop(work_tx);
    Ok(())
}

fn run_slash(agent: &mut Agent, tx: &mpsc::SyncSender<AgentEvent>, line: &str) -> Result<()> {
    if matches!(line, "/model" | "/models" | "/models refresh") {
        let (choices, source) = agent.kilo_models(line == "/models refresh")?;
        let _ = tx.send(AgentEvent::ModelChoices(choices, source));
        return Ok(());
    }
    let response = match line {
        "/models openrouter" => crate::models::refresh()?,
        _ if line.starts_with("/model ") => {
            agent.select_model(line.trim_start_matches("/model "))?
        }
        "/browser" => serde_json::to_string_pretty(&agent.browser_command("status")?)?,
        _ if line.starts_with("/browser ") => serde_json::to_string_pretty(
            &agent.browser_command(line.trim_start_matches("/browser "))?,
        )?,
        "/checks" => serde_json::to_string_pretty(&agent.check_status())?,
        "/memory" => serde_json::to_string_pretty(&agent.memory_command("")?)?,
        _ if line.starts_with("/memory ") => serde_json::to_string_pretty(
            &agent.memory_command(line.trim_start_matches("/memory "))?,
        )?,
        "/context" => serde_json::to_string_pretty(&agent.context_status())?,
        "/findings" => agent.findings_text()?,
        "/rotation" => serde_json::to_string_pretty(&agent.rotation_status())?,
        "/plan" => agent.plan_text(),
        "/report" => agent.report_text(),
        "/mcp" => serde_json::to_string_pretty(&agent.mcp_list())?,
        _ if line.starts_with("/connect ") => serde_json::to_string_pretty(
            &agent.connect_mcp(line.trim_start_matches("/connect ").trim())?,
        )?,
        "/map" => serde_json::to_string_pretty(&agent.project_map()?)?,
        "/status" => serde_json::to_string_pretty(&agent.git_status()?)?,
        "/diff" => agent.git_diff()?,
        "/skills" => serde_json::to_string_pretty(&agent.list_skills()?)?,
        "/session" => agent.session_id().to_owned(),
        "/doctor" => format!(
            "Model: {} | workspace: {}",
            agent.model(),
            agent.root().display()
        ),
        _ if line.starts_with("/skill ") => {
            let name = line.trim_start_matches("/skill ").trim();
            agent.activate_skill(name)?;
            format!("Activated local skill: {name}")
        }
        _ => format!("Unknown slash command: {line}. Type /help."),
    };
    let _ = tx.send(AgentEvent::Answer(response));
    Ok(())
}

fn panel(title: &str) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(43, 72, 96)))
}

fn transcript_style(line: &str) -> Style {
    let color = if line.starts_with("You:") {
        BLUE
    } else if line.starts_with("Dragon:") {
        GREEN
    } else if line.starts_with("Error:") || line.starts_with("Partial:") {
        Color::Yellow
    } else if line.starts_with("Tool:") || line.starts_with("System:") {
        MUTED
    } else {
        Color::White
    };
    Style::default().fg(color)
}

fn transcript_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    text.lines()
        .flat_map(|line| {
            let style = transcript_style(line);
            let wrapped = if line.is_empty() {
                vec![String::new()]
            } else {
                wrap_text(line, width)
            };
            wrapped
                .into_iter()
                .map(move |part| Line::styled(part, style))
        })
        .collect()
}

fn draw(frame: &mut Frame<'_>, app: &Screen) {
    let compact = frame.area().width < 70 || frame.area().height < 22;
    frame.render_widget(
        Block::default().style(Style::default().bg(SURFACE)),
        frame.area(),
    );
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if compact { 1 } else { 3 }),
            Constraint::Min(1),
            Constraint::Length(if compact { 2 } else { 4 }),
            Constraint::Length(3),
        ])
        .split(frame.area());
    let state = if app.approval.is_some() {
        "APPROVAL"
    } else if app.busy {
        "WORKING"
    } else {
        "READY"
    };
    let title = if compact {
        format!(" BLUE DRAGON  /  {state}")
    } else {
        format!(" BLUE DRAGON  ◆  AI CODING WORKBENCH  /  {state} ")
    };
    let header =
        Paragraph::new(title).style(Style::default().fg(BLUE).add_modifier(Modifier::BOLD));
    frame.render_widget(
        if compact {
            header
        } else {
            header.alignment(Alignment::Center).block(panel(""))
        },
        outer[0],
    );

    let text = if app.draft.is_empty() {
        app.lines.join("\n")
    } else {
        format!("{}\nDragon: {}", app.lines.join("\n"), app.draft)
    };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if outer[1].width >= 110 {
            vec![Constraint::Percentage(73), Constraint::Percentage(27)]
        } else {
            vec![Constraint::Percentage(100), Constraint::Length(0)]
        })
        .split(outer[1]);
    if columns[1].width > 0 {
        frame.render_widget(
            Paragraph::new(app.plan.clone())
                .wrap(Wrap { trim: false })
                .block(panel(" Task plan "))
                .style(Style::default().fg(MUTED)),
            columns[1],
        );
    }
    let width = usize::from(columns[0].width.saturating_sub(2)).max(1);
    let visual = transcript_lines(&text, width);
    let visible_lines = columns[0].height.saturating_sub(2) as usize;
    let start = visual
        .len()
        .saturating_sub(visible_lines)
        .saturating_sub(app.scroll_up);
    let conversation_title = if app.scroll_up > 0 {
        " Conversation · history (PgDn) "
    } else {
        " Conversation · live "
    };
    frame.render_widget(
        Paragraph::new(
            visual
                .into_iter()
                .skip(start)
                .take(visible_lines)
                .collect::<Vec<_>>(),
        )
        .block(panel(conversation_title)),
        columns[0],
    );
    let status = if compact {
        format!(" {}\n {}", app.status, app.model)
    } else {
        format!(" {}  |  {}\n {}", app.status, app.model, app.workspace)
    };
    let status_widget = Paragraph::new(status).style(Style::default().fg(MUTED));
    frame.render_widget(
        if compact {
            status_widget
        } else {
            status_widget.block(panel(" Session "))
        },
        outer[2],
    );

    let prompt = if app.busy {
        " Draft · Ctrl+C cancel "
    } else if compact {
        " Enter send · /help "
    } else {
        " Enter send · /models · PgUp history · Esc exit "
    };
    frame.render_widget(
        Paragraph::new(visible_input(
            &app.input,
            app.cursor,
            usize::from(outer[3].width.saturating_sub(6)).max(1),
        ))
        .style(Style::default().fg(Color::White))
        .block(panel(prompt).border_style(Style::default().fg(BLUE))),
        outer[3],
    );

    if let Some(picker) = &app.picker {
        let popup = center(frame.area(), 94, 90);
        frame.render_widget(Clear, popup);
        let regions = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(1),
                Constraint::Length(4),
            ])
            .split(popup);
        frame.render_widget(
            Paragraph::new(format!("Filter: {}\n{}", picker.query, picker.source))
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .title(" FREE MODELS | Kilo | NO API KEY ")
                        .borders(Borders::ALL),
                )
                .style(Style::default().fg(BLUE)),
            regions[0],
        );
        let filtered = picker.filtered();
        let items: Vec<_> = filtered
            .iter()
            .map(|c| ListItem::new(format!("{}\n{}", c.name, c.id)))
            .collect();
        let empty = items.is_empty();
        let items = if empty {
            vec![ListItem::new("No matching models. Clear the filter.")
                .style(Style::default().fg(MUTED))]
        } else {
            items
        };
        let mut state = ListState::default();
        if !empty {
            state.select(Some(picker.selected.min(items.len() - 1)));
        }
        frame.render_stateful_widget(
            List::new(items)
                .block(
                    Block::default()
                        .title(" Up/Down choose; type filter ")
                        .borders(Borders::ALL),
                )
                .highlight_style(Style::default().fg(Color::Black).bg(BLUE))
                .highlight_symbol("> "),
            regions[1],
            &mut state,
        );
        let footer = if popup.width < 70 {
            "Enter saves / Esc closes\nPrompts may be logged. Public only."
        } else {
            "Arrows: choose | Enter: select & save | Esc: close\nCatalog is not a live test. Kilo/upstreams may log prompts & tool results. No confidential data."
        };
        frame.render_widget(
            Paragraph::new(footer)
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default().fg(MUTED)),
            regions[2],
        );
    }

    if let Some(request) = &app.approval {
        let popup = center(frame.area(), 96, 90);
        frame.render_widget(Clear, popup);
        let shell = panel(" APPROVAL REQUIRED ")
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().bg(SURFACE));
        let inner = shell.inner(popup);
        frame.render_widget(shell, popup);
        let regions = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(inner);
        let message = format!("{}\n\n{}", request.action, request.detail);
        let wrapped = wrap_text(&message, usize::from(regions[0].width).max(1));
        let max_scroll = wrapped.len().saturating_sub(usize::from(regions[0].height));
        let start = usize::from(app.approval_scroll).min(max_scroll);
        frame.render_widget(
            Paragraph::new(
                wrapped
                    .into_iter()
                    .skip(start)
                    .take(usize::from(regions[0].height))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            .style(Style::default().fg(Color::White)),
            regions[0],
        );
        frame.render_widget(
            Paragraph::new("[Y] approve one · [N/Esc] deny\nUp/Down scroll details")
                .style(Style::default().fg(Color::Yellow)),
            regions[1],
        );
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    let mut out = Vec::new();
    for line in text.lines() {
        let mut current = String::new();
        let mut used = 0;
        for ch in line.chars() {
            let size = ch.width().unwrap_or(0);
            if used + size > width && !current.is_empty() {
                out.push(std::mem::take(&mut current));
                used = 0;
            }
            current.push(ch);
            used += size;
        }
        out.push(current);
    }
    out
}
fn visible_input(input: &str, cursor: usize, width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let chars = input
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<Vec<_>>();
    let cursor = cursor.min(chars.len());
    let mut start = cursor;
    let mut used = 1;
    while start > 0 {
        let size = chars[start - 1].width().unwrap_or(0);
        if used + size > width {
            break;
        }
        used += size;
        start -= 1;
    }
    let mut result = String::from(" > ");
    result.extend(&chars[start..cursor]);
    result.push('▌');
    for ch in &chars[cursor..] {
        let size = ch.width().unwrap_or(0);
        if used + size > width {
            break;
        }
        used += size;
        result.push(*ch);
    }
    result
}

fn center(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Min(0),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Min(0),
        ])
        .split(vertical[1]);
    horizontal[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_picker_filters_concrete_choices_and_renders_on_mobile_width() {
        let mut picker = ModelPicker {
            choices: vec![
                crate::models::ModelChoice {
                    id: "kilo-auto/free".into(),
                    name: "Auto Free".into(),
                    context_length: 0,
                },
                crate::models::ModelChoice {
                    id: "liquid/code:free".into(),
                    name: "Liquid Code".into(),
                    context_length: 32000,
                },
            ],
            query: String::new(),
            selected: 0,
            source: "Offline fixture".into(),
        };
        picker.move_by(true);
        assert_eq!(picker.current().as_deref(), Some("liquid/code:free"));
        picker.query = "LIQUID".into();
        picker.selected = 0;
        assert_eq!(picker.filtered().len(), 1);
        picker.query = "absent".into();
        assert!(picker.current().is_none());
        picker.query.clear();
        for (width, height) in [(40, 14), (120, 32)] {
            let mut app = Screen::new("fixture".into(), ".".into());
            app.picker = Some(ModelPicker {
                choices: picker.choices.clone(),
                query: String::new(),
                selected: 1,
                source: "Offline fixture".into(),
            });
            let mut terminal =
                Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| draw(f, &app)).unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("FREE MODELS"));
            assert!(text.contains("Liquid Code"));
        }
    }

    #[test]
    fn small_and_wide_terminal_render_with_unicode_and_approval() {
        use ratatui::backend::TestBackend;
        for (width, height) in [(40, 14), (120, 32)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut app = Screen::new("test-model".into(), "workspace".into());
            app.input = "اقرأ ملف المشروع".into();
            app.cursor = app.input.chars().count();
            app.draft = "A streamed reply".into();
            terminal.draw(|frame| draw(frame, &app)).unwrap();
            let rendered = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(rendered.contains("BLUE DRAGON"));
            let (tx, _) = mpsc::channel();
            app.approval = Some(ApprovalRequest {
                action: "Write demo".into(),
                detail: "Only this change".into(),
                respond_to: tx,
            });
            terminal.draw(|frame| draw(frame, &app)).unwrap();
            let rendered = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(rendered.contains("APPROVAL REQUIRED"));
        }
        assert!(visible_input("1234567890末尾", 12, 7).ends_with('▌'));
        assert_eq!(wrap_text("abcdefgh", 3), vec!["abc", "def", "gh"]);
    }

    fn rendered_text(terminal: &Terminal<ratatui::backend::TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn transcript_preserves_blank_lines_between_paragraphs_and_code() {
        let lines = transcript_lines("Dragon: intro\n\nfirst()\n\nsecond()", 40);
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[1].to_string(), "");
        assert_eq!(lines[3].to_string(), "");
        assert_eq!(lines[4].to_string(), "second()");
    }

    #[test]
    fn compact_layout_keeps_status_model_and_prompt_visible() {
        let mut app = Screen::new("fixture-model".into(), "/long/workspace/path".into());
        app.status = "Ready".into();
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(40, 14)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let text = rendered_text(&terminal);
        assert!(text.contains("fixture-model"));
        assert!(text.contains("Enter send"));
        assert!(text.contains("Conversation"));
    }

    #[test]
    fn approval_controls_stay_visible_when_details_scroll() {
        let mut app = Screen::new("fixture".into(), ".".into());
        let (tx, rx) = mpsc::channel();
        app.approval = Some(ApprovalRequest {
            action: "Write file".into(),
            detail: "Long change details\n".repeat(100),
            respond_to: tx,
        });
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(40, 14)).unwrap();
        for scroll in [0, 30, u16::MAX] {
            app.approval_scroll = scroll;
            terminal.draw(|f| draw(f, &app)).unwrap();
            let text = rendered_text(&terminal);
            assert!(text.contains("[Y] approve one"));
            assert!(text.contains("[N/Esc] deny"));
            assert!(text.contains("Long change details"));
        }
        app.approve(false);
        assert!(!rx.recv().unwrap());
        terminal.draw(|f| draw(f, &app)).unwrap();
        assert!(!rendered_text(&terminal).contains("APPROVAL REQUIRED"));
    }

    #[test]
    fn model_picker_shows_empty_result_and_closes_cleanly() {
        let mut app = Screen::new("fixture".into(), ".".into());
        app.picker = Some(ModelPicker {
            choices: vec![],
            query: "missing".into(),
            selected: 0,
            source: "Fixture".into(),
        });
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(60, 20)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        assert!(rendered_text(&terminal).contains("No matching models"));
        app.picker = None;
        terminal.draw(|f| draw(f, &app)).unwrap();
        assert!(!rendered_text(&terminal).contains("No matching models"));
    }

    #[test]
    fn tiny_terminal_layout_does_not_panic() {
        let app = Screen::new("fixture".into(), ".".into());
        for (w, h) in [(1, 1), (20, 6), (40, 10)] {
            let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| draw(f, &app)).unwrap();
        }
    }

    #[test]
    fn blank_input_does_not_start_agent() {
        let (tx, rx) = mpsc::channel();
        let mut app = Screen::new("test".into(), ".".into());
        app.input = "  ".into();
        app.submit(&tx);
        assert!(!app.busy);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn ordinary_task_is_dispatched_once() {
        let (tx, rx) = mpsc::channel();
        let mut app = Screen::new("test".into(), ".".into());
        app.input = "Explain this project".into();
        app.submit(&tx);
        assert!(app.busy);
        assert!(matches!(rx.try_recv().unwrap(), Work::Prompt(_)));
    }

    #[test]
    fn busy_worker_preserves_pending_input() {
        let (tx, rx) = mpsc::channel();
        let mut app = Screen::new("test".into(), ".".into());
        app.busy = true;
        app.input = "draft".into();
        app.submit(&tx);
        assert_eq!(app.input, "draft");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn approval_is_default_deny() {
        let (tx, rx) = mpsc::channel();
        let mut app = Screen::new("test".into(), ".".into());
        app.approval = Some(ApprovalRequest {
            action: "Write file".into(),
            detail: "demo".into(),
            respond_to: tx,
        });
        app.approve(false);
        assert!(!rx.recv().unwrap());
    }
}
