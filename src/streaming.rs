//! Bounded SSE decoding for OpenAI-compatible and Anthropic tool streams.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read},
};
const MAX_STREAM: u64 = 4_000_000;

#[derive(Default)]
struct Assembly {
    text: String,
    calls: BTreeMap<usize, Value>,
    arguments: BTreeMap<usize, String>,
    finished: bool,
    stream_end: bool,
    metadata: Value,
    saw_reasoning: bool,
    saw_content_parts: bool,
    saw_message_object: bool,
    saw_choice: bool,
    finish_reason: Option<&'static str>,
}

impl Assembly {
    fn event(&mut self, data: &str, anthropic: bool, delta: &mut impl FnMut(&str)) -> Result<()> {
        if data == "[DONE]" {
            self.finished = true;
            self.stream_end = true;
            return Ok(());
        }
        let v: Value = serde_json::from_str(data).context("Invalid SSE JSON")?;
        if !self.metadata.is_object() {
            self.metadata = json!({"usage":{}});
        }
        if let Some(model) = v["model"]
            .as_str()
            .or_else(|| v["message"]["model"].as_str())
        {
            self.metadata["model"] = json!(model);
        }
        let usage = if v["usage"].is_object() {
            &v["usage"]
        } else {
            &v["message"]["usage"]
        };
        for key in [
            "input_tokens",
            "output_tokens",
            "prompt_tokens",
            "completion_tokens",
            "total_tokens",
        ] {
            if let Some(number) = usage[key].as_u64() {
                self.metadata["usage"][key] = json!(number);
            }
        }
        if v.get("error").is_some() || v["type"] == "error" {
            bail!("Provider stream reported an error (payload suppressed)");
        }
        if anthropic {
            match v["type"].as_str() {
                Some("message_stop") => {
                    self.finished = true;
                    self.stream_end = true;
                }
                Some("message_delta") => {
                    if let Some(kind) = v["delta"]["stop_reason"]
                        .as_str()
                        .and_then(crate::provider_contract::stop_kind)
                    {
                        return Err(crate::provider_contract::incomplete_reason(
                            kind,
                            &self.text,
                            v["delta"]["stop_reason"].as_str().unwrap_or(""),
                        ));
                    }
                }
                Some("content_block_start") => {
                    let index = v["index"].as_u64().context("Missing block index")? as usize;
                    if index > 63 {
                        bail!("Too many streamed content blocks");
                    }
                    if v["content_block"]["type"] == "tool_use" {
                        self.calls.insert(index, v["content_block"].clone());
                    } else if let Some(text) = v["content_block"]["text"].as_str() {
                        self.text.push_str(text);
                        delta(text);
                    }
                }
                Some("content_block_delta") => {
                    let index = v["index"].as_u64().context("Missing delta index")? as usize;
                    if index > 63 {
                        bail!("Too many streamed content blocks");
                    }
                    if let Some(text) = v["delta"]["text"].as_str() {
                        self.text.push_str(text);
                        delta(text);
                    }
                    if let Some(text) = v["delta"]["partial_json"].as_str() {
                        self.arguments.entry(index).or_default().push_str(text);
                    }
                }
                _ => {}
            }
        } else {
            let choice = &v["choices"][0];
            self.saw_choice |= choice.is_object();
            self.saw_content_parts |= choice["delta"]["content"].is_array();
            self.saw_message_object |= choice["message"].is_object();
            self.saw_reasoning |= ["reasoning", "reasoning_content", "reasoning_details"]
                .iter()
                .any(|key| {
                    let field = &choice["delta"][*key];
                    field.as_str().is_some_and(|s| !s.is_empty())
                        || field.as_array().is_some_and(|items| !items.is_empty())
                        || field.is_object()
                });
            if choice["delta"]["refusal"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
            {
                return Err(crate::provider_contract::incomplete(
                    "explicit_refusal",
                    &self.text,
                ));
            }
            if let Some(text) = choice["delta"]["content"].as_str() {
                self.text.push_str(text);
                delta(text);
            }
            if let Some(calls) = choice["delta"]["tool_calls"].as_array() {
                for call in calls {
                    let index = call["index"].as_u64().context("Missing tool call index")? as usize;
                    if index > 63 {
                        bail!("Too many streamed tool calls");
                    }
                    let saved = self.calls.entry(index).or_insert_with(
                        || json!({"id":"","type":"function","function":{"name":"","arguments":""}}),
                    );
                    for (pointer, value) in [
                        ("/id", &call["id"]),
                        ("/function/name", &call["function"]["name"]),
                        ("/function/arguments", &call["function"]["arguments"]),
                    ] {
                        if let Some(fragment) = value.as_str() {
                            let slot = saved
                                .pointer_mut(pointer)
                                .context("Invalid call accumulator")?;
                            let mut content = slot.as_str().unwrap_or_default().to_owned();
                            content.push_str(fragment);
                            *slot = json!(content);
                        }
                    }
                }
            }
            if let Some(kind) = choice["finish_reason"]
                .as_str()
                .and_then(crate::provider_contract::stop_kind)
            {
                return Err(crate::provider_contract::incomplete_reason(
                    kind,
                    &self.text,
                    choice["finish_reason"].as_str().unwrap_or(""),
                ));
            }
            if let Some(reason) = choice["finish_reason"].as_str() {
                self.finish_reason = Some(match reason {
                    "stop" => "stop",
                    "tool_calls" => "tool_calls",
                    "function_call" => "function_call",
                    _ => "other",
                });
                self.finished = true;
            }
        }
        Ok(())
    }
    fn finish(mut self, anthropic: bool) -> Result<Value> {
        if !self.finished {
            bail!("Provider stream ended before its completion marker; no tool was executed");
        }
        if !anthropic {
            self.metadata["stream_shape"] = json!({
                "saw_choice": self.saw_choice,
                "saw_reasoning": self.saw_reasoning,
                "saw_content_parts": self.saw_content_parts,
                "saw_message_object": self.saw_message_object,
                "visible_characters": self.text.chars().count(),
                "native_calls": self.calls.len(),
                "finish_reason": self.finish_reason,
                "saw_done": self.stream_end
            });
        }
        let mut calls = Vec::new();
        for (index, call) in self.calls {
            let parsed = if anthropic {
                let input = match self.arguments.get(&index) {
                    Some(text) => serde_json::from_str::<Value>(text)
                        .context("Incomplete streamed tool JSON")?,
                    None => call["input"].clone(),
                };
                json!({"id":call["id"],"type":"function","function":{"name":call["name"],"arguments":serde_json::to_string(&input)?}})
            } else {
                call
            };
            if parsed["id"].as_str().unwrap_or_default().is_empty()
                || parsed["function"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .is_empty()
            {
                bail!("Incomplete streamed tool identity");
            }
            if calls
                .iter()
                .any(|existing: &Value| existing["id"] == parsed["id"])
            {
                bail!("Duplicate streamed tool ID");
            }
            calls.push(parsed);
        }
        Ok(
            json!({"role":"assistant","content":self.text,"tool_calls":calls,"_blue_provider_metadata":self.metadata}),
        )
    }
}

pub fn decode(
    reader: impl Read,
    anthropic: bool,
    mut delta: impl FnMut(&str),
    mut cancelled: impl FnMut() -> bool,
) -> Result<Value> {
    let mut input = BufReader::new(reader.take(MAX_STREAM + 1));
    let mut consumed = 0u64;
    let mut line = Vec::new();
    let mut data = String::new();
    let mut assembly = Assembly::default();
    loop {
        if cancelled() {
            bail!("Task cancelled");
        }
        line.clear();
        let count = input.read_until(b'\n', &mut line).map_err(|_| {
            crate::provider_contract::incomplete("connection_interrupted", &assembly.text)
        })?;
        consumed += count as u64;
        if consumed > MAX_STREAM {
            bail!("Provider stream exceeded 4 MB");
        }
        if count == 0 {
            break;
        }
        let text = std::str::from_utf8(&line)
            .context("Invalid stream UTF-8")?
            .trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            if !data.is_empty() {
                assembly
                    .event(&data, anthropic, &mut delta)
                    .map_err(|error| {
                        if error
                            .downcast_ref::<crate::provider_contract::Incomplete>()
                            .is_some()
                        {
                            error
                        } else {
                            crate::provider_contract::incomplete("malformed_stream", &assembly.text)
                        }
                    })?;
                data.clear();
                if assembly.stream_end {
                    break;
                }
            }
        } else if let Some(fragment) = text.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(fragment.strip_prefix(' ').unwrap_or(fragment));
        }
    }
    if !data.is_empty() {
        assembly
            .event(&data, anthropic, &mut delta)
            .map_err(|error| {
                if error
                    .downcast_ref::<crate::provider_contract::Incomplete>()
                    .is_some()
                {
                    error
                } else {
                    crate::provider_contract::incomplete("malformed_stream", &assembly.text)
                }
            })?;
    }
    if cancelled() {
        bail!("Task cancelled");
    }
    if !assembly.finished {
        return Err(crate::provider_contract::incomplete(
            "connection_interrupted",
            &assembly.text,
        ));
    }
    let partial = crate::reasoning::context::clip(&assembly.text, 16000);
    assembly
        .finish(anthropic)
        .map_err(|_| crate::provider_contract::incomplete("malformed_tool_call", &partial))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn joins_fragmented_openai_tools_and_unicode() {
        let chunks = [
            json!({"choices":[{"delta":{"content":"مرحبا","tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_","arguments":"{\"path\":"}}]}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"file","arguments":"\"a.rs\"}"}}]},"finish_reason":"tool_calls"}]}),
        ];
        let stream = chunks
            .iter()
            .map(|v| format!("data: {v}\n\n"))
            .collect::<String>();
        let mut seen = String::new();
        let result = decode(stream.as_bytes(), false, |s| seen.push_str(s), || false).unwrap();
        assert_eq!(seen, "مرحبا");
        assert_eq!(result["tool_calls"][0]["function"]["name"], "read_file");
        assert_eq!(
            result["tool_calls"][0]["function"]["arguments"],
            "{\"path\":\"a.rs\"}"
        );
    }
    #[test]
    fn anthropic_assembles_native_tool_input() {
        let values = [
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"u1","name":"read_file","input":{}}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"x\"}"}}),
            json!({"type":"message_stop"}),
        ];
        let stream = values
            .iter()
            .map(|v| format!("data: {v}\n\n"))
            .collect::<String>();
        assert_eq!(
            decode(stream.as_bytes(), true, |_| {}, || false).unwrap()["tool_calls"][0]["function"]
                ["arguments"],
            "{\"path\":\"x\"}"
        );
    }
    #[test]
    fn never_accepts_truncated_stream() {
        assert!(decode(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n".as_slice(),
            false,
            |_| {},
            || false
        )
        .is_err());
        assert!(decode(b"data: [DONE]\n\n".as_slice(), false, |_| {}, || true).is_err());
    }
    #[test]
    fn stream_limit_filter_and_disconnect_have_distinct_preserved_partials() {
        for (reason, kind) in [
            (Some("length"), "response_limit"),
            (Some("content_filter"), "content_filter"),
            (None, "connection_interrupted"),
        ] {
            let event = json!({"choices":[{"delta":{"content":"partial UTF-8 🐉"},"finish_reason":reason}]});
            let wire = format!("data: {event}\n\n");
            let error = decode(wire.as_bytes(), false, |_| {}, || false).unwrap_err();
            let failure = error
                .downcast_ref::<crate::provider_contract::Incomplete>()
                .unwrap();
            assert_eq!(failure.kind, kind);
            assert_eq!(failure.partial_text, "partial UTF-8 🐉");
        }
    }
    #[test]
    fn token_limit_does_not_report_completion() {
        let event = json!({"choices":[{"delta":{"content":"partial"},"finish_reason":"length"}]});
        let stream = format!("data: {event}\n\n");
        assert!(decode(stream.as_bytes(), false, |_| {}, || false).is_err());
    }
    #[test]
    fn command_fragments_preserve_raw_types_without_duplication() {
        let original =
            json!({"program":"fixture","args":"[\"a b\",\"مرحبا\"]","timeout_seconds":"30"})
                .to_string();
        let mut stream = String::new();
        for (index, chunk) in original.chars().collect::<Vec<_>>().chunks(7).enumerate() {
            let mut call =
                json!({"index":0,"function":{"arguments":chunk.iter().collect::<String>()}});
            if index == 0 {
                call["id"] = json!("c");
                call["function"]["name"] = json!("run_command");
            }
            stream.push_str(&format!(
                "data: {}\n\n",
                json!({"choices":[{"delta":{"tool_calls":[call]}}]})
            ));
        }
        stream
            .push_str("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n");
        let result = decode(stream.as_bytes(), false, |_| {}, || false).unwrap();
        assert_eq!(result["tool_calls"][0]["function"]["arguments"], original);
        let raw: Value = serde_json::from_str(&original).unwrap();
        assert!(raw["args"].is_string());
        assert!(raw["timeout_seconds"].is_string());
        let ready =
            crate::tool_arguments::prepare("run_command", &raw, &crate::tools::definitions())
                .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ready.arguments["args"], json!(["a b", "مرحبا"]));
        assert_eq!(ready.changes.len(), 2);
    }
}

