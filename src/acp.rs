//! ACP v1 over newline-delimited JSON-RPC. Only the dispatcher writes stdout.
use crate::{
    agent::{Agent, AgentEvent, ApprovalMode},
    config::Settings,
    tools::ApprovalRequest,
};
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

const FRAME: usize = 1024 * 1024;
fn send(mut packet: Value) -> Result<()> {
    packet["jsonrpc"] = json!("2.0");
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, &packet)?;
    out.write_all(b"\n")?;
    Ok(out.flush()?)
}
fn error(id: &Value, code: i32, msg: &str) -> Result<()> {
    send(json!({"id":id,"error":{"code":code,"message":msg}}))
}
fn update(id: &str, value: Value) -> Result<()> {
    send(json!({"method":"session/update","params":{"sessionId":id,"update":value}}))
}
fn block(text: &str) -> Value {
    json!({"type":"text","text":text})
}
fn prompt(params: &Value) -> Result<String> {
    let mut parts = Vec::new();
    for part in params["prompt"]
        .as_array()
        .context("Expected prompt blocks")?
    {
        parts.push(match part["type"].as_str(){
            Some("text")=>part["text"].as_str().context("Expected text")?.to_owned(),
            Some("resource_link")=>format!("Editor resource reference: {}",json!({"name":part["name"].as_str().context("Expected resource name")?,"uri":part["uri"].as_str().context("Expected resource URI")?})),
            _=>bail!("Unsupported prompt content")
        });
    }
    let text = parts.join("\n");
    ensure!(!text.trim().is_empty(), "Empty prompt");
    Ok(text)
}

