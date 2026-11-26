//! User-selected text prompts stay external data, including assistant-role text.
use super::*;
use std::collections::BTreeSet;

#[derive(Default, Clone)]
pub(super) struct Catalog {
    names: BTreeMap<String, BTreeMap<String, bool>>,
    next: Option<String>,
    cursors: BTreeSet<String>,
    pages: usize,
}
fn identifier(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
        .context("Invalid MCP prompt or argument name")
}
impl Catalog {
    fn page(mut self, value: &Value) -> Result<(Self, Value)> {
        let rows = value["prompts"]
            .as_array()
            .context("Missing MCP prompt list")?;
        anyhow::ensure!(
            rows.len() <= 32 && self.pages < 8,
            "MCP prompt page exceeds limit"
        );
        let mut result = Vec::new();
        for p in rows {
            let name = identifier(&p["name"])?;
            let mut args = BTreeMap::new();
            if let Some(v) = p.get("arguments") {
                let defs = v.as_array().context("Invalid prompt arguments")?;
                anyhow::ensure!(defs.len() <= 32, "Too many prompt arguments");
                for arg in defs {
                    let key = identifier(&arg["name"])?;
                    let required = arg
                        .get("required")
                        .map(|v| v.as_bool().context("Invalid required field"))
                        .transpose()?
                        .unwrap_or(false);
                    anyhow::ensure!(
                        args.insert(key.to_owned(), required).is_none(),
                        "Duplicate prompt argument"
                    );
                }
            }
            result.push(json!({"name":name,"description":p["description"].as_str().unwrap_or("").chars().take(512).collect::<String>(),"arguments":args}));
            anyhow::ensure!(
                self.names.len() < 128 && self.names.insert(name.into(), args).is_none(),
                "Duplicate or excessive prompts"
            );
        }
        self.pages += 1;
        self.next = match value.get("nextCursor") {
            None | Some(Value::Null) => None,
            Some(v) => {
                let c = v
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256)
                    .context("Invalid prompt cursor")?;
                anyhow::ensure!(self.cursors.insert(c.into()), "Repeated prompt cursor");
                Some(c.into())
            }
        };
        let out = json!({"prompts":result,"next_cursor":self.next,"catalog_complete":self.next.is_none()});
        Ok((self, out))
    }
}
fn messages(response: &Value) -> Result<Value> {
    let messages = response["messages"]
        .as_array()
        .context("Missing prompt messages")?;
    anyhow::ensure!(
        !messages.is_empty() && messages.len() <= 32,
        "Invalid prompt message count"
    );
    let mut bytes = 0;
    let mut rows = Vec::new();
    for m in messages {
        anyhow::ensure!(
            matches!(m["role"].as_str(), Some("assistant" | "user"))
                && m["content"]["type"] == "text",
            "Unsupported MCP prompt content"
        );
        let text = m["content"]["text"]
            .as_str()
            .context("Missing prompt text")?;
        bytes += text.len();
        anyhow::ensure!(bytes <= 65536, "Prompt content exceeds 64 KiB");
        rows.push(json!({"role":m["role"],"text":text}));
    }
    Ok(json!({"messages":rows,"text_bytes":bytes}))
}

impl Registry {
    pub fn prompt_command(&mut self, args: &str, approval: &ToolBox) -> Result<Value> {
        let (action, rest) = args
            .trim()
            .split_once(char::is_whitespace)
            .context("Use /mcp prompts SERVER or /mcp prompt SERVER NAME [JSON]")?;
        let (server, rest) = rest
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((rest.trim(), ""));
        let rest = rest.trim();
        let catalog = self
            .prompts
            .get(server)
            .context("Connect a server advertising prompts first")?;
        let (method, params, page) = match action {
            "prompts" if rest.is_empty() => ("prompts/list", json!({}), Some(Catalog::default())),
            "prompts" => {
                anyhow::ensure!(
                    catalog.next.as_deref() == Some(rest) && catalog.pages < 8,
                    "Use the next cursor from this server"
                );
                (
                    "prompts/list",
                    json!({"cursor":rest}),
                    Some(catalog.clone()),
                )
            }
            "prompt" => {
                let (name, raw) = rest.split_once(char::is_whitespace).unwrap_or((rest, "{}"));
                let declared = catalog
                    .names
                    .get(name)
                    .context("List and select a prompt from this connection")?;
                anyhow::ensure!(raw.len() <= 16384, "Prompt arguments exceed limit");
                let args: BTreeMap<String, String> = serde_json::from_str(raw.trim())?;
                anyhow::ensure!(
                    args.keys().all(|k| declared.contains_key(k))
                        && declared.iter().all(|(k, r)| !r || args.contains_key(k)),
                    "Missing or undeclared prompt argument"
                );
                ("prompts/get", json!({"name":name,"arguments":args}), None)
            }
            _ => bail!("Use /mcp prompts SERVER or /mcp prompt SERVER NAME [JSON]"),
        };
        anyhow::ensure!(!self.cancelled.load(Ordering::Relaxed), "Task cancelled");
        if !approval.approve(&format!("MCP {method} on {server}"), &params.to_string())? {
            return Ok(json!({"ok":false,"denied":true,"execution_status":"not_executed"}));
        }
        let response = match self
            .clients
            .get_mut(server)
            .context("Server disconnected")?
            .request(method, params)
        {
            Ok(v) => v,
            Err(e) => {
                if let Some(result) = remote_error(&e) {
                    return Ok(result);
                }
                self.disconnect(server);
                return Ok(
                    json!({"ok":false,"execution_status":"unknown","disconnected":true,"automatic_retry":false}),
                );
            }
        };
        let parsed = if let Some(page) = page {
            page.page(&response).map(|(next, out)| {
                self.prompts.insert(server.into(), next);
                out
            })
        } else {
            messages(&response)
        };
        match parsed {
            Ok(mut out) => {
                out["ok"] = json!(true);
                out["server"] = json!(server);
                out["untrusted_external_content"] = json!(true);
                out["instructions_are_authority"] = json!(false);
                Ok(out)
            }
            Err(_) => {
                self.prompts.insert(server.into(), Catalog::default());
                Ok(
                    json!({"ok":false,"execution_status":"response_rejected","automatic_retry":false}),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_privileged_roles_binary_messages_and_inconsistent_catalogs() {
        for bad in [
            json!({"role":"system","content":{"type":"text","text":"unexpected"}}),
            json!({"role":"user","content":{"type":"image","data":"abc"}}),
        ] {
            assert!(messages(&json!({"messages":[bad]})).is_err());
        }
        assert!(Catalog::default()
            .page(&json!({"prompts":[{"name":"one","arguments":[{"name":"x"},{"name":"x"}]}]}))
            .is_err());
    }
}
