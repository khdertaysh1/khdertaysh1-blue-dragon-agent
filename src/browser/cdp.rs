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

fn record_network(events: &mut VecDeque<Value>, evictions: &mut usize, value: &Value) {
    let p = &value["params"];
    if value["method"] == "Network.loadingFinished" {
        // A body queried before completion may become readable at this point.
        if let Some(row) = events
            .iter_mut()
            .rev()
            .find(|row| row["event"] == "response" && row["request_id"] == p["requestId"])
        {
            if let Some(obj) = row.as_object_mut() {
                obj.remove("body_unavailable");
            }
        }
    }
    if value["method"] == "Network.requestWillBeSent" && p["redirectResponse"].is_object() {
        let redirect = &p["redirectResponse"];
        let url = redirect["url"].as_str();
        push_event(
            events,
            evictions,
            json!({"event":"redirect","request_id":p["requestId"],
            "url":url.map(|u|u.chars().take(2000).collect::<String>()),
            "url_truncated":url.is_none_or(|u|u.chars().count()>2000),"status":redirect["status"]}),
        );
    }
    let event = match value["method"].as_str() {
        Some("Network.requestWillBeSent") => Some(
            json!({"event":"request","request_id":p["requestId"],"url":p["request"]["url"].as_str().unwrap_or("").chars().take(2000).collect::<String>(),"url_truncated":p["request"]["url"].as_str().is_none_or(|u|u.chars().count()>2000),"method":p["request"]["method"],"type":p["type"]}),
        ),
        Some("Network.responseReceived") => Some(
            json!({"event":"response","request_id":p["requestId"],"url":p["response"]["url"].as_str().unwrap_or("").chars().take(2000).collect::<String>(),"url_truncated":p["response"]["url"].as_str().is_none_or(|url|url.chars().count()>2000),"status":p["response"]["status"],"mime_type":p["response"]["mimeType"],"security_header_indicators":super::inspection::security_header_indicators(&p["response"]["headers"])}),
        ),
        Some("Network.loadingFailed") => Some(json!({"event":"error","request_id":p["requestId"],
            "error":p["errorText"].as_str().unwrap_or("").chars().take(200).collect::<String>()})),
        _ => None,
    };
    if let Some(event) = event {
        push_event(events, evictions, event);
    }
}

