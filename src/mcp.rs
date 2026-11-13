//! Opt-in stdio MCP. Server processes and every remote call require approval.
use crate::{mcp_protocol::JsonRpcRequest, tools::ToolBox};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variable NAMES only; values are read from the launching shell.
    #[serde(default)]
    pub env: Vec<String>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    servers: Vec<ServerConfig>,
}

fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 30 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn load(path: &Path) -> Result<Vec<ServerConfig>> {
    let f = fs::File::open(path)?;
    let mut bytes = Vec::new();
    f.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        bail!("MCP config exceeds 64 KiB");
    }
    let config: Configuration = serde_json::from_slice(&bytes)?;
    if config.servers.len() > 8 {
        bail!("At most 8 MCP servers");
    }
    let mut names = std::collections::BTreeSet::new();
    for s in &config.servers {
        if !valid_name(&s.name)
            || !names.insert(s.name.clone())
            || s.command.is_empty()
            || s.command.len() > 1000
            || s.args.len() > 30
            || s.args.iter().any(|a| a.len() > 2000)
            || s.env.len() > 30
            || s.env.iter().any(|n| !valid_name(n))
        {
            bail!("Invalid or duplicate MCP server configuration");
        }
    }
    Ok(config.servers)
}

struct Client {
    child: Child,
    send: mpsc::SyncSender<Vec<u8>>,
    receive: mpsc::Receiver<std::result::Result<Value, String>>,
    next_id: u64,
    cancelled: Arc<AtomicBool>,
}
impl Client {
    fn launch(config: &ServerConfig, root: &Path, cancelled: Arc<AtomicBool>) -> Result<Self> {
        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env_clear();
        for key in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "SYSTEMROOT",
            "WINDIR",
            "TEMP",
            "TMP",
            "TMPDIR",
            "LANG",
        ]
        .into_iter()
        .chain(config.env.iter().map(String::as_str))
        {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut child = command.spawn().context("Could not launch MCP server")?;
        let input = child.stdin.take().context("No MCP stdin")?;
        let output = child.stdout.take().context("No MCP stdout")?;
        let (send, queue) = mpsc::sync_channel::<Vec<u8>>(1);
        thread::spawn(move || {
            let mut input = input;
            for bytes in queue {
                if input.write_all(&bytes).and_then(|_| input.flush()).is_err() {
                    break;
                }
            }
        });
        let (writer, receive) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = Vec::new();
                let result = reader.by_ref().take(1_048_577).read_until(b'\n', &mut line);
                let item = match result {
                    Ok(0) => break,
                    Ok(_) if line.len() > 1_048_576 => Err("MCP message exceeds 1 MiB".into()),
                    Ok(_) => serde_json::from_slice(&line)
                        .map_err(|_| "Invalid MCP JSON on stdout".into()),
                    Err(_) => Err("MCP output closed".into()),
                };
                let failed = item.is_err();
                if writer.send(item).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            send,
            receive,
            next_id: 1,
            cancelled,
        })
    }
    fn send(&self, value: &Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(value)?;
        if bytes.len() > 65536 {
            bail!("MCP request exceeds 64 KiB");
        }
        bytes.push(b'\n');
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.send.try_send(bytes) {
                Ok(()) => return Ok(()),
                Err(mpsc::TrySendError::Full(returned)) => {
                    if self.cancelled.load(Ordering::Relaxed) || Instant::now() >= deadline {
                        bail!("MCP input unavailable or timed out");
                    }
                    bytes = returned;
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => bail!("MCP input disconnected"),
            }
        }
    }
    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&serde_json::to_value(JsonRpcRequest::new(
            id,
            method,
            Some(params),
        ))?)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut total = 0;
        loop {
            if self.cancelled.load(Ordering::Relaxed) {
                self.child.kill().ok();
                bail!("Task cancelled");
            }
            if Instant::now() >= deadline {
                self.child.kill().ok();
                bail!("MCP request timed out after 30 seconds");
            }
            let value = match self.receive.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => bail!("{e}"),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => bail!("MCP server disconnected"),
            };
            total += serde_json::to_vec(&value)?.len();
            if total > 4_000_000 {
                bail!("MCP request output exceeded 4 MB");
            }
            if value["jsonrpc"] != "2.0" {
                bail!("Invalid MCP JSON-RPC version");
            }
            // Server-initiated sampling/elicitation is deliberately unsupported.
            if value.get("method").is_some() {
                if value.get("id").is_some() {
                    self.send(&json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32601,"message":"Client method not supported"}}))?;
                }
                continue;
            }
            if value["id"].as_u64() != Some(id) {
                bail!("MCP response ID mismatch");
            }
            if value.get("error").is_some() {
                bail!("MCP server returned a JSON-RPC error (details suppressed)");
            }
            return value.get("result").cloned().context("MCP result missing");
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

