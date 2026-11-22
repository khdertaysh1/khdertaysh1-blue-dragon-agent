//! Conservative wire-contract validation; never repairs history by dropping data.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn calls(message: &Value) -> Result<Vec<&Value>> {
    match message.get("tool_calls") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(calls)) => Ok(calls.iter().collect()),
        _ => bail!("Malformed tool-call collection"),
    }
}
fn identity(call: &Value) -> Result<&str> {
    let id = call["id"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .context("Missing or oversized tool-call identity")?;
    let name = call["function"]["name"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_whitespace))
        .context("Malformed tool name")?;
    let _ = name;
    let args = call["function"]["arguments"]
        .as_str()
        .context("Tool arguments must be serialized JSON")?;
    if args.len() > 256 * 1024 || !serde_json::from_str::<Value>(args).is_ok_and(|v| v.is_object())
    {
        bail!("Tool arguments are incomplete or not a JSON object; no tool executed");
    }
    Ok(id)
}
pub fn preflight(messages: &[Value], tools: &Value, model: &str) -> Result<()> {
    if model.trim().is_empty() {
        bail!("Model identifier is empty");
    }
    let definitions = tools
        .as_array()
        .context("Tool definitions must be an array")?;
    let mut names = BTreeSet::new();
    for definition in definitions {
        let name = definition["function"]["name"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_whitespace))
            .context("Invalid tool definition name")?;
        if definition["type"] != "function"
            || !names.insert(name)
            || !definition["function"]["parameters"].is_object()
        {
            bail!("Invalid or duplicate tool definition");
        }
    }
    let mut seen = BTreeSet::new();
    let mut pending = BTreeSet::new();
    for message in messages {
        let role = message["role"].as_str().context("Message role missing")?;
        if role == "tool" {
            let id = message["tool_call_id"]
                .as_str()
                .context("Tool result identity missing")?;
            if !pending.remove(id) || !message["content"].is_string() {
                bail!("Unmatched, duplicate or malformed tool result");
            }
        } else {
            if !pending.is_empty() {
                bail!("Tool-call results are missing before the next message; original history preserved");
            }
            if !matches!(role, "system" | "user" | "assistant") {
                bail!("Unsupported message role");
            }
            if !(message["content"].is_string()
                || message["content"].is_array()
                || (role == "assistant" && message["content"].is_null()))
            {
                bail!("Unsupported message content shape");
            }
            for call in calls(message)? {
                if role != "assistant" {
                    bail!("Only assistant messages may contain tool calls");
                }
                let id = identity(call)?;
                if !seen.insert(id) {
                    bail!("Duplicate tool-call identity in request history");
                }
                pending.insert(id);
            }
        }
    }
    if !pending.is_empty() {
        bail!("Request contains tool calls without results; nothing sent");
    }
    Ok(())
}

