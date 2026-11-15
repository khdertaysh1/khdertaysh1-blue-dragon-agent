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
    response_limit: u64,
    delegated: bool,
}

impl Provider {
    pub fn new(base_url: &str, model: String, key: Option<String>) -> Result<Self> {
        let parsed = reqwest::Url::parse(base_url).context("Invalid provider URL")?;
        if parsed.host_str() == Some("api.jarvisclaw.ai")
            && (model != crate::models::FREE_MODEL || key.is_some())
        {
            anyhow::bail!(
                "JarvisClaw free access requires auto/free and no credential; use --provider free"
            );
        }
        if matches!(
            parsed.host_str(),
            Some("ai-gateway.kilo.ai" | "api.kilo.ai")
        ) && (!crate::models::kilo_id(&model) || key.is_some())
        {
            anyhow::bail!(
                "Kilo free access requires a free model ID and no credential; use --provider kilo"
            );
        }
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
            response_limit: 4_000_000,
            delegated: false,
        })
    }

    pub fn delegated(base_url: &str, model: String, key: Option<String>) -> Result<Self> {
        let mut provider = Self::new(base_url, model, key)?;
        provider.delegated = true;
        provider.response_limit = 65536;
        provider.client = Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(provider)
    }

    pub fn vision_destination(&self) -> Option<String> {
        crate::vision::supported(&self.url, &self.model)
            .then(|| format!("{} (model {})", self.url, self.model))
    }
    pub fn inspect_image(&self, bytes: &[u8], question: &str) -> Result<String> {
        let payload = crate::vision::payload(&self.url, &self.model, bytes, question)?;
        self.send_image_request(&payload)
    }
    fn send_image_request(&self, payload: &Value) -> Result<String> {
        let mut req = self.client.post(&self.url).json(payload);
        if self.anthropic {
            req = req.header("anthropic-version", "2023-06-01");
        }
        if let Some(key) = &self.key {
            req = if self.anthropic {
                req.header("x-api-key", key)
            } else {
                req.bearer_auth(key)
            };
        }
        let response = req
            .send()
            .context("Image request failed; transmission may have occurred, no automatic retry")?;
        if !response.status().is_success() {
            anyhow::bail!(
                "Image provider returned HTTP {}; no automatic retry",
                response.status()
            );
        }
        let mut bytes = Vec::new();
        response.take(65_537).read_to_end(&mut bytes)?;
        if bytes.len() > 65_536 {
            anyhow::bail!("Image response exceeds limit");
        }
        crate::vision::description(&serde_json::from_slice(&bytes)?, self.anthropic)
    }
    pub fn model(&self) -> &str {
        &self.model
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
        if self.url.starts_with("https://api.jarvisclaw.ai/")
            || self.url.starts_with(crate::models::KILO_BASE)
        {
            payload["max_tokens"] = json!(4096);
        }
        if self.delegated {
            payload["max_tokens"] = json!(2048);
        }
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
        if !code.is_success() {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let mut bytes = Vec::new();
            response.take(16_385).read_to_end(&mut bytes)?;
            let body = if bytes.len() <= 16_384 {
                String::from_utf8_lossy(&bytes).into_owned()
            } else {
                String::new()
            };
            return Err(crate::rotation::ProviderFailure::from_response(
                code.as_u16(),
                retry_after,
                &body,
            )
            .into());
        }
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
            .take(self.response_limit + 1)
            .read_to_end(&mut bytes)
            .context("Could not read provider response")?;
        if bytes.len() as u64 > self.response_limit {
            anyhow::bail!("Provider response exceeded size limit");
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_wire_request_sends_pixels_and_bounds_response_without_retry() {
        let endpoint = "https://api.openai.com/v1/chat/completions";
        let payload = crate::vision::payload(
            endpoint,
            "gpt-4o",
            &[0xff, 0xd8, 0xff, 0xd9],
            "Describe layout",
        )
        .unwrap();
        for (status, body, expected) in [
            (200, json!({"choices":[{"finish_reason":"stop","message":{"content":"Visible button"}}]}).to_string(), true),
            (429, "rate limited".into(), false),
            (200, "x".repeat(65_537), false),
            (200, json!({"choices":[{"finish_reason":"length","message":{"content":"partial"}}]}).to_string(), false),
        ] {
            let (base, server) = crate::test_support::http_status(vec![(status, "application/json", body)]);
            let provider = Provider::new(&base, "gpt-4o".into(), None).unwrap();
            // Exercise the real HTTP serialization/response path with an already
            // validated official payload. The public API still rejects localhost.
            assert!(provider.inspect_image(&[0xff, 0xd8, 0xff, 0xd9], "x").is_err());
            let result = provider.send_image_request(&payload);
            assert_eq!(result.is_ok(), expected);
            let requests = server.join().unwrap();
            assert_eq!(requests, vec![payload.clone()]);
            assert!(requests[0].get("tools").is_none());
        }
    }
    #[test]
    fn anonymous_gateway_rejects_credentials_and_paid_models() {
        assert!(
            Provider::new(crate::models::KILO_BASE, "kilo-auto/frontier".into(), None).is_err()
        );
        assert!(Provider::new(
            crate::models::KILO_BASE,
            crate::models::KILO_MODEL.into(),
            Some("unrelated-test-key".into())
        )
        .is_err());
        assert!(Provider::new(
            crate::models::KILO_BASE,
            crate::models::KILO_MODEL.into(),
            None
        )
        .is_ok());
        assert!(Provider::new(crate::models::FREE_BASE, "paid".into(), None).is_err());
        assert!(Provider::new(
            crate::models::FREE_BASE,
            crate::models::FREE_MODEL.into(),
            Some("unrelated-test-key".into())
        )
        .is_err());
        assert!(Provider::new(
            crate::models::FREE_BASE,
            crate::models::FREE_MODEL.into(),
            None
        )
        .is_ok());
    }
}
