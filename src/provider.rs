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
    coordinator: Option<crate::provider_budget::Coordinator>,
    metrics: std::sync::Mutex<Value>,
}

impl Provider {
    pub fn new(base_url: &str, model: String, key: Option<String>) -> Result<Self> {
        crate::provider_traffic::validate_env()?;
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
            coordinator: None,
            metrics: std::sync::Mutex::new(json!({"provider_instance_requests_sent":0})),
        })
    }

    fn http_failure(
        &self,
        response: reqwest::blocking::Response,
        messages: &[Value],
    ) -> anyhow::Error {
        let code = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| crate::provider_diagnostics::retry_after(v, chrono::Utc::now()));
        let request_id = ["x-request-id", "request-id", "x-amzn-requestid"]
            .iter()
            .find_map(|name| response.headers().get(*name).and_then(|v| v.to_str().ok()))
            .map(str::to_owned);
        let mut bytes = Vec::new();
        let body_read = response.take(16_385).read_to_end(&mut bytes);
        let body = if body_read.is_ok() && bytes.len() <= 16_384 {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            String::new()
        };
        let mut failure =
            crate::rotation::ProviderFailure::from_response(code.as_u16(), retry_after, &body);
        if body_read.is_err() {
            failure.reason =
                Some("Provider error body could not be read; HTTP status retained".into());
        } else if bytes.len() > 16_384 {
            failure.reason = Some("Provider error body exceeds diagnostic limit; omitted".into());
        }
        // Extract from the bounded complete body, redact BEFORE truncating.
        // Cutting a credential or echoed request first defeats exact redaction.
        failure.code = failure.code.map(|s| {
            crate::reasoning::context::clip(
                &crate::provider_diagnostics::scrub(&s, self.key.as_deref(), messages),
                128,
            )
        });
        failure.reason = failure
            .reason
            .map(|s| crate::provider_diagnostics::scrub(&s, self.key.as_deref(), messages));
        failure.request_id = request_id
            .map(|s| crate::provider_diagnostics::scrub(&s, self.key.as_deref(), messages));
        if let Some(coordinator) = &self.coordinator {
            if let Err(error) = coordinator.failure(&failure) {
                failure.model_specific = false;
                return anyhow::Error::new(failure).context(format!(
                    "Cooldown persistence failed; stop further provider work: {error}"
                ));
            }
        }
        failure.into()
    }
    pub fn cooldown_until(&self) -> Result<Option<u64>> {
        self.coordinator
            .as_ref()
            .map_or(Ok(None), |c| c.cooldown_until())
    }
    pub fn output_reserve(&self) -> u64 {
        crate::provider_traffic::output_limit(&self.url, self.delegated).unwrap_or(4096)
    }
    pub fn metrics(&self) -> Value {
        self.metrics.lock().unwrap().clone()
    }
    fn sent(&self, messages: &[Value], tools: &Value) {
        let mut metrics = self.metrics.lock().unwrap();
        let count = metrics["provider_instance_requests_sent"]
            .as_u64()
            .unwrap_or(0)
            + 1;
        *metrics = json!({"output_budget_source":if std::env::var_os("BLUE_DRAGON_MAX_OUTPUT_TOKENS").is_some(){"user_environment"}else{"client_default"},"provider_instance_requests_sent":count,"requested_model":crate::provider_diagnostics::scrub(&self.model,self.key.as_deref(),&[]),
            "history_bytes":messages.iter().map(|m|m.to_string().len()).sum::<usize>(),
            "tool_result_bytes":messages.iter().filter(|m|m["role"]=="tool").map(|m|m["content"].as_str().unwrap_or("").len()).sum::<usize>(),
            "max_output_tokens":crate::provider_traffic::output_limit(&self.url,self.delegated),
            "tool_definition_bytes":tools.to_string().len(),"estimated_tool_tokens":crate::reasoning::context::estimate(&tools.to_string()),
            "estimated_message_tokens":messages.iter().map(|m|crate::reasoning::context::estimate(&m.to_string())).sum::<usize>(),
            "usage_reported":false,"actual_model":null,"scope":"This provider instance only; children have separate instance counters and shared admission budgets. Estimates are not billed usage."});
    }
    fn audit_start(&self, kind: &str, stream: bool) {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut metrics = self.metrics.lock().unwrap();
        metrics["dispatch_id"] = json!(format!(
            "{}-{}-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            std::process::id(),
            SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        metrics["unix_ms"] = json!(chrono::Utc::now().timestamp_millis());
        metrics["kind"] = json!(kind);
        metrics["stream"] = json!(stream);
        metrics["attempts"] = json!(1);
        metrics["provider_origin"] = json!(reqwest::Url::parse(&self.url)
            .ok()
            .map(|u| u.origin().ascii_serialization()));
        metrics["outcome"] = json!("dispatch_started_outcome_unknown");
        crate::provider_traffic::record(metrics.clone());
    }
    fn response_metadata(&self, value: &Value) {
        let mut metrics = self.metrics.lock().unwrap();
        if let Some(model) = value["model"].as_str().filter(|s| {
            !s.is_empty()
                && s.len() <= 128
                && !self
                    .key
                    .as_deref()
                    .is_some_and(|key| !key.is_empty() && s.contains(key))
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_./:-".contains(&b))
        }) {
            metrics["actual_model"] = json!(model);
        }
        let mut usage = serde_json::Map::new();
        for key in [
            "input_tokens",
            "output_tokens",
            "prompt_tokens",
            "completion_tokens",
            "total_tokens",
        ] {
            if let Some(number) = value["usage"][key].as_u64() {
                usage.insert(key.into(), json!(number));
            }
        }
        metrics["usage_reported"] = json!(!usage.is_empty());
        metrics["usage"] = Value::Object(usage);
    }
    pub fn coordinated(mut self, home: &std::path::Path) -> Result<Self> {
        self.coordinator = Some(crate::provider_budget::Coordinator::new(
            home,
            &self.url,
            self.key.as_deref(),
            &self.model,
        )?);
        Ok(self)
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
        let started = chrono::Utc::now().timestamp_millis();
        let before = self.metrics()["provider_instance_requests_sent"]
            .as_u64()
            .unwrap_or(0);
        let result = self.send_image_request_inner(payload);
        let mut event = self.metrics();
        let sent = event["provider_instance_requests_sent"]
            .as_u64()
            .unwrap_or(0)
            > before;
        if !sent {
            event = json!({});
        }
        event["kind"] = json!("image_completion");
        event["unix_ms"] = json!(started);
        event["elapsed_ms"] = json!(chrono::Utc::now().timestamp_millis() - started);
        event["attempts"] = json!(u8::from(sent));
        event["provider_origin"] = json!(reqwest::Url::parse(&self.url)?
            .origin()
            .ascii_serialization());
        event["max_output_tokens"] = payload["max_tokens"].clone();
        event["outcome"] = json!(if !sent {
            "blocked_before_dispatch"
        } else if result.is_ok() {
            "completed"
        } else {
            "failed_or_incomplete"
        });
        if sent {
            *self.metrics.lock().unwrap() = event.clone();
        }
        crate::provider_traffic::record(event);
        result
    }
    fn send_image_request_inner(&self, payload: &Value) -> Result<String> {
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
        if let Some(coordinator) = &self.coordinator {
            coordinator.admit_tokens(
                crate::reasoning::context::estimate(&payload.to_string()) as u64
                    + payload["max_tokens"].as_u64().unwrap_or(4096),
            )?;
        }
        self.sent(
            payload["messages"]
                .as_array()
                .map_or(&[][..], Vec::as_slice),
            &json!([]),
        );
        self.metrics.lock().unwrap()["max_output_tokens"] = payload["max_tokens"].clone();
        self.audit_start("image_completion", false);
        let response = req
            .send()
            .context("Image request failed; transmission may have occurred, no automatic retry")?;
        self.metrics.lock().unwrap()["rate_limit_headers"] =
            crate::provider_traffic::headers(response.headers());
        self.metrics.lock().unwrap()["http_status"] = json!(response.status().as_u16());
        if !response.status().is_success() {
            return Err(self.http_failure(
                response,
                payload["messages"]
                    .as_array()
                    .map_or(&[][..], Vec::as_slice),
            ));
        }
        let mut bytes = Vec::new();
        response.take(65_537).read_to_end(&mut bytes)?;
        if bytes.len() > 65_536 {
            anyhow::bail!("Image response exceeds limit");
        }
        let value = serde_json::from_slice(&bytes)?;
        self.response_metadata(&value);
        crate::vision::description(&value, self.anthropic)
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
        let start = chrono::Utc::now().timestamp_millis();
        let before = self.metrics()["provider_instance_requests_sent"]
            .as_u64()
            .unwrap_or(0);
        let result = self.request_inner(messages, tools, stream, delta, cancelled);
        let mut event = self.metrics();
        let sent = event["provider_instance_requests_sent"]
            .as_u64()
            .unwrap_or(0)
            > before;
        if !sent {
            event = json!({});
        }
        event["unix_ms"] = json!(start);
        event["elapsed_ms"] = json!(chrono::Utc::now().timestamp_millis().saturating_sub(start));
        event["kind"] = json!(if self.delegated {
            "delegate_completion"
        } else {
            "completion"
        });
        event["provider_origin"] = json!(reqwest::Url::parse(&self.url)?
            .origin()
            .ascii_serialization());
        event["stream"] = json!(stream);
        event["attempts"] = json!(u8::from(sent));
        event["outcome"] = json!(if !sent {
            "blocked_before_dispatch"
        } else if result.is_ok() {
            "completed"
        } else {
            "failed_or_incomplete"
        });
        if let Some(failure) = result
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<crate::rotation::ProviderFailure>())
        {
            event["rate_limit_dimension"] = json!(if failure.code.as_deref()
                == Some("tpm_rate_limit_exceeded")
            {
                "tokens_per_minute"
            } else {
                "unknown"
            });
            event["limit_scope"] = json!(if failure.model_specific {
                "explicit_model"
            } else {
                "unknown_or_shared"
            });
            event["cooldown_seconds"] = json!(failure.retry_after);
            event["cooldown_source"] = json!(failure.retry_after_source);
        }
        event["estimated_output_tokens"] = result
            .as_ref()
            .ok()
            .map(|value| json!(crate::reasoning::context::estimate(&value.to_string())))
            .unwrap_or(Value::Null);
        if sent {
            *self.metrics.lock().unwrap() = event.clone();
        }
        crate::provider_traffic::record(event);
        result
    }
    fn request_inner(
        &self,
        messages: &[Value],
        tools: Value,
        stream: bool,
        delta: impl FnMut(&str),
        cancelled: impl FnMut() -> bool,
    ) -> Result<Value> {
        crate::provider_contract::preflight(messages, &tools, &self.model)?;
        let mut payload = if self.anthropic {
            crate::anthropic::request_body(messages, &tools, &self.model)?
        } else {
            json!({
                "model": self.model,
                "messages": messages,
                "tools": tools.clone(),
                "tool_choice": "auto",
                "temperature": 0.2
            })
        };
        if self.url.starts_with("https://api.jarvisclaw.ai/")
            || self.url.starts_with(crate::models::KILO_BASE)
        {
            payload["max_tokens"] = json!(4096);
        }
        if let Some(limit) = crate::provider_traffic::output_limit(&self.url, self.delegated) {
            payload["max_tokens"] = json!(limit);
        }
        if payload.get("max_tokens").is_none() {
            payload["max_tokens"] = json!(self.output_reserve());
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
        if let Some(coordinator) = &self.coordinator {
            let input = messages
                .iter()
                .map(|m| crate::reasoning::context::estimate(&m.to_string()) as u64)
                .sum::<u64>()
                + crate::reasoning::context::estimate(&tools.to_string()) as u64;
            coordinator.admit_tokens(
                input.saturating_add(payload["max_tokens"].as_u64().unwrap_or(4096)),
            )?;
        }
        self.sent(messages, &tools);
        self.metrics.lock().unwrap()["max_output_tokens"] = payload["max_tokens"].clone();
        self.audit_start(
            if self.delegated {
                "delegate_completion"
            } else {
                "completion"
            },
            stream,
        );
        let response = req.send().map_err(|_| {
            crate::provider_contract::incomplete("connection_failed_outcome_unknown", "")
        })?;
        self.metrics.lock().unwrap()["rate_limit_headers"] =
            crate::provider_traffic::headers(response.headers());
        let code = response.status();
        self.metrics.lock().unwrap()["http_status"] = json!(code.as_u16());
        if !code.is_success() {
            return Err(self.http_failure(response, messages));
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
            let mut message = crate::streaming::decode(response, self.anthropic, delta, cancelled)?;
            if let Some(metadata) = message
                .as_object_mut()
                .and_then(|m| m.remove("_blue_provider_metadata"))
            {
                self.response_metadata(&metadata);
                if let Some(shape) = metadata.get("stream_shape") {
                    self.metrics.lock().unwrap()["stream_shape"] = shape.clone();
                }
            }
            crate::provider_contract::validate_completion(&message, &tools)?;
            return Ok(message);
        }
        let mut bytes = Vec::new();
        response
            .take(self.response_limit + 1)
            .read_to_end(&mut bytes)
            .context("Could not read provider response")?;
        if bytes.len() as u64 > self.response_limit {
            anyhow::bail!("Provider response exceeded size limit");
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| crate::provider_contract::incomplete("malformed_response", ""))?;
        self.response_metadata(&value);
        let reason = if self.anthropic {
            value["stop_reason"].as_str().or_else(|| {
                value["content"]
                    .as_array()
                    .is_some_and(|parts| parts.iter().any(|p| p["type"] == "refusal"))
                    .then_some("refusal")
            })
        } else {
            value["choices"][0]["finish_reason"].as_str()
        };
        let message = if self.anthropic {
            crate::anthropic::decode_response(&value)?
        } else {
            value
                .pointer("/choices/0/message")
                .cloned()
                .context("Response missing choices[0].message")?
        };
        if let Some(kind) = reason.and_then(crate::provider_contract::stop_kind) {
            return Err(crate::provider_contract::incomplete_reason(
                kind,
                &crate::provider_contract::text(&message),
                reason.unwrap_or(""),
            ));
        }
        crate::provider_contract::validate_completion(&message, &tools)?;
        Ok(message)
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

#[cfg(test)]
mod reliability_tests {
    use super::*;
    fn tools() -> Value {
        json!([{"type":"function","function":{"name":"write_file","parameters":{"type":"object"}}}])
    }
    #[test]
    fn nonstream_stop_causes_and_bad_batches_never_become_executable_messages() {
        for (reason, expected) in [
            ("length", "response_limit"),
            ("content_filter", "content_filter"),
            ("refusal", "explicit_refusal"),
        ] {
            let body = json!({"choices":[{"finish_reason":reason,"message":{"role":"assistant","content":"Partial finding","tool_calls":[{"id":"a","function":{"name":"write_file","arguments":"{\"path\":"}}]}}]});
            let (base, worker) =
                crate::test_support::http(vec![("application/json", body.to_string())]);
            let provider = Provider::new(&base, "fixture".into(), None).unwrap();
            let error = provider
                .complete(&[json!({"role":"user","content":"bounded test"})], tools())
                .unwrap_err();
            let failure = error
                .downcast_ref::<crate::provider_contract::Incomplete>()
                .unwrap();
            assert_eq!(failure.kind, expected);
            assert_eq!(failure.partial_text, "Partial finding");
            assert_eq!(worker.join().unwrap().len(), 1);
        }
    }
    #[test]
    fn anthropic_wire_stops_reject_complete_tool_batches_in_both_transports() {
        for stream in [false, true] {
            for reason in [
                "model_context_window_exceeded",
                "pause_turn",
                "refusal",
                "max_tokens",
                "content_filter",
                "future_reason",
            ] {
                let blocks = json!([{"type":"text","text":"Partial finding"},{"type":"tool_use","id":"a","name":"write_file","input":{"path":"a.txt","content":"never execute"}}]);
                let body = if stream {
                    let events = [
                        json!({"type":"content_block_start","index":0,"content_block":blocks[0]}),
                        json!({"type":"content_block_start","index":1,"content_block":blocks[1]}),
                        json!({"type":"message_delta","delta":{"stop_reason":reason}}),
                        json!({"type":"message_stop"}),
                    ];
                    events
                        .iter()
                        .map(|event| format!("data: {event}\n\n"))
                        .collect::<String>()
                } else {
                    json!({"content":blocks,"stop_reason":reason}).to_string()
                };
                let (base, worker) = crate::test_support::http(vec![(
                    if stream {
                        "text/event-stream"
                    } else {
                        "application/json"
                    },
                    body,
                )]);
                let mut provider =
                    Provider::new(&base, "fixture".into(), Some("synthetic-key".into())).unwrap();
                // Internal test wiring only: production endpoint/auth selection is unchanged.
                provider.anthropic = true;
                provider.url = format!("{base}/messages");
                let error = provider
                    .request(
                        &[json!({"role":"user","content":"bounded fixture"})],
                        tools(),
                        stream,
                        |_| {},
                        || false,
                    )
                    .unwrap_err();
                let failure = error
                    .downcast_ref::<crate::provider_contract::Incomplete>()
                    .unwrap();
                assert_eq!(
                    Some(failure.kind),
                    crate::provider_contract::stop_kind(reason)
                );
                assert_eq!(failure.partial_text, "Partial finding");
                let packets = worker.join().unwrap();
                assert_eq!(packets.len(), 1);
                assert_eq!(packets[0]["tools"][0]["input_schema"]["type"], "object");
            }
        }
    }
    #[test]
    fn http_diagnostics_preserve_reason_id_and_source_without_credential_or_prompt() {
        let private = "PRIVATE_SESSION_TEXT_123456";
        let body = json!({"error":{"code":"invalid_tool_schema","message":format!("Malformed tool: {private}; credential actual-key" )}});
        let (base, worker) = crate::test_support::http_status(vec![(
            400,
            "application/json\r\nRetry-After: 7\r\nx-request-id: fixture-req",
            body.to_string(),
        )]);
        let provider = Provider::new(&base, "fixture".into(), Some("actual-key".into())).unwrap();
        let error = provider
            .complete(&[json!({"role":"user","content":private})], tools())
            .unwrap_err();
        let failure = error
            .downcast_ref::<crate::rotation::ProviderFailure>()
            .unwrap();
        assert_eq!(failure.status, 400);
        assert_eq!(failure.code.as_deref(), Some("invalid_tool_schema"));
        assert_eq!(failure.retry_after, 7);
        assert_eq!(failure.retry_after_source, "provider_header");
        assert_eq!(failure.request_id.as_deref(), Some("fixture-req"));
        assert!(!failure.model_specific);
        let rendered = error.to_string();
        assert!(rendered.contains("Malformed tool"));
        assert!(!rendered.contains(private));
        assert!(!rendered.contains("actual-key"));
        assert!(!rendered.contains("shared"));
        assert_eq!(worker.join().unwrap().len(), 1);
    }
    #[test]
    fn shared_cooldown_blocks_a_new_provider_instance_and_model() {
        let home = tempfile::tempdir().unwrap();
        let (base, worker) = crate::test_support::http_status(vec![(
            429,
            "application/json\r\nRetry-After: 120",
            json!({"error":{"code":"account_rate_limit","message":"Account limit"}}).to_string(),
        )]);
        let first = Provider::new(&base, "first".into(), None)
            .unwrap()
            .coordinated(home.path())
            .unwrap();
        let messages = [json!({"role":"user","content":"Fixture"})];
        assert!(first.complete(&messages, tools()).is_err());
        assert_eq!(worker.join().unwrap().len(), 1);
        let second = Provider::new(&base, "another".into(), None)
            .unwrap()
            .coordinated(home.path())
            .unwrap();
        assert!(second
            .complete(&messages, tools())
            .unwrap_err()
            .to_string()
            .contains("cooldown"));
    }
    #[test]
    fn preflight_failure_does_not_reach_listener_or_rewrite_history() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let provider = Provider::new(
            &format!("http://{}", listener.local_addr().unwrap()),
            "fixture".into(),
            None,
        )
        .unwrap();
        let history = vec![
            json!({"role":"tool","tool_call_id":"orphan","content":"Keep exact private evidence"}),
        ];
        let original = history.clone();
        assert!(provider
            .complete(&history, tools())
            .unwrap_err()
            .to_string()
            .contains("Unmatched"));
        assert_eq!(history, original);
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[cfg(test)]
mod telemetry_tests {
    use super::*;
    #[test]
    fn trailing_stream_usage_is_recorded_without_polluting_history() {
        let body="data: {\"model\":\"actual-model\",\"choices\":[{\"delta\":{\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":17,\"completion_tokens\":3,\"private_field\":\"DO_NOT_KEEP\"}}\n\ndata: [DONE]\n\n";
        let (base, worker) = crate::test_support::http(vec![("text/event-stream", body.into())]);
        let provider = Provider::new(&base, "requested-model".into(), None).unwrap();
        let result = provider
            .stream(
                &[json!({"role":"user","content":"Fixture"})],
                json!([]),
                |_| {},
                || false,
            )
            .unwrap();
        assert_eq!(result["content"], "done");
        assert!(result.get("_blue_provider_metadata").is_none());
        let metrics = provider.metrics();
        assert_eq!(metrics["actual_model"], "actual-model");
        assert_eq!(metrics["usage"]["prompt_tokens"], 17);
        assert_eq!(metrics["outcome"], "completed");
        assert_eq!(metrics["provider_instance_requests_sent"], 1);
        assert!(!metrics.to_string().contains("DO_NOT_KEEP"));
        assert_eq!(worker.join().unwrap().len(), 1);
    }
}