#[cfg(test)]
mod stream_shape_tests {
    use super::*;

    #[test]
    fn distinguishes_empty_response_shapes_without_recording_payloads() {
        for (name, choice) in [
            (
                "saw_reasoning",
                json!({"delta":{"reasoning_content":"private"}}),
            ),
            (
                "saw_content_parts",
                json!({"delta":{"content":[{"text":"private"}]}}),
            ),
            (
                "saw_message_object",
                json!({"message":{"content":"private"}}),
            ),
            ("empty_delta", json!({"delta":{}})),
            ("visible", json!({"delta":{"content":"ok"}})),
        ] {
            let event = json!({"choices":[choice]});
            let stream = format!("data: {event}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: {{\"choices\":[],\"usage\":{{\"completion_tokens\":133}}}}\n\ndata: [DONE]\n\n");
            let msg = decode(stream.as_bytes(), false, |_| {}, || false).unwrap();
            let metadata = &msg["_blue_provider_metadata"];
            let shape = &metadata["stream_shape"];
            assert_eq!(metadata["usage"]["completion_tokens"], 133);
            assert_eq!(shape["finish_reason"], "stop");
            assert_eq!(shape["saw_done"], true);
            assert!(!metadata.to_string().contains("private"));
            if name.starts_with("saw_") {
                assert_eq!(shape[name], true);
            }
            let result = crate::provider_contract::validate_completion(&msg, &json!([]));
            if name == "visible" {
                result.unwrap();
                assert_eq!(shape["visible_characters"], 2);
            } else {
                assert_eq!(
                    result
                        .unwrap_err()
                        .downcast_ref::<crate::provider_contract::Incomplete>()
                        .unwrap()
                        .kind,
                    "empty_response"
                );
            }
        }
    }
}