pub struct Registry {
    config: Vec<ServerConfig>,
    root: PathBuf,
    clients: BTreeMap<String, Client>,
    tools: BTreeMap<String, (String, String, Value)>,
    cancelled: Arc<AtomicBool>,
}
impl Registry {
    pub fn new(path: Option<&Path>, root: &Path, cancelled: Arc<AtomicBool>) -> Result<Self> {
        Ok(Self {
            config: match path {
                Some(p) => load(p)?,
                None => Vec::new(),
            },
            root: root.to_owned(),
            clients: BTreeMap::new(),
            tools: BTreeMap::new(),
            cancelled,
        })
    }
    pub fn set_cancellation(&mut self, token: Arc<AtomicBool>) {
        self.cancelled = token.clone();
        for client in self.clients.values_mut() {
            client.cancelled = token.clone();
        }
    }
    pub fn list(&self) -> Value {
        json!(self
            .config
            .iter()
            .map(|c| json!({"name":c.name,"connected":self.clients.contains_key(&c.name)}))
            .collect::<Vec<_>>())
    }
    pub fn connect(&mut self, name: &str, approval: &ToolBox) -> Result<Value> {
        if self.clients.contains_key(name) {
            return Ok(json!({"connected":name}));
        }
        let config = self
            .config
            .iter()
            .find(|c| c.name == name)
            .context("No configured MCP server with this name")?;
        if !approval.approve("Launch configured MCP server",&format!("{}\nProgram: {}\nArgs: {:?}\nThis process runs with your OS permissions, not in a sandbox.",name,config.command,config.args))?{return Ok(json!({"ok":false,"status":"denied"}));}
        let mut client = Client::launch(config, &self.root, self.cancelled.clone())?;
        let initialized=client.request("initialize",json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"blue-dragon","version":env!("CARGO_PKG_VERSION")}}))?;
        if !matches!(
            initialized["protocolVersion"].as_str(),
            Some("2024-11-05" | "2025-03-26" | "2025-06-18")
        ) {
            bail!("Unsupported MCP protocol version");
        }
        client.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        if initialized["capabilities"].get("tools").is_none() {
            bail!("MCP server has no tools capability");
        }
        let mut pending = BTreeMap::new();
        let mut cursor: Option<String> = None;
        for _ in 0..8 {
            let page = client.request(
                "tools/list",
                match &cursor {
                    Some(c) => json!({"cursor":c}),
                    None => json!({}),
                },
            )?;
            let tools = page["tools"].as_array().context("MCP tools list missing")?;
            for tool in tools {
                let original = tool["name"].as_str().context("MCP tool name missing")?;
                if original.len() > 40
                    || !original
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                {
                    bail!("Unsupported MCP tool name");
                }
                let exposed = format!("mcp__{name}__{original}");
                if exposed.len() > 64
                    || pending.contains_key(&exposed)
                    || pending.len() + self.tools.len() >= 64
                {
                    bail!("Duplicate/too many MCP tools or long names");
                }
                if tool["inputSchema"]["type"] != "object" {
                    bail!("MCP tool schema must be an object");
                }
                let schema = json!({"type":"function","function":{"name":exposed,"description":tool["description"].as_str().unwrap_or("External MCP tool"),"parameters":tool["inputSchema"]}});
                pending.insert(exposed, (name.to_string(), original.to_string(), schema));
            }
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        if cursor.is_some() {
            bail!("MCP tool pagination limit reached");
        }
        let count = pending.len();
        self.tools.extend(pending);
        self.clients.insert(name.to_owned(), client);
        Ok(json!({"connected":name,"tools":count}))
    }
    pub fn definitions(&self) -> Vec<Value> {
        self.tools.values().map(|(_, _, v)| v.clone()).collect()
    }
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }
    pub fn call(&mut self, name: &str, args: &Value, approval: &ToolBox) -> Result<Value> {
        let (server, remote, _) = self.tools.get(name).context("Unknown MCP tool")?.clone();
        if !approval.approve(
            &format!("External MCP tool {name}"),
            &serde_json::to_string_pretty(args)?,
        )? {
            return Ok(json!({"ok":false,"status":"denied"}));
        }
        let response = self
            .clients
            .get_mut(&server)
            .context("MCP server disconnected")?
            .request("tools/call", json!({"name":remote,"arguments":args}));
        let result = match response {
            Ok(v) => v,
            Err(error) => {
                self.clients.remove(&server);
                self.tools.retain(|_, (s, _, _)| s != &server);
                return Err(error);
            }
        };
        if result["isError"] == true {
            return Ok(json!({"ok":false,"result":result}));
        }
        Ok(json!({"ok":true,"result":result}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_explicit_valid_unique_configuration() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("mcp.json");
        fs::write(
            &p,
            r#"{"servers":[{"name":"x","command":"demo"},{"name":"x","command":"demo"}]}"#,
        )
        .unwrap();
        assert!(load(&p).is_err());
        fs::write(
            &p,
            r#"{"servers":[{"name":"docs","command":"demo","env":["TOKEN"]}]}"#,
        )
        .unwrap();
        assert_eq!(load(&p).unwrap()[0].name, "docs");
    }
    fn fixture(extra: Option<&str>) -> ServerConfig {
        let mut args = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/mcp_server.py")
            .to_string_lossy()
            .to_string()];
        if let Some(extra) = extra {
            args.push(extra.into());
        }
        ServerConfig {
            name: "fixture".into(),
            command: "python".into(),
            args,
            env: Vec::new(),
        }
    }
    #[test]
    fn approved_stdio_server_exposes_and_calls_real_tools() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("servers.json");
        let server = fixture(None);
        fs::write(
            &config,
            json!({"servers":[{"name":server.name,"command":server.command,"args":server.args}]})
                .to_string(),
        )
        .unwrap();
        let (tx, rx) = mpsc::channel::<crate::tools::ApprovalRequest>();
        let approver = thread::spawn(move || {
            for _ in 0..2 {
                let req = rx.recv_timeout(Duration::from_secs(10)).unwrap();
                req.respond_to.send(true).unwrap();
            }
        });
        let tools = ToolBox::new(root.path().to_owned(), crate::tools::ApprovalMode::Tui(tx));
        let mut registry =
            Registry::new(Some(&config), root.path(), Arc::new(AtomicBool::new(false))).unwrap();
        registry.connect("fixture", &tools).unwrap();
        assert!(registry.has("mcp__fixture__echo"));
        let output = registry
            .call(
                "mcp__fixture__echo",
                &json!({"text":"real subprocess result"}),
                &tools,
            )
            .unwrap();
        assert_eq!(
            output["result"]["content"][0]["text"],
            "real subprocess result"
        );
        approver.join().unwrap();
    }
    #[test]
    fn rejects_wrong_rpc_id_and_respects_cancel() {
        let root = tempfile::tempdir().unwrap();
        let token = Arc::new(AtomicBool::new(false));
        let mut client =
            Client::launch(&fixture(Some("--wrong-id")), root.path(), token.clone()).unwrap();
        assert!(client
            .request("initialize", json!({}))
            .unwrap_err()
            .to_string()
            .contains("ID mismatch"));
        token.store(true, Ordering::Relaxed);
        assert!(client.request("initialize", json!({})).is_err());
    }
}
