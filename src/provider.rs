use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;

pub struct Provider {
    client: Client,
    url: String,
    model: String,
    key: Option<String>,
    anthropic: bool,
}

impl Provider {
    pub fn new(base_url: &str, model: String, key: Option<String>) -> Result<Self> {
        let parsed = reqwest::Url::parse(base_url).context("Invalid provider URL")?;
        let anthropic = parsed.host_str() == Some("api.anthropic.com");
        let url = if anthropic {
            format!("{}/messages", base_url)
        } else {
            format!("{}/chat/completions", base_url)
        };
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            url,
            model,
            key,
            anthropic,
        })
    }

    pub fn complete(&self, messages: &[Value], tools: Value) -> Result<Value> {
        self.request(messages, tools, false, |_| {}, || false)
    }
    pub fn stream(
        &self,
        messages: &[Value],
        tools: Value,
        delta: impl FnMut(&str),
        cancelled: impl FnMut() -> bool,
    ) -> Result<Value> {
        self.request(messages, tools, true, delta, cancelled)
    }
    fn request(
        &self,
        messages: &[Value],
        tools: Value,
        stream: bool,
        delta: impl FnMut(&str),
        cancelled: impl FnMut() -> bool,
    ) -> Result<Value> {
        let mut payload = if self.anthropic {
            crate::anthropic::request_body(messages, &tools, &self.model)?
        } else {
            json!({
                "model": self.model,
                "messages": messages,
                "tools": tools,
                "tool_choice": "auto",
                "temperature": 0.2
            })
        };
        if stream {
            payload["stream"] = json!(true);
        }
        let mut req = self.client.post(&self.url).json(&payload);
        if self.anthropic {
            req = req.header("anthropic-version", "2023-06-01");
        }
        if let Some(ref key) = self.key {
            req = if self.anthropic {
                req.header("x-api-key", key.as_str())
            } else {
                req.bearer_auth(key)
            };
        }
        let response = req.send().context("Provider request failed")?;
        let code = response.status();
        if code.is_success() && stream {
            if !response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .starts_with("text/event-stream")
            {
                anyhow::bail!("Provider did not return SSE; use --no-stream for this endpoint");
            }
            return crate::streaming::decode(response, self.anthropic, delta, cancelled);
        }
        let mut bytes = Vec::new();
        response
            .take(4_000_001)
            .read_to_end(&mut bytes)
            .context("Could not read provider response")?;
        if bytes.len() > 4_000_000 {
            anyhow::bail!("Provider response exceeded size limit");
        }
        if !code.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            // Never print entire error payload; providers can return echoed credentials or request bodies.
            anyhow::bail!(
                "Provider returned HTTP {}: {}",
                code.as_u16(),
                redact_error(&body)
            );
        }
        let value: Value = serde_json::from_slice(&bytes).context("Invalid provider JSON")?;
        if self.anthropic {
            crate::anthropic::decode_response(&value)
        } else {
            value
                .pointer("/choices/0/message")
                .cloned()
                .context("Response missing choices[0].message")
        }
    }
}

fn redact_error(text: &str) -> String {
    let clean = text.chars().take(200).collect::<String>();
    if clean.to_ascii_lowercase().contains("sk-") || clean.to_ascii_lowercase().contains("bearer") {
        "provider rejected request (details suppressed)".to_string()
    } else {
        "provider rejected request (response payload suppressed)".to_owned()
    }
}