#[derive(Debug)]
pub struct Incomplete {
    pub kind: &'static str,
    pub partial_text: String,
    pub raw_stop_reason: Option<String>,
}
impl std::fmt::Display for Incomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f,"Provider completion incomplete: {}; partial text retained privately, no tool batch executed",self.kind)
    }
}
impl std::error::Error for Incomplete {}
pub fn incomplete(kind: &'static str, text: &str) -> anyhow::Error {
    Incomplete {
        kind,
        raw_stop_reason: None,
        partial_text: crate::reasoning::context::clip(text, 16000),
    }
    .into()
}
pub fn incomplete_reason(kind: &'static str, text: &str, reason: &str) -> anyhow::Error {
    Incomplete {
        kind,
        partial_text: crate::reasoning::context::clip(text, 16000),
        raw_stop_reason: Some(
            if reason.len() <= 64 && reason.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                reason.to_owned()
            } else {
                "unrecognized_reason_redacted".into()
            },
        ),
    }
    .into()
}
pub fn stop_kind(reason: &str) -> Option<&'static str> {
    match reason {
        "length" | "max_tokens" => Some("response_limit"),
        "content_filter" => Some("content_filter"),
        "refusal" => Some("explicit_refusal"),
        "model_context_window_exceeded" => Some("context_limit"),
        "pause_turn" => Some("provider_pause"),
        "stop" | "tool_calls" | "function_call" | "end_turn" | "tool_use" | "stop_sequence" => None,
        _ => Some("unknown_stop_reason"),
    }
}
pub fn text(message: &Value) -> String {
    match &message["content"] {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
pub fn validate_completion(message: &Value, _tools: &Value) -> Result<()> {
    let text = text(message);
    if message["refusal"].as_str().is_some_and(|s| !s.is_empty()) {
        return Err(incomplete("explicit_refusal", &text));
    }
    let batch = calls(message).map_err(|_| incomplete("malformed_tool_call", &text))?;
    if batch.len() > 8 {
        return Err(incomplete("tool_batch_limit", &text));
    }
    let mut seen = BTreeSet::new();
    for call in &batch {
        let valid = identity(call).is_ok_and(|id| seen.insert(id));
        if !valid {
            return Err(incomplete("malformed_tool_call", &text));
        }
    }
    if batch.is_empty() && text.trim().is_empty() {
        return Err(incomplete("empty_response", &text));
    }
    Ok(())
}
pub fn partial_record(error: &Incomplete) -> Value {
    json!({"raw_stop_reason":error.raw_stop_reason,"classification_source":"client_stop_reason_mapping","kind":error.kind,"partial_text_retained_in_session":true,"partial_characters":error.partial_text.chars().count(),"incomplete":true,"tool_calls_executed":0,"trusted_evidence":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tool() -> Value {
        json!([{"type":"function","function":{"name":"read","parameters":{"type":"object"}}}])
    }
    fn call() -> Value {
        json!({"role":"assistant","content":null,"tool_calls":[{"id":"a","function":{"name":"read","arguments":"{}"}}]})
    }
    #[test]
    fn paired_history_is_unchanged_and_invalid_history_rejected() {
        let history = vec![
            json!({"role":"user","content":"Keep my constraints"}),
            call(),
            json!({"role":"tool","tool_call_id":"a","content":"private result"}),
        ];
        let before = history.clone();
        preflight(&history, &tool(), "model").unwrap();
        assert_eq!(history, before);
        assert!(preflight(&history[..2], &tool(), "model").is_err());
        let mut duplicate = history.clone();
        duplicate.extend(history[1..].iter().cloned());
        assert!(preflight(&duplicate, &tool(), "model").is_err());
        let mut wrong = history.clone();
        wrong[2]["tool_call_id"] = json!("wrong");
        assert!(preflight(&wrong, &tool(), "model").is_err());
        let mut malformed = history;
        malformed[1]["tool_calls"][0]["function"]["arguments"] = json!("{\"path\":");
        assert!(preflight(&malformed, &tool(), "model").is_err());
    }
    #[test]
    fn whole_batch_rejected_on_bad_arguments_or_identity() {
        for args in ["{", "null", "[]", "1"] {
            let mut m = call();
            m["tool_calls"][0]["function"]["arguments"] = json!(args);
            assert!(validate_completion(&m, &tool()).is_err());
        }
        let mut m = call();
        let c = m["tool_calls"][0].clone();
        m["tool_calls"].as_array_mut().unwrap().push(c);
        assert!(validate_completion(&m, &tool()).is_err());
        validate_completion(&call(), &tool()).unwrap();
        validate_completion(
            &json!({"content":[{"type":"text","text":"Unicode 🐉"}]}),
            &tool(),
        )
        .unwrap();
        assert!(validate_completion(&call(), &json!([])).is_ok()); // Dispatch still denies unavailable tools.
        assert!(validate_completion(&json!({"content":""}), &tool()).is_err());
    }
}
