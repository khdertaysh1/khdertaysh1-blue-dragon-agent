//! A loopback-only graphical client for the same interactive agent as the terminal.
mod http;

use crate::agent::{Agent, AgentEvent, ApprovalMode};
use crate::{config::Settings, tools::ApprovalRequest};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

type Shared = Arc<Mutex<View>>;
#[derive(Default)]
struct View {
    rows: VecDeque<Value>,
    workspace: String,
    session: String,
    model: String,
    draft: String,
    busy: bool,
    revision: u64,
    sequence: u64,
    approvals: VecDeque<(u64, ApprovalRequest)>,
}

impl View {
    fn row(&mut self, role: &str, text: &str) {
        if self.rows.len() == 160 {
            self.rows.pop_front();
        }
        self.rows
            .push_back(json!([role, text.chars().take(24000).collect::<String>()]));
    }
    fn snapshot(&self) -> Value {
        json!({"revision":self.revision,"workspace":self.workspace,"session":self.session,"model":self.model,
            "rows":self.rows,"draft":self.draft,"busy":self.busy,
            "approval":self.approvals.front().map(|(id,a)|json!({"id":id,"action":a.action,"detail":a.detail,"page_reads":crate::browser::group_approval(&a.action)}))})
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    State {
        since: Option<u64>,
    },
    Sessions,
    Open {
        workspace: String,
        session: Option<String>,
    },
    Send {
        session: String,
        text: String,
    },
    Approve {
        id: u64,
        allow: bool,
    },
}
enum Work {
    Open(String, Option<String>),
    Send(String),
}

struct Client {
    work: mpsc::Sender<Work>,
    view: Shared,
    sessions: std::path::PathBuf,
}
impl Client {
    fn api(&self, request: Request) -> Result<Value> {
        if matches!(request, Request::Sessions) {
            return Ok(json!(crate::session::list_sessions(&self.sessions)?
                .into_iter()
                .take(100)
                .map(|s| json!({"id":s.id,"workspace":s.workspace,"updated":s.updated_at}))
                .collect::<Vec<_>>()));
        }
        let mut v = self.view.lock().unwrap();
        match request {
            Request::State { since } => {
                return Ok(if since == Some(v.revision) {
                    Value::Null
                } else {
                    v.snapshot()
                })
            }
            Request::Approve { id, allow } => {
                ensure!(
                    v.approvals.front().is_some_and(|(n, _)| *n == id),
                    "Approval expired"
                );
                v.approvals
                    .pop_front()
                    .unwrap()
                    .1
                    .respond_to
                    .send(allow)
                    .context("Approval expired")?;
            }
            Request::Open { workspace, session } => {
                ensure!(!v.busy, "A task is running");
                self.work.send(Work::Open(workspace, session))?;
                v.busy = true;
            }
            Request::Send { session, text } => {
                ensure!(
                    v.session == session,
                    "Session changed; review the current workspace"
                );
                ensure!(!v.busy, "A task is running");
                ensure!(!text.trim().is_empty(), "Enter a message");
                self.work.send(Work::Send(text.clone()))?;
                v.row("You", &text);
                v.busy = true;
            }
            Request::Sessions => unreachable!(),
        }
        v.revision += 1;
        Ok(json!({"ok":true}))
    }
}

fn open(
    settings: &Settings,
    approval: &ApprovalMode,
    resume: Option<&str>,
    view: &Shared,
) -> Result<Agent> {
    let mut agent = Agent::new(settings.clone(), approval.clone(), resume)?;
    agent.quota_picker =
        Box::new(|_| anyhow::bail!("Select a provider/model with /model before continuing"));
    let mut v = view.lock().unwrap();
    v.session = agent.session_id().into();
    v.model = agent.model().into();
    v.workspace = settings.root.display().to_string();
    v.rows.clear();
    v.draft.clear();
    if resume.is_some() {
        let saved =
            crate::session::load(&settings.sessions_dir, agent.session_id(), &settings.root)?;
        for (i, msg) in saved.messages.iter().enumerate() {
            let role = match msg["role"].as_str() {
                Some("user") if saved.user_turns.contains(&i) => "You",
                Some("assistant") => "Dragon",
                _ => continue,
            };
            if let Some(s) = msg["content"].as_str().filter(|s| !s.is_empty()) {
                v.row(role, s);
            }
        }
    }
    Ok(agent)
}

pub fn run(
    settings: Settings,
    read_only: bool,
    resume: Option<String>,
    launch: bool,
) -> Result<()> {
    let sessions = settings.sessions_dir.clone();
    let view = Shared::default();
    let (work, rx) = mpsc::channel();
    let (ready, started) = mpsc::channel();
    let (approve, approvals) = mpsc::channel::<ApprovalRequest>();
    let display = view.clone();
    thread::spawn(move || {
        for req in approvals {
            let mut v = display.lock().unwrap();
            v.revision += 1;
            v.sequence += 1;
            let id = v.sequence;
            v.approvals.push_back((id, req));
        }
    });
    let display = view.clone();
    thread::spawn(move || {
        let mode = if read_only {
            ApprovalMode::ReadOnly
        } else {
            ApprovalMode::Tui(approve)
        };
        let mut agent = match open(&settings, &mode, resume.as_deref(), &display) {
            Ok(a) => a,
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        };
        let _ = ready.send(Ok(()));
        for task in rx {
            let result = match task {
                Work::Open(path, id) => (|| {
                    let mut next = settings.clone();
                    next.root = std::path::Path::new(&path).canonicalize()?;
                    ensure!(next.root.is_dir(), "Choose a workspace directory");
                    if id.as_deref() != Some(agent.session_id()) || next.root != agent.root() {
                        agent = open(&next, &mode, id.as_deref(), &display)?;
                    }
                    Ok(())
                })(),
                Work::Send(text) => {
                    let (tx, events) = mpsc::sync_channel(256);
                    agent.set_output(tx.clone());
                    thread::scope(|scope| {
                        let v = &display;
                        scope.spawn(move || {
                            for event in events {
                                let mut v = v.lock().unwrap();
                                v.revision += 1;
                                match event {
                                    AgentEvent::Finished(_) => break,
                                    AgentEvent::Delta(s) => {
                                        if v.draft.len() < 100000 {
                                            v.draft.push_str(&s)
                                        }
                                    }
                                    AgentEvent::Answer(s) => {
                                        v.draft.clear();
                                        v.row("Dragon", &s)
                                    }
                                    AgentEvent::ToolStart(s) => v.row("Tool", &s),
                                    AgentEvent::ToolResult(s) => v.row("Result", &s),
                                    AgentEvent::Notice(s) => v.row("System", &s),
                                    AgentEvent::Plan(s) => v.row("Plan", &s),
                                    AgentEvent::Model(s) => v.model = s,
                                    AgentEvent::ModelChoices(choices, _) => v.row(
                                        "Models",
                                        &choices
                                            .into_iter()
                                            .map(|c| c.id)
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                    ),
                                }
                            }
                        });
                        let result = if text.trim_start().starts_with('/') {
                            crate::tui::run_slash(&mut agent, &tx, text.trim())
                        } else {
                            agent.turn(&text)
                        };
                        let _ = tx.send(AgentEvent::Finished(Ok(())));
                        result
                    })
                }
            };
            let mut v = display.lock().unwrap();
            if let Err(e) = result {
                v.row("Error", &format!("{e:#}"));
            }
            v.approvals.clear();
            v.draft.clear();
            v.revision += 1;
            v.busy = false;
        }
    });
    started.recv()??;
    http::serve(
        Client {
            work,
            view,
            sessions,
        },
        launch,
    )
}