struct Session {
    prompts: mpsc::Sender<(String, Arc<AtomicBool>)>,
    worker: Option<thread::JoinHandle<()>>,
    events: mpsc::Receiver<AgentEvent>,
    approvals: mpsc::Receiver<ApprovalRequest>,
    cancel: Arc<AtomicBool>,
    pending: Option<(String, ApprovalRequest)>,
    turn: Option<Value>,
    tool: Option<String>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
        // Closing the event and prompt channels unblocks both worker sends and idle receives.
        let (_, empty) = mpsc::sync_channel(0);
        drop(std::mem::replace(&mut self.events, empty));
        let (closed, _) = mpsc::channel();
        drop(std::mem::replace(&mut self.prompts, closed));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Session {
    fn new(settings: Settings, readonly: bool) -> Result<(String, Self)> {
        let (sender, prompts) = mpsc::channel::<(String, Arc<AtomicBool>)>();
        let (output, events) = mpsc::sync_channel(64);
        let (ask, approvals) = mpsc::channel();
        let (ready, started) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker = thread::Builder::new()
            .name("acp-session".into())
            .spawn(move || {
                let approval = if readonly {
                    ApprovalMode::ReadOnly
                } else {
                    ApprovalMode::Tui(ask)
                };
                let mut agent = match Agent::new(settings, approval, None) {
                    Ok(agent) => agent,
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };

                agent.set_editor_output(output.clone());
                if ready.send(Ok(agent.session_id().to_owned())).is_err() {
                    return;
                }
                for (text, token) in prompts {
                    agent.set_cancellation(token);
                    let result = agent.turn(&text).map_err(|e| e.to_string());
                    if output.send(AgentEvent::Finished(result)).is_err() {
                        break;
                    }
                }
            })?;
        Ok((
            started.recv()??,
            Self {
                prompts: sender,
                worker: Some(worker),
                events,
                approvals,
                cancel,
                pending: None,
                turn: None,
                tool: None,
            },
        ))
    }
    fn stop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some((_, ask)) = self.pending.take() {
            let _ = ask.respond_to.send(false);
        }
    }
    fn pump(&mut self, id: &str, seq: &mut u64) -> Result<()> {
        let approval = self.approvals.try_recv().ok();
        for _ in 0..64 {
            let event = match self.events.try_recv() {
                Ok(e) => e,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let Some(r) = self.turn.take() {
                        error(&r, -32603, "Session worker stopped")?
                    }
                    break;
                }
            };
            match event {
                AgentEvent::Answer(text) => update(
                    id,
                    json!({"sessionUpdate":"agent_message_chunk","content":block(&text)}),
                )?,
                AgentEvent::Notice(text) => update(
                    id,
                    json!({"sessionUpdate":"agent_message_chunk","content":block(&format!("{text}\n"))}),
                )?,
                AgentEvent::ToolStart(title) => {
                    *seq += 1;
                    let call = format!("tool-{seq}");
                    update(
                        id,
                        json!({"sessionUpdate":"tool_call","toolCallId":call,"title":title,"status":"in_progress"}),
                    )?;
                    self.tool = Some(call);
                }
                AgentEvent::EditorToolResult(summary, success) => {
                    if let Some(call) = self.tool.take() {
                        update(
                            id,
                            json!({"sessionUpdate":"tool_call_update","toolCallId":call,"status":if success{"completed"}else{"failed"},"content":[{"type":"content","content":block(&summary)}]}),
                        )?;
                    }
                }
                AgentEvent::Finished(done) => {
                    if let Some(call) = self.tool.take() {
                        update(
                            id,
                            json!({"sessionUpdate":"tool_call_update","toolCallId":call,"status":"failed","content":[{"type":"content","content":block("Action ended without a confirmed result.")}]}),
                        )?;
                    }
                    self.pending.take();
                    if let Some(r) = self.turn.take() {
                        if self.cancel.load(Ordering::SeqCst) {
                            send(json!({"id":r,"result":{"stopReason":"cancelled"}}))?
                        } else if let Err(msg) = done {
                            error(&r, -32603, &msg)?
                        } else {
                            send(json!({"id":r,"result":{"stopReason":"end_turn"}}))?
                        }
                    }
                }
                // Answer carries the final reviewed text. Never concatenate it with provisional deltas.
                _ => (),
            }
        }
        if let Some(ask) = approval {
            if self.cancel.load(Ordering::SeqCst) || self.turn.is_none() {
                let _ = ask.respond_to.send(false);
                return Ok(());
            }
            *seq += 1;
            let key = format!("permission-{seq}");
            let call = self.tool.clone().unwrap_or_else(|| format!("action-{seq}"));
            send(
                json!({"id":key,"method":"session/request_permission","params":{"sessionId":id,"toolCall":{"toolCallId":call,"title":ask.action,"content":[{"type":"content","content":block(&ask.detail)}]},"options":[{"optionId":"allow","name":"Allow once","kind":"allow_once"},{"optionId":"deny","name":"Reject","kind":"reject_once"}]}}),
            )?;
            self.pending = Some((key, ask));
        }
        Ok(())
    }
}
pub fn serve(base: Settings, readonly: bool, pinned_root: bool) -> Result<()> {
    let (input, lines) = mpsc::sync_channel(8);
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        loop {
            let mut bytes = Vec::new();
            use std::io::Read;
            match (&mut stdin)
                .take((FRAME + 1) as u64)
                .read_until(b'\n', &mut bytes)
            {
                Ok(0) | Err(_) => break,
                Ok(n) if n > FRAME => break,
                _ => {
                    if input.send(bytes).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let mut initialized = false;
    let mut sessions = BTreeMap::<String, Session>::new();
    let mut seq = 0;
    loop {
        for (id, s) in sessions.iter_mut().filter(|(_, s)| s.turn.is_some()) {
            s.pump(id, &mut seq)?;
        }
        let incoming = if sessions.values().any(|s| s.turn.is_some()) {
            lines.recv_timeout(Duration::from_millis(10))
        } else {
            lines
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        let raw = match incoming {
            Ok(b) => b,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => {
                for s in sessions.values_mut() {
                    s.stop();
                }
                return Ok(());
            }
        };
        let p: Value = match serde_json::from_slice(&raw) {
            Ok(v) => v,
            Err(_) => {
                error(&Value::Null, -32700, "Parse error")?;
                continue;
            }
        };
        if p["jsonrpc"] != "2.0" {
            error(&p["id"], -32600, "Invalid JSON-RPC version")?;
            continue;
        }
        let id = &p["id"];
        if p.get("method").is_none() {
            for s in sessions.values_mut() {
                if s.pending
                    .as_ref()
                    .is_some_and(|(key, _)| Some(key.as_str()) == id.as_str())
                {
                    let (_, ask) = s.pending.take().unwrap();
                    let outcome = &p["result"]["outcome"];
                    let _ = ask.respond_to.send(
                        p.get("error").is_none()
                            && outcome["outcome"] == "selected"
                            && outcome["optionId"] == "allow",
                    );
                    break;
                }
            }
            continue;
        }
        let method = p["method"].as_str().unwrap_or("");
        let params = &p["params"];
        let sid = params["sessionId"].as_str().unwrap_or("");
        if id.is_null() {
            if method == "session/cancel" {
                if let Some(s) = sessions.get_mut(sid).filter(|s| s.turn.is_some()) {
                    s.stop()
                }
            }
            continue;
        }
        if !id.is_number() && !id.is_string() {
            error(&Value::Null, -32600, "Invalid request ID")?;
            continue;
        }
        if method != "initialize" && !initialized {
            error(id, -32002, "Initialize first")?;
            continue;
        }
        let result = (|| -> Result<Option<Value>> {
            Ok(Some(match method {
                "initialize" => {
                    ensure!(
                        !initialized && params["protocolVersion"].is_u64(),
                        "Invalid initialization"
                    );
                    initialized = true;
                    json!({"protocolVersion":1,"agentCapabilities":{"promptCapabilities":{}},"authMethods":[],"agentInfo":{"name":"blue-dragon","title":"Blue Dragon","version":env!("BLUE_DRAGON_PUBLIC_VERSION")}})
                }
                "session/new" => {
                    ensure!(
                        sessions.len() < 16,
                        "Session capacity reached; reconnect for additional sessions"
                    );
                    ensure!(
                        params["mcpServers"].as_array().is_some_and(Vec::is_empty),
                        "Editor-supplied MCP servers are unsupported"
                    );
                    let path = Path::new(params["cwd"].as_str().context("Expected cwd")?);
                    ensure!(
                        path.is_absolute() && path.is_dir(),
                        "Expected absolute workspace directory"
                    );
                    let mut settings = base.clone();
                    settings.root = path.canonicalize()?;
                    ensure!(
                        !pinned_root || settings.root.starts_with(&base.root),
                        "Workspace outside --root"
                    );
                    let (sid, s) = Session::new(settings, readonly)?;
                    sessions.insert(sid.clone(), s);
                    json!({"sessionId":sid})
                }
                "session/prompt" => {
                    let s = sessions.get_mut(sid).context("Unknown session")?;
                    ensure!(s.turn.is_none(), "Session already running a prompt");
                    let text = prompt(params)?;
                    s.cancel = Arc::new(AtomicBool::new(false));
                    s.prompts.send((text, s.cancel.clone()))?;
                    s.turn = Some(id.clone());
                    return Ok(None);
                }
                _ => return Ok(None),
            }))
        })();
        match result {
            Ok(Some(value)) => send(json!({"id":id,"result":value}))?,
            Ok(None) if method == "session/prompt" => (),
            Ok(None) => error(id, -32601, "Method not found")?,
            Err(e) => error(id, -32602, &e.to_string())?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_preserves_order_and_never_reads_resource_uris() {
        let params = json!({"prompt":[{"type":"text","text":"راجع هالملف"},{"type":"resource_link","uri":"file:///does-not-exist/demo.js","name":"demo.js"},{"type":"text","text":"End."}]});
        let text = prompt(&params).unwrap();
        assert!(text.starts_with("راجع هالملف\n"));
        assert!(text.contains("file:///does-not-exist/demo.js"));
        assert!(text.ends_with("\nEnd."));
        assert!(
            prompt(&json!({"prompt":[{"type":"text","text":"valid"},{"type":"audio"}]})).is_err()
        );
        assert!(prompt(&json!({"prompt":[]})).is_err());
    }
}
