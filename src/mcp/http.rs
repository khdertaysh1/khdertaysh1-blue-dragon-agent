//! MCP Streamable HTTP. Each outbound message has exactly one POST.
use super::*;
use reqwest::blocking::{Client as Http, Response};
use reqwest::header::{HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::{redirect::Policy, Url};

pub(super) fn endpoint(raw: &str) -> Result<Url> {
    let url = Url::parse(raw)?;
    let host = url.host_str().unwrap_or("");
    let local = host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    anyhow::ensure!(
        raw.len() <= 2048
            && (url.scheme() == "https" || (url.scheme() == "http" && local))
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none(),
        "MCP requires an HTTPS or loopback HTTP endpoint without embedded credentials or fragment"
    );
    Ok(url)
}

#[derive(Clone)]
pub(super) struct Transport {
    client: Http,
    url: Url,
    bearer: Option<HeaderValue>,
    session: Option<HeaderValue>,
    pub version: Option<String>,
    pub cancelled: Arc<AtomicBool>,
    next_id: u64,
}

impl Transport {
    pub fn new(config: &ServerConfig, cancelled: Arc<AtomicBool>) -> Result<Self> {
        let url = endpoint(config.url.as_deref().context("Missing MCP URL")?)?;
        let bearer = config
            .bearer_env
            .as_ref()
            .map(|name| -> Result<_> {
                let token =
                    std::env::var(name).context("MCP bearer environment variable missing")?;
                anyhow::ensure!(
                    !token.is_empty() && token.len() <= 8192,
                    "Invalid MCP bearer token"
                );
                let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
                    .context("Invalid MCP bearer token")?;
                value.set_sensitive(true);
                Ok(value)
            })
            .transpose()?;
        let client = Http::builder()
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            client,
            url,
            bearer,
            session: None,
            version: None,
            cancelled,
            next_id: 1,
        })
    }

    pub fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.exchange(serde_json::to_value(JsonRpcRequest::new(
            id,
            method,
            Some(params),
        ))?)
    }

    pub fn exchange(&mut self, msg: Value) -> Result<Value> {
        anyhow::ensure!(
            !self.cancelled.load(Ordering::Relaxed),
            "MCP operation cancelled"
        );
        let bytes = serde_json::to_vec(&msg)?;
        anyhow::ensure!(bytes.len() <= 65536, "MCP request exceeds 64 KiB");
        let mut post = self
            .client
            .post(self.url.clone())
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .body(bytes);
        if let Some(v) = &self.session {
            post = post.header("Mcp-Session-Id", v);
        }
        if let Some(v) = &self.version {
            post = post.header("MCP-Protocol-Version", v);
        }
        if let Some(v) = &self.bearer {
            post = post.header(reqwest::header::AUTHORIZATION, v);
        }
        let initialize = msg["method"] == "initialize";
        let id = msg["id"].as_u64();
        let cancel = self.cancelled.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("mcp-http".into())
            .spawn(move || {
                let result = (|| {
                    anyhow::ensure!(!cancel.load(Ordering::Relaxed), "MCP operation cancelled");
                    let response = post
                        .send()
                        .map_err(|_| anyhow::anyhow!("MCP HTTP exchange failed"))?;
                    anyhow::ensure!(
                        response.status().is_success(),
                        "MCP HTTP status {}",
                        response.status().as_u16()
                    );
                    let session = if initialize {
                        response.headers().get("Mcp-Session-Id").cloned()
                    } else {
                        None
                    };
                    if let Some(ref v) = session {
                        anyhow::ensure!(
                            !v.is_empty()
                                && v.as_bytes().len() <= 1024
                                && v.as_bytes().iter().all(|b| (0x21..=0x7e).contains(b)),
                            "Invalid MCP session ID"
                        );
                    }
                    let value = if let Some(id) = id {
                        read_response(response, id, &cancel)?
                    } else {
                        anyhow::ensure!(
                            response.status().as_u16() == 202,
                            "Expected HTTP 202 for MCP notification"
                        );
                        Value::Null
                    };
                    Ok((session, value))
                })();
                let _ = tx.send(result);
            })?;
        loop {
            anyhow::ensure!(
                !self.cancelled.load(Ordering::Relaxed),
                "Task cancelled; MCP outcome may be unknown"
            );
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => {
                    let (session, value) = result?;
                    if initialize {
                        self.session = session;
                    }
                    return Ok(value);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => bail!("MCP HTTP worker disconnected"),
            }
        }
    }
}

fn rpc(value: Value, id: u64) -> Result<Option<Value>> {
    anyhow::ensure!(value["jsonrpc"] == "2.0", "Invalid JSON-RPC version");
    if value["method"].is_string() && value.get("id").is_none() {
        return Ok(None);
    }
    anyhow::ensure!(
        value["id"].as_u64() == Some(id) && value.get("method").is_none(),
        "Unexpected MCP response identity"
    );
    reject_rpc_error(&value)?;
    Ok(Some(
        value.get("result").context("Missing MCP result")?.clone(),
    ))
}

fn read_response(response: Response, id: u64, cancel: &AtomicBool) -> Result<Value> {
    const LIMIT: u64 = 1_048_576;
    let media = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let mut reader = BufReader::new(response.take(LIMIT + 1));
    if media == "application/json" {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() as u64 <= LIMIT,
            "MCP HTTP response exceeds 1 MiB"
        );
        return rpc(serde_json::from_slice(&bytes)?, id)?.context("Missing MCP response");
    }
    anyhow::ensure!(
        media == "text/event-stream",
        "Unsupported MCP response content type"
    );
    let mut data = String::new();
    let mut size = 0;
    loop {
        anyhow::ensure!(!cancel.load(Ordering::Relaxed), "MCP SSE cancelled");
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        size += n;
        anyhow::ensure!(
            size <= LIMIT as usize && n > 0,
            "MCP SSE truncated or over limit"
        );
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() && !data.is_empty() {
            if let Some(result) = rpc(serde_json::from_str(&data)?, id)? {
                return Ok(result);
            }
            data.clear();
        } else if let Some(field) = line.strip_prefix("data:") {
            data.push_str(field.strip_prefix(' ').unwrap_or(field));
            data.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_endpoint_and_matching_rpc_envelope() {
        for raw in [
            "http://example.com/mcp",
            "https://user:secret@example.com/mcp",
            "file:///tmp/mcp",
            "https://example.com/mcp#part",
        ] {
            assert!(endpoint(raw).is_err());
        }
        for raw in [
            "http://[::1]:9000/mcp",
            "http://127.0.0.1/mcp",
            "https://example.com/mcp",
        ] {
            assert!(endpoint(raw).is_ok());
        }
        assert!(rpc(json!({"jsonrpc":"2.0","id":3,"result":{}}), 4).is_err());
        let error = rpc(
            json!({"jsonrpc":"2.0","id":4,"error":{"code":-32602,"message":"private details"}}),
            4,
        )
        .unwrap_err();
        assert_eq!(remote_error(&error).unwrap()["rpc_code"], -32602);
    }
}
