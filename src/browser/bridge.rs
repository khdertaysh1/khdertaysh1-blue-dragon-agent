use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::{
    handshake::server::{ErrorResponse, Request, Response},
    Message,
};

struct Command {
    id: u64,
    action: String,
    args: Value,
    reply: SyncSender<Result<Value>>,
    deadline: Instant,
}
struct State {
    alive: AtomicBool,
    slot: Mutex<Option<SyncSender<Command>>>,
    clients: AtomicUsize,
    sequence: AtomicU64,
}
pub struct Bridge {
    state: Arc<State>,
    port: u16,
    token: String,
}
impl Bridge {
    pub fn start(home: &Path, port: u16) -> Result<Self> {
        std::fs::create_dir_all(home)?;
        let token_path = home.join("browser-pair.token");
        let existing = (|| -> std::io::Result<String> {
            use std::io::Read;
            let mut value = String::new();
            std::fs::File::open(&token_path)?
                .take(65)
                .read_to_string(&mut value)?;
            Ok(value)
        })();
        let token = match existing {
            Ok(s) if s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()) => s,
            Ok(_) => anyhow::bail!(
                "Invalid browser pairing token; remove the local token file to re-pair"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut bytes = [0u8; 32];
                getrandom::fill(&mut bytes)
                    .map_err(|_| anyhow::anyhow!("OS random generator failed"))?;
                let token = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                use std::io::Write;
                options.open(&token_path)?.write_all(token.as_bytes())?;
                token
            }
            Err(e) => return Err(e.into()),
        };
        let listener = TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], port)))
            .context("Browser bridge port unavailable")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let state = Arc::new(State {
            alive: AtomicBool::new(true),
            slot: Mutex::new(None),
            clients: AtomicUsize::new(0),
            sequence: AtomicU64::new(1),
        });
        let shared = state.clone();
        let secret = token.clone();
        thread::spawn(move || {
            while shared.alive.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, peer)) if peer.ip().is_loopback() => {
                        if shared.clients.fetch_add(1, Ordering::Relaxed) >= 8 {
                            shared.clients.fetch_sub(1, Ordering::Relaxed);
                            continue;
                        }
                        let shared = shared.clone();
                        let secret = secret.clone();
                        thread::spawn(move || {
                            handle(stream, &shared, &secret).ok();
                            shared.clients.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(40))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self { state, port, token })
    }
    pub fn pairing(&self) -> Value {
        json!({"url":format!("ws://127.0.0.1:{}/bridge",self.port),"pairing_token":self.token,"instruction":"Copy this local secret into Blue Dragon Browser Bridge options. It is never sent to the model. Install the extension first; keep it private."})
    }
    pub fn status(&self) -> Value {
        json!({"backend":"extension","port":self.port,"ready":self.state.slot.lock().map(|s|s.is_some()).unwrap_or(false),"scope":"managed tabs only"})
    }
    pub fn request(&self, action: &str, args: &Value) -> Result<Value> {
        let sender = self
            .state
            .slot
            .lock()
            .map_err(|_| anyhow::anyhow!("Bridge state unavailable"))?
            .clone()
            .context("Browser extension not paired/connected")?;
        let (tx, rx) = mpsc::sync_channel(1);
        let deadline = Instant::now() + Duration::from_secs(30);
        let command = Command {
            id: self.state.sequence.fetch_add(1, Ordering::Relaxed),
            action: action.into(),
            args: args.clone(),
            reply: tx,
            deadline,
        };
        sender.try_send(command).map_err(|_| {
            anyhow::anyhow!("Browser command queue unavailable; action not dispatched")
        })?;
        rx.recv_timeout(Duration::from_secs(31))
            .context("Browser deadline exceeded; outcome unknown; inspect before retry")?
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.state.alive.store(false, Ordering::Relaxed);
    }
}
fn same_secret(expected: &str, actual: &str) -> bool {
    expected.len() == actual.len()
        && expected
            .bytes()
            .zip(actual.bytes())
            .fold(0u8, |a, (x, y)| a | (x ^ y))
            == 0
}
fn origin_ok(origin: &str) -> bool {
    origin.starts_with("moz-extension://") || origin.starts_with("chrome-extension://")
}
fn handle(stream: TcpStream, state: &Arc<State>, token: &str) -> Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut config = tungstenite::protocol::WebSocketConfig::default();
    config.max_message_size = Some(3_000_000);
    config.max_frame_size = Some(3_000_000);
    // The upstream handshake callback requires this concrete unboxed error type.
    #[allow(clippy::result_large_err)]
    let callback =
        |req: &Request, response: Response| -> std::result::Result<Response, ErrorResponse> {
            let allowed = req.uri().path() == "/bridge"
                && req.uri().query().is_none()
                && req
                    .headers()
                    .get("origin")
                    .and_then(|s| s.to_str().ok())
                    .is_some_and(origin_ok);
            if allowed {
                Ok(response)
            } else {
                Err(tungstenite::http::Response::builder()
                    .status(403)
                    .body(Some("Extension origin required".into()))
                    .unwrap())
            }
        };
    let mut ws = tungstenite::accept_hdr_with_config(stream, callback, Some(config))?;
    let text = ws.read()?.into_text()?;
    let hello: Value = serde_json::from_str(&text)?;
    if hello["type"] != "hello"
        || hello["version"] != 1
        || !same_secret(token, hello["token"].as_str().unwrap_or(""))
        || !super::CAPABILITIES.iter().all(|name| {
            hello["capabilities"]
                .as_array()
                .is_some_and(|caps| caps.iter().any(|v| v == name))
        })
    {
        ws.close(None).ok();
        anyhow::bail!("Browser pairing rejected");
    }
    let (tx, rx) = mpsc::sync_channel::<Command>(16);
    {
        let mut slot = state
            .slot
            .lock()
            .map_err(|_| anyhow::anyhow!("Bridge state unavailable"))?;
        if slot.is_some() {
            ws.close(None).ok();
            anyhow::bail!("A browser is already connected");
        }
        *slot = Some(tx);
    }
    let mut pending: HashMap<u64, Command> = HashMap::new();
    let outcome = (|| -> Result<()> {
        ws.send(Message::Text(
            json!({"type":"ready","version":1}).to_string().into(),
        ))?;
        ws.get_ref()
            .set_read_timeout(Some(Duration::from_millis(80)))?;

        while state.alive.load(Ordering::Relaxed) {
            while let Ok(command) = rx.try_recv() {
                if command.deadline < Instant::now() {
                    command
                        .reply
                        .try_send(Err(anyhow::anyhow!("Command expired before dispatch")))
                        .ok();
                    continue;
                }
                ws.send(Message::Text(json!({"type":"request","id":command.id,"action":command.action,"args":command.args}).to_string().into()))?;
                pending.insert(command.id, command);
            }
            let expired = pending
                .iter()
                .filter(|(_, c)| c.deadline < Instant::now())
                .map(|(id, _)| *id)
                .collect::<Vec<_>>();
            for id in expired {
                if let Some(c) = pending.remove(&id) {
                    c.reply
                        .try_send(Err(anyhow::anyhow!(
                            "Browser timeout; outcome unknown; no retry"
                        )))
                        .ok();
                }
            }
            match ws.read() {
                Ok(Message::Text(text)) => {
                    let value: Value = serde_json::from_str(&text)?;
                    if value["type"] == "ping" {
                        ws.send(Message::Text(json!({"type":"pong"}).to_string().into()))?;
                        continue;
                    }
                    if value["type"] != "response" {
                        anyhow::bail!("Unexpected browser packet");
                    }
                    if let Some(c) = value["id"].as_u64().and_then(|id| pending.remove(&id)) {
                        let result = if value["ok"] == true {
                            Ok(value["result"].clone())
                        } else {
                            Err(anyhow::anyhow!("Browser action failed; outcome may be unknown; inspect before retry. Untrusted error: {}",value["error"].as_str().unwrap_or("extension error").chars().filter(|c|!c.is_control()).take(300).collect::<String>()))
                        };
                        c.reply.try_send(result).ok();
                    }
                }
                Ok(Message::Ping(data)) => {
                    ws.send(Message::Pong(data))?;
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    })();
    if let Ok(mut slot) = state.slot.lock() {
        *slot = None;
    }
    for (_, c) in pending {
        c.reply
            .try_send(Err(anyhow::anyhow!(
                "Browser disconnected; outcome unknown; no action replayed"
            )))
            .ok();
    }
    ws.close(None).ok();
    outcome
}
#[cfg(test)]
mod tests {
    use super::*;
    use tungstenite::client::IntoClientRequest;
    #[test]
    fn rejects_web_origins_wrong_tokens_and_missing_capabilities() {
        let home = tempfile::tempdir().unwrap();
        let bridge = Bridge::start(home.path(), 0).unwrap();
        let url = bridge.pairing()["url"].as_str().unwrap().to_owned();
        let mut request = url.clone().into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Origin", "https://example.com".parse().unwrap());
        assert!(tungstenite::connect(request).is_err());
        for (token, caps) in [
            ("wrong".to_owned(), json!(super::super::CAPABILITIES)),
            (bridge.token.clone(), json!([])),
        ] {
            let mut request = url.clone().into_client_request().unwrap();
            request
                .headers_mut()
                .insert("Origin", "moz-extension://fixture".parse().unwrap());
            let (mut ws, _) = tungstenite::connect(request).unwrap();
            ws.send(Message::Text(
                json!({"type":"hello","version":1,"token":token,"capabilities":caps})
                    .to_string()
                    .into(),
            ))
            .unwrap();
            assert!(matches!(ws.read(), Ok(Message::Close(_)) | Err(_)));
            assert_eq!(bridge.status()["ready"], false);
        }
    }
    #[test]
    fn authenticates_extension_and_routes_one_bounded_action() {
        let home = tempfile::tempdir().unwrap();
        let bridge = Bridge::start(home.path(), 0).unwrap();
        let url = bridge.pairing()["url"].as_str().unwrap().to_owned();
        let token = bridge.token.clone();
        let worker = thread::spawn(move || {
            let mut request = url.into_client_request().unwrap();
            request
                .headers_mut()
                .insert("Origin", "moz-extension://fixture".parse().unwrap());
            let (mut ws, _) = tungstenite::connect(request).unwrap();
            ws.send(Message::Text(
                json!({"type":"hello","version":1,"token":token,"capabilities":super::super::CAPABILITIES})
                    .to_string()
                    .into(),
            ))
            .unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&ws.read().unwrap().into_text().unwrap()).unwrap()
                    ["type"],
                "ready"
            );
            let command: Value =
                serde_json::from_str(&ws.read().unwrap().into_text().unwrap()).unwrap();
            assert_eq!(command["action"], "snapshot");
            ws.send(Message::Text(json!({"type":"response","id":command["id"],"ok":true,"result":{"title":"Fixture"}}).to_string().into())).unwrap();
            thread::sleep(Duration::from_millis(200));
        });
        for _ in 0..100 {
            if bridge.status()["ready"] == true {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            bridge.request("snapshot", &json!({})).unwrap()["title"],
            "Fixture"
        );
        worker.join().unwrap();
        assert!(!origin_ok("https://malicious.example"));
        assert!(!same_secret("secret", "wrong"));
        assert!(bridge.status().get("pairing_token").is_none());
    }
}
