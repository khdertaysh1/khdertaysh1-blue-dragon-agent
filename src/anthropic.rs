//! Translate the agent's OpenAI-like internal message format to the native
//! Anthropic Messages API, preserving tool-call IDs and tool results.
use anyhow::{Context, Result};
use serde_json::{json, Value};

pub fn request_body(messages: &[Value], tools: &Value, model: &str) -> Result<Value> {
    let mut system = Vec::new();
    let mut history: Vec<Value> = Vec::new();

    for msg in messages {
        let role = msg["role"].as_str().context("Message missing role")?;
        match role {
            "system" => {
                if let Some(text) = msg["content"].as_str() {
                    system.push(text.to_string());
                }
            }
            "user" => {
                history.push(json!({
                    "role": "user",
                    "content": msg["content"].as_str().context("User message not text")?
                }));
            }
            "assistant" => {
                let mut blocks = Vec::new();
                if let Some(text) = msg["content"].as_str() {
                    if !text.is_empty() {
                        blocks.push(json!({"type":"text","text":text}));
                    }
                }
                if let Some(calls) = msg["tool_calls"].as_array() {
                    for call in calls {
                        let input: Value = serde_json::from_str(
                            call.pointer("/function/arguments")
                                .and_then(Value::as_str)
                                .context("Tool arguments missing")?,
                        )?;
                        blocks.push(json!({
                            "type":"tool_use",
                            "id":call["id"],
                            "name":call["function"]["name"],
                            "input":input
                        }));
                    }
                }
                if !blocks.is_empty() {
                    history.push(json!({"role":"assistant","content":blocks}));
                }
            }
            "tool" => {
                let block = json!({
                    "type":"tool_result",
                    "tool_use_id":msg["tool_call_id"],
                    "content":msg["content"].as_str().context("Tool result not text")?
                });
                if let Some(last) = history.last_mut() {
                    if last["role"] == "user" && last["content"].is_array() {
                        last["content"].as_array_mut().context("Unexpected content")?.push(block);
                        continue;
                    }
                }
                history.push(json!({"role":"user","content":[block]}));
            }
            other => anyhow::bail!("Unsupported message role: {other}"),
        }
    }

    let available = tools.as_array().context("Tool definitions must be an array")?;
    let definitions: Vec<Value> = available
        .iter()
        .map(|tool| {
            json!({
                "name":tool["function"]["name"],
                "description":tool["function"]["description"],
                "input_schema":tool["function"]["parameters"]
            })
        })
        .collect();

    let mut body = json!({
        "model":model,
        "max_tokens":4096,
        "messages":history,
        "tools":definitions
    });
    if !system.is_empty() {
        body["system"] = json!(system.join("\n\n"));
    }
    Ok(body)
}

pub fn decode_response(value: &Value) -> Result<Value> {
    let parts = value["content"].as_array().context("Anthropic response has no content blocks")?;
    let mut text = Vec::new();
    let mut calls = Vec::new();
    for part in parts {
        match part["type"].as_str() {
            Some("text") => {
                if let Some(content) = part["text"].as_str() {
                    text.push(content.to_string());
                }
            }
            Some("tool_use") => {
                let id = part["id"].as_str().context("Tool use missing ID")?;
                let name = part["name"].as_str().context("Tool use missing name")?;
                let arguments = serde_json::to_string(&part["input"])?;
                calls.push(json!({
                    "id":id,
                    "type":"function",
                    "function":{"name":name,"arguments":arguments}
                }));
            }
            _ => {}
        }
    }
    Ok(json!({
        "role":"assistant",
        "content":text.join("\n"),
        "tool_calls":calls
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_tool_roundtrip() {
        let messages = vec![
            json!({"role":"system","content":"You are helpful"}),
            json!({"role":"user","content":"Inspect the repo"}),
            json!({"role":"assistant","content":null,"tool_calls":[{
                "id":"call_1","type":"function",
                "function":{"name":"list_files","arguments":"{\"path\":\".\"}"}
            }]}),
            json!({"role":"tool","tool_call_id":"call_1","content":"{\"files\":[]}"}),
        ];
        let tools = json!([{"type":"function","function":{
            "name":"list_files","description":"List files",
            "parameters":{"type":"object","properties":{}}
        }}]);
        let request = request_body(&messages, &tools, "claude-test").unwrap();
        assert_eq!(request["messages"][1]["content"][0]["name"], "list_files");
        assert_eq!(request["messages"][2]["content"][0]["tool_use_id"], "call_1");
        assert_eq!(request["tools"][0]["input_schema"]["type"], "object");
    }

    #[test]
    fn converts_native_tool_use_to_agent_message() {
        let response = json!({"content":[
            {"type":"text","text":"Checking."},
            {"type":"tool_use","id":"toolu_1","name":"git_status","input":{}}
        ]});
        let parsed = decode_response(&response).unwrap();
        assert_eq!(parsed["content"], "Checking.");
        assert_eq!(parsed["tool_calls"][0]["function"]["name"], "git_status");
        assert_eq!(parsed["tool_calls"][0]["function"]["arguments"], "{}");
    }
}
