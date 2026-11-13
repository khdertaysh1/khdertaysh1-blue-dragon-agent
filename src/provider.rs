use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration;

pub struct Provider {
    client: Client,
    url: String,
    model: String,
    key: Option<String>,
}

impl Provider {
    pub fn new(base_url: &str, model: String, key: Option<String>) -> Result<Self> {
        let url = format!("{}/chat/completions", base_url);
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            url,
            model,
            key,
        })
    }

    pub fn complete(&self, messages: &[Value], tools: Value) -> Result<Value> {
        let payload = json!({
            "model": self.model,
            "messages": messages,
            "tools": tools,
            "tool_choice": "auto",
            "temperature": 0.2
        });
        let mut req = self.client.post(&self.url).json(&payload);
        if let Some(ref key) = self.key {
            req = req.bearer_auth(key);
        }
        let response = req.send().context("Provider request failed")?;
        let code = response.status();
        let bytes = response
            .bytes()
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
        value
            .pointer("/choices/0/message")
            .cloned()
            .context("Response missing choices[0].message")
    }
}

fn redact_error(text: &str) -> String {
    let clean = text.chars().take(200).collect::<String>();
    if clean.to_ascii_lowercase().contains("sk-") || clean.to_ascii_lowercase().contains("bearer") {
        "provider rejected request (details suppressed)".to_string()
    } else {
        clean
    }
}
