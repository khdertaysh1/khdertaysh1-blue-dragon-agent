//! Cross-platform full-screen terminal client.
//! The model worker never reads from the terminal: every mutation is approved
//! through a typed UI event with default-deny on disconnect.
use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    io::{self, IsTerminal},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use crate::{
    agent::{Agent, AgentEvent, ApprovalMode},
    config::Settings,
    tools::ApprovalRequest,
};

const BLUE: Color = Color::Rgb(25, 170, 232);
const MUTED: Color = Color::Rgb(139, 170, 191);
const MAX_INPUT_CHARS: usize = 4096;
const MAX_LOG_LINES: usize = 600;

enum Work {
    Prompt(String),
    Slash(String),
}

struct Screen {
    lines: Vec<String>,
    input: String,
    busy: bool,
    approval: Option<ApprovalRequest>,
    scroll_up: usize,
    status: String,
    model: String,
    workspace: String,
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
            scroll_up: 0,
            status: "Ready".into(),
            model,
            workspace,
        }
    }

    fn log(&mut self, label: &str, message: &str) {
        let clean = message.replace('\r', "");
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
        let text = self.input.trim().to_string();
        self.input.clear();
        if text.is_empty() || self.busy {
            return;
        }
        if text == "/help" {
            self.log(
                "System:",
                "Commands: /help /clear /exit /map /status /diff /session /skills /skill <name> /doctor",
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
        if tx.send(command).is_ok() {
            self.busy = true;
            self.status = "Working (approval may be requested)".into();
        } else {
            self.status = "Agent worker stopped".into();
        }
    }

    fn approve(&mut self, accepted: bool) {
        if let Some(request) = self.approval.take() {
            let _ = request.respond_to.send(accepted);
            self.log(
                "Approval:",
                if accepted { "Approved one action" } else { "Denied action" },
            );
            self.status = if accepted {
                "Action approved; working".into()
            } else {
                "Action denied; working".into()
            };
        }
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run(settings: Settings, read_only: bool, resume: Option<&str>) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        anyhow::bail!("TUI needs an interactive terminal. Use 'blue-dragon chat' or 'run' instead");
    }
    let mut app = Screen::new(
        settings.model.clone(),
        settings.root.display().to_string(),
    );

    let (work_tx, work_rx) = mpsc::channel::<Work>();
    let (message_tx, message_rx) = mpsc::channel::<AgentEvent>();
    let (approval_tx, approval_rx) = mpsc::channel::<ApprovalRequest>();
    let (done_tx, done_rx) = mpsc::channel::<std::result::Result<(), String>>();
    let saved_resume = resume.map(str::to_owned);

    thread::spawn(move || {
        let approval = if read_only {
            ApprovalMode::ReadOnly
        } else {
            ApprovalMode::Tui(approval_tx)
        };
        let mut agent = match Agent::new(settings, approval, saved_resume.as_deref()) {
            Ok(agent) => agent,
            Err(error) => {
                let _ = done_tx.send(Err(format!("Unable to start agent: {error:#}")));
                return;
            }
        };
        agent.set_output(message_tx.clone());
        for work in work_rx {
            let result = match work {
                Work::Prompt(prompt) => agent.turn(&prompt),
                Work::Slash(line) => run_slash(&mut agent, &message_tx, &line),
            };
            let status = result.map_err(|error| format!("{error:#}"));
            if done_tx.send(status).is_err() {
                break;
            }
        }
    });

    enable_raw_mode().context("Could not enable terminal raw mode")?;
    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(error.into());
    }
    let _guard = TerminalGuard;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Could not initialize TUI")?;

    loop {
        while let Ok(message) = message_rx.try_recv() {
            match message {
                AgentEvent::Answer(text) => app.log("Dragon:", &text),
                AgentEvent::ToolStart(name) => {
                    app.log("Tool:", &name);
                    app.status = "Tool running".into();
                }
                AgentEvent::ToolResult(ok) => {
                    app.log("Result:", if ok { "Completed" } else { "Failed or denied" });
                }
            }
        }
        while let Ok(request) = approval_rx.try_recv() {
            if app.approval.is_some() {
                let _ = request.respond_to.send(false);
                app.log("System:", "Concurrent approval request rejected");
            } else {
                app.status = "Approval required (Y/N)".into();
                app.approval = Some(request);
            }
        }
        while let Ok(result) = done_rx.try_recv() {
            app.busy = false;
            match result {
                Ok(()) => app.status = "Ready".into(),
                Err(error) => {
                    app.log("Error:", &error);
                    app.status = "Task failed; see transcript".into();
                }
            }
        }

        terminal.draw(|frame| draw(frame, &app))?;

        if !event::poll(Duration::from_millis(60))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if app.approval.is_some() {
            match key.code {
                KeyCode::Char('y' | 'Y') => app.approve(true),
                KeyCode::Char('n' | 'N') | KeyCode::Esc => app.approve(false),
                _ => {}
            }
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            break;
        }
        match key.code {
            KeyCode::Esc => break,
            KeyCode::PageUp => app.scroll_up = app.scroll_up.saturating_add(5),
            KeyCode::PageDown => app.scroll_up = app.scroll_up.saturating_sub(5),
            KeyCode::Enter => {
                if app.input.trim() == "/exit" || app.input.trim() == "/quit" {
                    break;
                }
                app.submit(&work_tx);
            }
            KeyCode::Backspace => {
                app.input.pop();
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if app.input.chars().count() < MAX_INPUT_CHARS {
                    app.input.push(ch);
                }
            }
            _ => {}
        }
    }
    app.approve(false);
    drop(work_tx);
    Ok(())
}

