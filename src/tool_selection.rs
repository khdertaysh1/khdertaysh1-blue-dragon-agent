//! Schema selection is a context optimization, never a permission grant.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub fn core() -> BTreeSet<String> {
    [
        "discover_tools",
        "run_command",
        "write_file",
        "list_files",
        "read_file",
        "read_file_window",
        "search_file",
        "file_info",
        "recall_context",
        "update_plan",
        "task_continuity",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
pub fn filter(all: &Value, selected: Option<&BTreeSet<String>>) -> Value {
    json!(all
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| selected
            .is_none_or(|set| set.contains(d["function"]["name"].as_str().unwrap_or(""))))
        .cloned()
        .collect::<Vec<_>>())
}
pub fn discover(
    all: &Value,
    selected: &mut Option<BTreeSet<String>>,
    args: &Value,
) -> Result<Value> {
    let definitions = all.as_array().context("Tool registry unavailable")?;
    let offset = args
        .get("offset")
        .map(|v| v.as_u64().context("offset must be a nonnegative integer"))
        .transpose()?
        .unwrap_or(0);
    let offset = usize::try_from(offset)?;
    if offset > definitions.len() {
        bail!("Tool catalog offset outside bounds");
    }
    if let Some(names) = args.get("names") {
        let names = names
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 8)
            .context("Enable 1..8 named tools for the next phase")?;
        let mut next = core();
        for name in names {
            let name = name.as_str().context("Tool name must be text")?;
            if !definitions.iter().any(|d| d["function"]["name"] == name) {
                bail!("Unknown tool; use the paginated catalog");
            }
            next.insert(name.to_owned());
        }
        *selected = Some(next);
    }
    let end = offset.saturating_add(20).min(definitions.len());
    let rows=definitions[offset..end].iter().map(|d|json!({"name":d["function"]["name"],"description":crate::reasoning::context::clip(d["function"]["description"].as_str().unwrap_or(""),160)})).collect::<Vec<_>>();
    Ok(
        json!({"ok":true,"catalog":rows,"total":definitions.len(),"next_offset":if end<definitions.len(){Some(end)}else{None},"active_schema_names":filter(all,selected.as_ref()).as_array().unwrap().iter().map(|d|d["function"]["name"].clone()).collect::<Vec<_>>(),"permission_changed":false,"note":"names replaces optional schemas for the next phase; core discovery/recall stays available. Existing scope and human approvals still apply. No tool was executed."}),
    )
}
pub fn definition() -> Value {
    json!({"type":"function","function":{"name":"discover_tools","description":"List available tools in pages of 20, or activate 1..8 named tool schemas for the next task phase plus core file/recall/planning tools. This changes advertised schemas only, never permissions or approval. Discover browser/source/research/agent/MCP/editing tools here when absent from the request.","parameters":{"type":"object","properties":{"offset":{"type":"integer","minimum":0},"names":{"type":"array","minItems":1,"maxItems":8,"items":{"type":"string"}}},"additionalProperties":false}}})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paginated_catalog_keeps_tools_discoverable_and_selection_is_explicit() {
        let all = json!((0..45)
            .map(|i| json!({"function":{"name":format!("tool{i}"),"description":"Example"}}))
            .chain(std::iter::once(definition()))
            .collect::<Vec<_>>());
        let mut selected = Some(core());
        let page = discover(&all, &mut selected, &json!({})).unwrap();
        assert_eq!(page["catalog"].as_array().unwrap().len(), 20);
        assert_eq!(page["next_offset"], 20);
        let page = discover(
            &all,
            &mut selected,
            &json!({"offset":40,"names":["tool44"]}),
        )
        .unwrap();
        assert_eq!(page["next_offset"], Value::Null);
        assert_eq!(page["permission_changed"], false);
        assert!(filter(&all, selected.as_ref())
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["function"]["name"] == "tool44"));
        let before = selected.clone();
        assert!(discover(&all, &mut selected, &json!({"names":["unknown"]})).is_err());
        assert_eq!(selected, before);
    }
}
