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
}

impl Assembly {
    fn event(&mut self, data: &str, anthropic: bool, delta: &mut impl FnMut(&str)) -> Result<()> {
        if data == "[DONE]" {
            self.finished = true;
            return Ok(());
        }
        let v: Value = serde_json::from_str(data).context("Invalid SSE JSON")?;
        if v.get("error").is_some() || v["type"] == "error" {
            bail!("Provider stream reported an error (payload suppressed)");
        }
        if anthropic {
            match v["type"].as_str() {
                Some("message_stop") => self.finished = true,
                Some("message_delta")
                    if matches!(
                        v["delta"]["stop_reason"].as_str(),
                        Some("max_tokens" | "refusal")
                    ) =>
                {
                    bail!("Provider stopped before a complete response (token limit or refusal)");
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
            if matches!(
                choice["finish_reason"].as_str(),
                Some("length" | "content_filter")
            ) {
                bail!("Provider stopped before a complete response (token limit or filter)");
            }
            if choice["finish_reason"].is_string() {
                self.finished = true;
            }
        }
        Ok(())
    }
    fn finish(self, anthropic: bool) -> Result<Value> {
        if !self.finished {
            bail!("Provider stream ended before its completion marker; no tool was executed");
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
        Ok(json!({"role":"assistant","content":self.text,"tool_calls":calls}))
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
        let count = input.read_until(b'\n', &mut line)?;
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
                assembly.event(&data, anthropic, &mut delta)?;
                data.clear();
                if assembly.finished {
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
        assembly.event(&data, anthropic, &mut delta)?;
    }
    if cancelled() {
        bail!("Task cancelled");
    }
    assembly.finish(anthropic)
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
    fn token_limit_does_not_report_completion() {
        let event = json!({"choices":[{"delta":{"content":"partial"},"finish_reason":"length"}]});
        let stream = format!("data: {event}\n\n");
        assert!(decode(stream.as_bytes(), false, |_| {}, || false).is_err());
    }
}
