use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::Read,
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};

pub struct Cdp {
    ws: WebSocket<TcpStream>,
    id: u64,
    network: VecDeque<Value>,
    console: VecDeque<Value>,
    loaded_documents: VecDeque<String>,
    target: String,
    port: u16,
    alive: bool,
    child: Option<Child>,
    profile: Option<tempfile::TempDir>,
}
impl Cdp {
    pub fn attach(port: u16) -> Result<Self> {
        if port == 0 {
            anyhow::bail!("CDP port must be nonzero");
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let response = client
            .put(format!("http://127.0.0.1:{port}/json/new?about:blank"))
            .send()?;
        if !response.status().is_success() {
            anyhow::bail!(
                "CDP could not create a dedicated tab: HTTP {}",
                response.status()
            );
        }
        let mut bytes = Vec::new();
        response.take(131_073).read_to_end(&mut bytes)?;
        if bytes.len() > 131_072 {
            anyhow::bail!("CDP discovery too large");
        }
        let info: Value = serde_json::from_slice(&bytes)?;
        let url = reqwest::Url::parse(
            info["webSocketDebuggerUrl"]
                .as_str()
                .context("CDP websocket missing")?,
        )?;
        if url.scheme() != "ws"
            || !matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
            || url.port() != Some(port)
            || !url.username().is_empty()
            || url.password().is_some()
        {
            anyhow::bail!("CDP discovery returned a nonlocal websocket");
        }
        let stream = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_secs(5),
        )?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let mut config = tungstenite::protocol::WebSocketConfig::default();
        config.max_message_size = Some(3_000_000);
        config.max_frame_size = Some(3_000_000);
        let (ws, _) = tungstenite::client::client_with_config(url.as_str(), stream, Some(config))?;
        ws.get_ref()
            .set_read_timeout(Some(Duration::from_millis(100)))?;
        let mut cdp = Self {
            ws,
            id: 0,
            network: VecDeque::new(),
            console: VecDeque::new(),
            loaded_documents: VecDeque::new(),
            target: info["id"].as_str().context("CDP target missing")?.into(),
            port,
            alive: true,
            child: None,
            profile: None,
        };
        for method in ["Page.enable", "Runtime.enable", "Network.enable"] {
            cdp.call(method, json!({}))?;
        }
        cdp.call("Page.setLifecycleEventsEnabled", json!({"enabled":true}))?;
        Ok(cdp)
    }
    pub fn launch(headless: bool, executable: Option<PathBuf>) -> Result<Self> {
        let binary = executable.or_else(find_browser).context("Chrome/Chromium/Edge not found. Install a browser, use Firefox /browser start, or attach an explicitly forwarded CDP port.")?;
        let profile = tempfile::Builder::new()
            .prefix("blue-dragon-browser-")
            .tempdir()?;
        let mut command = Command::new(binary);
        command
            .args([
                "--remote-debugging-address=127.0.0.1",
                "--remote-debugging-port=0",
                "--no-first-run",
                "--no-default-browser-check",
            ])
            .arg(format!("--user-data-dir={}", profile.path().display()))
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if headless {
            command.arg("--headless=new");
        }
        // Official Termux Chromium cannot use the desktop setuid/seccomp sandbox.
        // This applies only to our dedicated Termux child, never a desktop browser.
        if super::setup::termux() {
            command.args(["--no-sandbox", "--disable-dev-shm-usage"]);
            if headless {
                command.arg("--disable-gpu");
            }
        }
        let mut child = command.spawn()?;
        let started = Instant::now();
        let port = loop {
            if let Ok(contents) = std::fs::read_to_string(profile.path().join("DevToolsActivePort"))
            {
                if let Some(port) = contents.lines().next().and_then(|v| v.parse::<u16>().ok()) {
                    break port;
                }
            }
            if child.try_wait()?.is_some() {
                anyhow::bail!("Browser exited before debugger startup. Check installed browser compatibility and browser-install.log.");
            }
            if started.elapsed() > Duration::from_secs(20) {
                child.kill().ok();
                child.wait().ok();
                anyhow::bail!("Browser startup timed out");
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        match Self::attach(port) {
            Ok(mut cdp) => {
                cdp.child = Some(child);
                cdp.profile = Some(profile);
                Ok(cdp)
            }
            Err(error) => {
                child.kill().ok();
                child.wait().ok();
                Err(error)
            }
        }
    }
    pub fn status(&self) -> Value {
        json!({"backend":"cdp","ready":self.alive,"port":self.port,"dedicated_tab":self.target,"launched_by_blue_dragon":self.child.is_some(),"termux_chromium_sandbox_unavailable":self.child.is_some() && super::setup::termux()})
    }
    fn record(&mut self, value: &Value) {
        let p = &value["params"];
        if value["method"] == "Page.lifecycleEvent"
            && matches!(p["name"].as_str(), Some("DOMContentLoaded" | "load"))
        {
            if let Some(loader) = p["loaderId"].as_str() {
                self.loaded_documents.push_back(loader.to_owned());
                if self.loaded_documents.len() > 16 {
                    self.loaded_documents.pop_front();
                }
            }
        }
        let event = match value["method"].as_str() {
            Some("Network.requestWillBeSent") => Some(
                json!({"event":"request","request_id":p["requestId"],"url":p["request"]["url"].as_str().unwrap_or("").chars().take(2000).collect::<String>(),"method":p["request"]["method"],"type":p["type"]}),
            ),
            Some("Network.responseReceived") => Some(
                json!({"event":"response","request_id":p["requestId"],"url":p["response"]["url"].as_str().unwrap_or("").chars().take(2000).collect::<String>(),"status":p["response"]["status"],"mime_type":p["response"]["mimeType"],"security_headers":super::inspection::security_headers(&p["response"]["headers"])}),
            ),
            _ => None,
        };
        if let Some(event) = event {
            self.network.push_back(event);
            if self.network.len() > 300 {
                self.network.pop_front();
            }
        }
        if value["method"] == "Runtime.consoleAPICalled" {
            let text = p["args"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| {
                    v["value"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v["description"].as_str().unwrap_or("[object]").into())
                })
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(4000)
                .collect::<String>();
            self.console
                .push_back(json!({"level":p["type"],"text":text}));
            if self.console.len() > 200 {
                self.console.pop_front();
            }
        }
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        let id = self.id;
        self.ws.send(Message::Text(
            json!({"id":id,"method":method,"params":params})
                .to_string()
                .into(),
        ))?;
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until {
            match self.ws.read() {
                Ok(Message::Text(text)) => {
                    let value: Value = serde_json::from_str(&text)?;
                    if value["id"] == id {
                        if value.get("error").is_some() {
                            anyhow::bail!("Browser protocol error; action outcome may be unknown. Inspect a snapshot before retrying.");
                        }
                        return Ok(value["result"].clone());
                    }
                    self.record(&value);
                }
                Ok(Message::Ping(data)) => {
                    self.ws.send(Message::Pong(data))?;
                }
                Ok(Message::Close(_)) => {
                    self.alive = false;
                    anyhow::bail!(
                        "Browser closed; action outcome unknown; never automatically retried"
                    );
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => {
                    self.alive = false;
                    anyhow::bail!(
                        "Browser connection interrupted; action outcome unknown; no retry"
                    );
                }
            }
        }
        anyhow::bail!("Browser action deadline exceeded; outcome unknown; inspect before retry")
    }
    fn evaluate(&mut self, expression: String) -> Result<Value> {
        let result=self.call("Runtime.evaluate",json!({"expression":expression,"returnByValue":true,"awaitPromise":true,"timeout":15000}))?;
        if result.get("exceptionDetails").is_some() {
            anyhow::bail!("Page JavaScript failed (untrusted error details omitted)");
        }
        Ok(result["result"]["value"].clone())
    }
    pub fn action(&mut self, action: &str, args: &Value) -> Result<Value> {
        match action {
            "open"|"navigate" => {
                super::validate_url(args["url"].as_str().context("Missing URL")?)?;
                if args["fresh_document"] == true {
                    self.call("Page.navigate",json!({"url":"about:blank"}))?;
                }
                let result=self.call("Page.navigate",json!({"url":args["url"]}))?;
                if result.get("errorText").is_some() { anyhow::bail!("Browser navigation failed"); }
                let loader = result["loaderId"].as_str().map(str::to_owned);
                let until = Instant::now() + Duration::from_secs(15);
                while loader.as_ref().is_some_and(|id| !self.loaded_documents.contains(id)) && Instant::now() < until {
                    self.call("Runtime.evaluate", json!({"expression":"document.readyState","returnByValue":true}))?;
                    std::thread::sleep(Duration::from_millis(30));
                }
                Ok(json!({"navigated":true,"dom_content_loaded":loader.as_ref().is_none_or(|id|self.loaded_documents.contains(id)),"note":"Snapshot reflects the current document; dynamic applications may continue loading."}))
            }
            "network" => { self.call("Runtime.evaluate",json!({"expression":"0"}))?; Ok(json!({"entries":self.network,"headers_and_cookies":"Only selected security headers; cookies and authorization are not captured"})) }
            "console" => { self.call("Runtime.evaluate",json!({"expression":"0"}))?; Ok(json!({"entries":self.console})) }
            "source" => {
                let id=args["request_id"].as_str().context("CDP source needs a request_id from network output")?;
                if !self.network.iter().any(|e|e["request_id"]==id && e["event"]=="response") { anyhow::bail!("Request ID not observed in this dedicated tab"); }
                let body=self.call("Network.getResponseBody",json!({"requestId":id}))?;
                if body["body"].as_str().unwrap_or("").len()>128_000 { anyhow::bail!("Response body exceeds limit"); }
                Ok(body)
            }
            "screenshot" => self.call("Page.captureScreenshot",json!({"format":"jpeg","quality":60,"captureBeyondViewport":false})),
            "close" => { self.call("Page.close",json!({}))?; self.alive=false; Ok(json!({"closed":true})) }
            "snapshot"|"click"|"fill"|"evaluate" => {
                self.evaluate(format!("(async()=>{{for(let i=0;i<100&&document.readyState==='loading';i++)await new Promise(r=>setTimeout(r,50));return ({})({},{});}})()",super::PAGE_CODE,serde_json::to_string(action)?,args))
            }
            _ => anyhow::bail!("Unsupported CDP action"),
        }
    }
}
impl Drop for Cdp {
    fn drop(&mut self) {
        if self.child.is_some() {
            self.ws
                .send(Message::Text(
                    json!({"id":999999,"method":"Browser.close"})
                        .to_string()
                        .into(),
                ))
                .ok();
        } else if self.alive {
            self.ws
                .send(Message::Text(
                    json!({"id":999998,"method":"Page.close"})
                        .to_string()
                        .into(),
                ))
                .ok();
        }
        self.ws.close(None).ok();
        if let Some(mut child) = self.child.take() {
            for _ in 0..20 {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            child.kill().ok();
            child.wait().ok();
        }
    }
}
pub(super) fn find_browser() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("BLUE_DRAGON_BROWSER_BIN") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    for name in [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "chrome",
        "msedge",
    ] {
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            let p = dir.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            });
            if p.is_file() {
                return Some(p);
            }
        }
    }
    for var in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
        if let Some(root) = std::env::var_os(var) {
            for suffix in [
                "Google/Chrome/Application/chrome.exe",
                "Microsoft/Edge/Application/msedge.exe",
            ] {
                let p = PathBuf::from(&root).join(suffix);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}