fn run_slash(agent: &mut Agent, tx: &Sender<AgentEvent>, line: &str) -> Result<()> {
    let response = match line {
        "/map" => serde_json::to_string_pretty(&agent.project_map()?)?,
        "/status" => serde_json::to_string_pretty(&agent.git_status()?)?,
        "/diff" => agent.git_diff()?,
        "/skills" => serde_json::to_string_pretty(&agent.list_skills()?)?,
        "/session" => agent.session_id().to_owned(),
        "/doctor" => format!("Model: {} | workspace: {}", agent.model(), agent.root().display()),
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

fn draw(frame: &mut Frame<'_>, app: &Screen) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new(" BLUE DRAGON  ◆  AI CODING AGENT ")
            .style(Style::default().fg(BLUE).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BLUE))),
        outer[0],
    );

    let text = app.lines.join("\n");
    let visible_lines = outer[1].height.saturating_sub(2) as usize;
    let start = app
        .lines
        .len()
        .saturating_sub(visible_lines)
        .saturating_sub(app.scroll_up);
    let scroll = u16::try_from(start).unwrap_or(u16::MAX);
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .block(Block::default().title(" Conversation ").borders(Borders::ALL))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        outer[1],
    );

    let status = format!(
        " {}  |  {}  |  {} ",
        app.status, app.model, app.workspace
    );
    frame.render_widget(
        Paragraph::new(status)
            .style(Style::default().fg(MUTED))
            .block(Block::default().title(" Status ").borders(Borders::ALL)),
        outer[2],
    );

    let prompt = if app.busy { " Busy" } else { " Prompt" };
    frame.render_widget(
        Paragraph::new(format!(" > {}▌", app.input))
            .style(Style::default().fg(Color::White))
            .block(Block::default().title(prompt).borders(Borders::ALL).border_style(Style::default().fg(BLUE))),
        outer[3],
    );

    if let Some(request) = &app.approval {
        let popup = center(frame.area(), 86, 54);
        frame.render_widget(Clear, popup);
        let message = format!(
            "{}\n\n{}\n\n[Y] approve exactly this action    [N] deny    [Esc] deny",
            request.action, request.detail
        );
        frame.render_widget(
            Paragraph::new(message)
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .title(" APPROVAL REQUIRED ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow)),
                )
                .wrap(Wrap { trim: false }),
            popup,
        );
    }
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
