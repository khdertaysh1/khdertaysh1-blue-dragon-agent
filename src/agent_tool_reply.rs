//! Normalize complete provider replies at the boundary before history or tool dispatch.
use serde_json::{json, Value};
use std::path::Path;

pub(crate) fn parse(text: &str) -> Option<Vec<Value>> {
    let text = text.trim();
    let text = if let Some(body) = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
    {
        body.strip_suffix("```")?.trim()
    } else {
        text
    };
    if let Some(body) = text.strip_prefix("to=functions.") {
        let end = body.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
        let (name, rest) = body.split_at(end);
        let rest = rest.trim_start();
        let rest = rest.strip_prefix("commentary").unwrap_or(rest).trim_start();
        let rest = rest.strip_prefix("code").unwrap_or(rest).trim_start();
        let args: Value = serde_json::from_str(rest).ok()?;
        return (!name.is_empty() && args.is_object())
            .then(|| vec![json!({"tool":name,"args":args})]);
    }
    if !text.starts_with('{') {
        return None;
    }
    let mut batch = Vec::new();
    for item in serde_json::Deserializer::from_str(text).into_iter::<Value>() {
        let value = item.ok()?;
        if batch.len() == 8
            || value.as_object()?.len() != 2
            || !value["tool"].is_string()
            || !value["args"].is_object()
        {
            return None;
        }
        batch.push(value);
    }
    (!batch.is_empty()).then_some(batch)
}

pub(crate) fn normalize(
    batch: &[Value],
    schemas: &Value,
    root: &Path,
    index: usize,
) -> Result<Value, String> {
    let mut calls = Vec::new();
    for (number, envelope) in batch.iter().enumerate() {
        let name = envelope["tool"].as_str().unwrap_or("");
        if !matches!(
            name,
            "list_files"
                | "file_info"
                | "read_file"
                | "read_file_window"
                | "read_lines"
                | "search_file"
                | "search_files"
                | "file_outline"
                | "code_search"
                | "edit_file"
                | "multi_edit_file"
                | "write_file"
                | "run_command"
        ) {
            return Err("Use the native tool protocol for this action.".into());
        }
        let mut args = envelope["args"].clone();
        if let Some(path) = args["path"]
            .as_str()
            .map(Path::new)
            .filter(|p| p.is_absolute())
        {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "Use a workspace-relative path within the active project.")?;
            args["path"] = json!(if relative.as_os_str().is_empty() {
                "."
            } else {
                relative.to_str().ok_or("Path must be UTF-8")?
            });
        }
        if name == "search_files" && (args.get("pattern").is_some() || args.get("root").is_some()) {
            return Err("search_files searches contents and takes only query. To locate a file by name, use list_files with a workspace-relative path, then inspect the returned filenames.".into());
        }
        crate::tool_arguments::prepare(name, &args, schemas).map_err(|e| {
            e["error"]
                .as_str()
                .unwrap_or("Use the advertised schema.")
                .to_owned()
        })?;
        calls.push(json!({"id":format!("local-format-{index}-{number}"),"type":"function","function":{"name":name,"arguments":args.to_string()}}));
    }
    Ok(json!({"role":"assistant","content":null,"tool_calls":calls}))
}