fn push_event(events: &mut VecDeque<Value>, evictions: &mut usize, event: Value) {
    events.push_back(event);
    if events.len() > 300 {
        events.pop_front();
        *evictions = evictions.saturating_add(1);
    }
}
fn network_snapshot(events: &VecDeque<Value>, evictions: usize) -> Value {
    let mut rows = Vec::new();
    let mut used = 0;
    for event in events.iter().rev() {
        let size = event.to_string().len() + 1;
        if used + size > 180_000 {
            break;
        }
        used += size;
        rows.push(event);
    }
    rows.reverse();
    json!({"entries":rows,"entries_omitted":events.len()-rows.len(),"ring_entries_omitted":evictions,"ring_omission_scope":"dedicated CDP tab since connection start","header_values_and_cookies":"Not retained; bounded policy indicators only"})
}
pub struct Cdp {
    ws: WebSocket<TcpStream>,
    id: u64,
    network: VecDeque<Value>,
    network_evictions: usize,
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
            network_evictions: 0,
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
        record_network(&mut self.network, &mut self.network_evictions, value);
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
        } else if value["method"] == "Runtime.exceptionThrown" {
            let details = &p["exceptionDetails"];
            let frame = &details["stackTrace"]["callFrames"][0];
            let location = |key: &str| details.get(key).or_else(|| frame.get(key));
            let msg = details["exception"]["description"]
                .as_str()
                .or(details["text"].as_str())
                .unwrap_or("Uncaught exception");
            self.console.push_back(json!({"level":"error","kind":"uncaught_exception",
                "text":msg.lines().next().unwrap_or("").chars().take(1000).collect::<String>(),
                "url":location("url").and_then(Value::as_str).map(|u|u.chars().take(2000).collect::<String>()),
                "line":location("lineNumber").and_then(Value::as_u64).and_then(|v|v.checked_add(1)),
                "column":location("columnNumber").and_then(Value::as_u64).and_then(|v|v.checked_add(1))}));
        }
        if self.console.len() > 200 {
            self.console.pop_front();
        }
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        let id = self.id;
        if self
            .ws
            .send(Message::Text(
                json!({"id":id,"method":method,"params":params})
                    .to_string()
                    .into(),
            ))
            .is_err()
        {
            self.alive = false;
            anyhow::bail!(
                "Browser connection interrupted while sending; action outcome unknown; no retry"
            );
        }
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until {
            match self.ws.read() {
                Ok(Message::Text(text)) => {
                    let value: Value = serde_json::from_str(&text)?;
                    if value["id"] == id {
                        if let Some(error) = value.get("error") {
                            if method == "Network.getResponseBody" {
                                let code = error["code"]
                                    .as_i64()
                                    .map(|n| n.to_string())
                                    .unwrap_or_else(|| "unknown".to_owned());
                                let raw = error["message"]
                                    .as_str()
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or("No protocol message supplied");
                                let reason: String =
                                    raw.chars().take(240).flat_map(char::escape_debug).collect();
                                let message = format!("Network.getResponseBody (CDP {code}): {reason}. CDP could not provide the body. No file saved.");
                                if error["code"] == -32000
                                    && (raw.contains("No resource with given identifier")
                                        || raw.contains("evicted from inspector cache"))
                                {
                                    if let Some(row) = self.network.iter_mut().rev().find(|row| {
                                        row["event"] == "response"
                                            && row["request_id"] == params["requestId"]
                                    }) {
                                        row["body_unavailable"] = json!(true);
                                    }
                                    return Err(super::ResponseBodyUnavailable(message).into());
                                }
                                anyhow::bail!(message);
                            }
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
        self.alive = false;
        anyhow::bail!("Browser action deadline exceeded; outcome unknown; reconnect explicitly and inspect before retry")
    }
    pub fn source_url(&self, request_id: &str) -> Result<&str> {
        observed_response_url(&self.network, request_id)
    }
    pub fn script_request_id(&self, url: &str) -> Result<&str> {
        let mut matches = self
            .network
            .iter()
            .filter(|row| {
                row["event"] == "response"
                    && row["body_unavailable"] != true
                    && row["url_truncated"] == false
                    && row["url"] == url
                    && row["status"]
                        .as_f64()
                        .is_some_and(|n| (200.0..300.0).contains(&n))
                    && row["mime_type"].as_str().is_some_and(is_javascript_mime)
            })
            .filter_map(|row| row["request_id"].as_str());
        let id = matches
            .next()
            .context("No retained successful script response for this exact URL")?;
        anyhow::ensure!(
            matches.all(|other| other == id),
            "Multiple request IDs for this URL; select the intended observed request ID"
        );
        Ok(id)
    }

    pub fn javascript_response(&self, id: &str) -> Result<(&str, u16)> {
        let row = self
            .network
            .iter()
            .rev()
            .find(|e| e["event"] == "response" && e["request_id"] == id)
            .context("Response ID was not observed in this tab")?;
        let status = row["status"].as_f64().unwrap_or(0.0);
        let mime = row["mime_type"].as_str().unwrap_or_default();
        anyhow::ensure!(
            (200.0..300.0).contains(&status) && is_javascript_mime(mime),
            "Only successful JavaScript responses can be saved"
        );
        Ok((self.source_url(id)?, status as u16))
    }

    pub fn response_bytes(&mut self, id: &str) -> Result<Vec<u8>> {
        use base64::Engine;
        const MAX: usize = 1024 * 1024;
        self.javascript_response(id)?;
        let body = self.call("Network.getResponseBody", json!({"requestId":id}))?;
        let text = body["body"].as_str().context("Response body unavailable")?;
        let bytes = match body["base64Encoded"].as_bool() {
            Some(false) if text.len() <= MAX => text.as_bytes().to_vec(),
            Some(true) if text.len() <= MAX.div_ceil(3) * 4 => {
                base64::engine::general_purpose::STANDARD.decode(text)?
            }
            _ => anyhow::bail!("Response body exceeds 1 MiB or has unknown encoding"),
        };
        anyhow::ensure!(bytes.len() <= MAX, "Response body exceeds 1 MiB");
        Ok(bytes)
    }

    fn evaluate(&mut self, expression: String) -> Result<Value> {
        let result=self.call("Runtime.evaluate",json!({"expression":expression,"returnByValue":true,"awaitPromise":true,"timeout":15000}))?;
        if result.get("exceptionDetails").is_some() {
            anyhow::bail!("Page JavaScript failed (untrusted error details omitted)");
        }
        Ok(result["result"]["value"].clone())
    }
    fn document_identity(&mut self) -> Result<Value> {
        let tree = self.call("Page.getFrameTree", json!({}))?;
        let frame = &tree["frameTree"]["frame"];
        Ok(json!([frame["url"], frame["loaderId"]]))
    }
    pub fn action(&mut self, action: &str, args: &Value) -> Result<Value> {
        match action {
            "open" | "navigate" => {
                super::validate_url(args["url"].as_str().context("Missing URL")?)?;
                if args["fresh_document"] == true {
                    self.call("Page.navigate", json!({"url":"about:blank"}))?;
                }
                let result = self.call("Page.navigate", json!({"url":args["url"]}))?;
                if result.get("errorText").is_some() {
                    anyhow::bail!("Browser navigation failed");
                }
                let loader = result["loaderId"].as_str().map(str::to_owned);
                let until = Instant::now() + Duration::from_secs(15);
                while loader
                    .as_ref()
                    .is_some_and(|id| !self.loaded_documents.contains(id))
                    && Instant::now() < until
                {
                    self.call(
                        "Runtime.evaluate",
                        json!({"expression":"document.readyState","returnByValue":true}),
                    )?;
                    std::thread::sleep(Duration::from_millis(30));
                }
                Ok(
                    json!({"navigated":true,"document_identity":self.document_identity().ok(),"dom_content_loaded":loader.as_ref().is_none_or(|id|self.loaded_documents.contains(id)),"note":"Snapshot reflects the current document; dynamic applications may continue loading."}),
                )
            }
            "network" => {
                let identity = self.document_identity()?;
                let mut rows = network_snapshot(&self.network, self.network_evictions);
                rows["document_identity"] = identity;
                Ok(rows)
            }
            "document_identity" => self.document_identity(),
            "console" => {
                let identity = self.document_identity()?;
                Ok(json!({"entries":self.console,"document_identity":identity}))
            }
            "source" => {
                let id = args["request_id"]
                    .as_str()
                    .context("CDP source needs a request_id from network output")?;
                if !self
                    .network
                    .iter()
                    .any(|e| e["request_id"] == id && e["event"] == "response")
                {
                    anyhow::bail!("Request ID not observed in this dedicated tab");
                }
                let body = self.call("Network.getResponseBody", json!({"requestId":id}))?;
                if body["body"].as_str().unwrap_or("").len() > 128_000 {
                    anyhow::bail!("Response body exceeds limit");
                }
                Ok(body)
            }
            "screenshot" => self.call(
                "Page.captureScreenshot",
                json!({"format":"jpeg","quality":60,"captureBeyondViewport":false}),
            ),
            "close" => {
                self.call("Page.close", json!({}))?;
                self.alive = false;
                Ok(json!({"closed":true}))
            }
            "snapshot" | "wait_for" | "click" | "fill" | "select" | "set_checked" | "scroll"
            | "evaluate" => {
                let mut page_args = args.clone();
                let mut cursor = 0;
                if action == "click" {
                    // Drain earlier protocol events while the tab is still untouched.
                    self.document_identity()?;
                    cursor = self.network_evictions.saturating_add(self.network.len());
                    page_args["observe_after_click"] = json!(true);
                }
                let mut result=self.evaluate(format!("(async()=>{{for(let i=0;i<100&&document.readyState==='loading';i++)await new Promise(r=>setTimeout(r,50));return ({})({},{});}})()",super::PAGE_CODE,serde_json::to_string(action)?,page_args))?;
                if action == "click" && result["clicked"] == true {
                    let count = self
                        .network
                        .len()
                        .saturating_sub(cursor.saturating_sub(self.network_evictions));
                    let events: Vec<_> = self.network.iter().skip(self.network.len() - count).take(16)
                        .map(|row| json!({"event":row["event"],"request_id":row["request_id"],"status":row["status"]})).collect();
                    result["network_interval"] = json!({"events":events,"retained_event_count":count,
                        "preview_omitted":count.saturating_sub(16),"ring_gap_possible":self.network_evictions>cursor,
                        "causal_link_verified":false});
                }
                if matches!(action, "snapshot" | "wait_for") && args.get("frame_selector").is_none()
                {
                    let identity = self.document_identity()?;
                    if identity[0] == result["url"] {
                        result["document_identity"] = identity;
                    }
                }
                Ok(result)
            }
            _ => anyhow::bail!("Unsupported CDP action"),
        }
    }
}
fn is_javascript_mime(mime: &str) -> bool {
    matches!(
        mime.split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "application/javascript"
            | "text/javascript"
            | "application/ecmascript"
            | "text/ecmascript"
            | "application/x-javascript"
    )
}

fn observed_response_url<'a>(events: &'a VecDeque<Value>, id: &str) -> Result<&'a str> {
    let response = events
        .iter()
        .rev()
        .find(|event| event["event"] == "response" && event["request_id"] == id).context("No retained response with this ID on the current connection; historical captures are separate")?;
    if response["body_unavailable"] == true {
        return Err(super::ResponseBodyUnavailable("CDP no longer retains this response body. Do not retry this request ID or navigate to the script. Use a newly observed response if available, or report that the bytes are unavailable.".into()).into());
    }
    if response["url_truncated"] == true {
        anyhow::bail!("Recorded response URL is truncated; scope cannot be verified");
    }
    response["url"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("Recorded response has no URL")
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

#[cfg(test)]
mod source_scope_tests {
    use super::*;
    #[test]
    fn cdp_ring_omissions_reach_policy_engine_from_producer() {
        let mut events = VecDeque::new();
        let mut evictions = 0;
        for n in 0..301 {
            record_network(
                &mut events,
                &mut evictions,
                &json!({"method":"Network.responseReceived","params":{"requestId":n.to_string(),"response":{"url":"https://example.test/","status":200,"headers":{"X-Frame-Options":"DENY"}}}}),
            );
        }
        let network = network_snapshot(&events, evictions);
        assert_eq!(network["entries"].as_array().unwrap().len(), 300);
        assert_eq!(network["ring_entries_omitted"], 1);
        assert_eq!(network["entries_omitted"], 0);
        let root = tempfile::tempdir().unwrap();
        let mut session = crate::session::new(root.path(), "test");
        session
            .scope
            .command("allow https://example.test/ read")
            .unwrap();
        session.messages.push(json!({"role":"assistant","tool_calls":[{"id":"c","function":{"name":"browser","arguments":"{\"action\":\"network\"}"}}]}));
        let result = json!({"ok":true,"result":network});
        session
            .observations
            .push(crate::reasoning::observe(1, "browser", &result));
        session
            .messages
            .push(json!({"role":"tool","tool_call_id":"c","content":result.to_string()}));
        let out = crate::audit_engines::review(
            &session,
            &json!({"engine":"response_policy_review","evidence_id":"E000001"}),
        )
        .unwrap();
        assert_eq!(out["review"]["upstream_capture_partial"], true);
        assert_eq!(out["review"]["upstream_capture_completeness_known"], true);
        assert_eq!(out["vulnerability_confirmed"], false);
        for event in &mut events {
            event["url"] = json!("x".repeat(2000));
        }
        let bounded = network_snapshot(&events, evictions);
        assert!(bounded["entries_omitted"].as_u64().unwrap() > 0);
        assert!(bounded.to_string().len() < 181000);
    }
    #[test]
    fn uses_latest_response_location_and_refuses_truncated_provenance() {
        let mut events = VecDeque::from([
            json!({"event":"response","request_id":"r","url":"https://allowed.test/start"}),
            json!({"event":"response","request_id":"r","url":"https://outside.test/final"}),
        ]);
        assert_eq!(
            observed_response_url(&events, "r").ok(),
            Some("https://outside.test/final")
        );
        events.back_mut().unwrap()["url_truncated"] = json!(true);
        assert!(observed_response_url(&events, "r").is_err());
        assert!(observed_response_url(&events, "unknown").is_err());
    }
}
