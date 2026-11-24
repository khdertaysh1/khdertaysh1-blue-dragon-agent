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
/// Advertise browser capability for URL-bearing tasks without granting access.
pub fn for_prompt(prompt: &str) -> BTreeSet<String> {
    let mut selected = core();
    if prompt
        .split_whitespace()
        .any(|w| !w.contains("://") && crate::intelligence::fresh::source_path(w).is_some())
    {
        selected.insert("read_file".into());
    }
    if prompt.split_whitespace().any(|word| {
        let word = word
            .trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '<' | '>' | '\"' | '\''));
        reqwest::Url::parse(word).is_ok_and(|url| matches!(url.scheme(), "https" | "http"))
    }) {
        selected.insert("browser".into());
    }
    selected
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
/// The same discovery in an unchanged phase cannot provide new information.
pub fn discover_once(
    all: &Value,
    selected: &mut Option<BTreeSet<String>>,
    args: &Value,
    seen: &mut BTreeSet<String>,
) -> Result<Value> {
    // Validate and compute the requested phase before consulting old reads.
    // A previously visited transition is still useful when it changes the
    // currently advertised schemas (A -> B -> A -> B).
    let mut next = selected.clone();
    let result = discover(all, &mut next, args)?;
    let changed = next != *selected;
    // Equivalent name encodings and reordered names expose the same phase.
    // Paging remains keyed by its actual offset; changing phase still resets it.
    let key = json!([args["offset"].as_u64().unwrap_or(0), next]).to_string();
    if !changed && seen.contains(&key) {
        return Ok(
            json!({"ok":false,"execution_status":"not_executed","repetition_blocked":true,"error":"This catalog page/selection was already provided in the same phase. Use the advertised tool or choose different names; no permissions changed."}),
        );
    }
    if changed {
        seen.clear();
    }
    *selected = next;
    seen.insert(key);
    Ok(result)
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
        bail!("Tool catalog offset outside bounds (total {}); stop paging at next_offset=null and enable exact catalog names. No selection or permission changed", definitions.len());
    }
    let mut name_normalizations = Vec::new();
    if let Some(names) = args.get("names") {
        let names = names
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 8)
            .context("Enable 1..8 named tools for the next phase")?;
        let mut next = core();
        for name in names {
            let original = name.as_str().context("Tool name must be text")?;
            let registered = |name: &str| definitions.iter().any(|d| d["function"]["name"] == name);
            let name = if registered(original) {
                original.to_owned()
            } else {
                // Compatibility is restricted to one JSON string encoding of an
                // exact registered name. No trimming, aliases or fuzzy matching.
                let decoded = if original.len() <= 1024 {
                    serde_json::from_str::<String>(original).ok()
                } else {
                    None
                };
                let decoded = decoded.filter(|name| registered(name)).context("Unknown tool name; names must contain exact registered names without extra quotes (example: {\"names\":[\"run_command\"]}). No selection or permission changed")?;
                name_normalizations.push(json!({"original":original,"effective":decoded,"operation":"single_json_string_decode","permission_changed":false}));
                decoded
            };
            next.insert(name);
        }
        *selected = Some(next);
    }
    let end = offset.saturating_add(20).min(definitions.len());
    let rows=definitions[offset..end].iter().map(|d|json!({"name":d["function"]["name"],"description":crate::reasoning::context::clip(d["function"]["description"].as_str().unwrap_or(""),160)})).collect::<Vec<_>>();
    Ok(
        json!({"ok":true,"catalog":rows,"total":definitions.len(),"next_offset":if end<definitions.len(){Some(end)}else{None},"active_schema_names":filter(all,selected.as_ref()).as_array().unwrap().iter().map(|d|d["function"]["name"].clone()).collect::<Vec<_>>(),"permission_changed":false,"name_normalizations":name_normalizations,"note":"names replaces optional schemas for the next phase; core discovery/recall stays available. Existing scope and human approvals still apply. No tool was executed."}),
    )
}
pub fn definition() -> Value {
    json!({"type":"function","function":{"name":"discover_tools","description":"List available tools in pages of 20, or activate 1..8 named tool schemas for the next task phase plus core file/recall/planning tools. This changes advertised schemas only, never permissions or approval. Discover browser/source/research/agent/MCP/editing tools here when absent from the request.","parameters":{"type":"object","properties":{"offset":{"type":"integer","minimum":0},"names":{"type":"array","minItems":1,"maxItems":8,"items":{"type":"string"}}},"additionalProperties":false}}})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_discovery_is_blocked_but_a_changed_phase_can_restore_tools() {
        let all = json!([{"function":{"name":"browser"}},{"function":{"name":"file_outline"}}]);
        let mut selected = Some(core());
        let mut seen = BTreeSet::new();
        let args = json!({"names":["browser"]});
        assert_eq!(
            discover_once(&all, &mut selected, &args, &mut seen).unwrap()["ok"],
            true
        );
        assert_eq!(
            discover_once(&all, &mut selected, &args, &mut seen).unwrap()["repetition_blocked"],
            true
        );
        assert_eq!(
            discover_once(
                &all,
                &mut selected,
                &json!({"names":["file_outline"]}),
                &mut seen
            )
            .unwrap()["ok"],
            true
        );
        assert_eq!(
            discover_once(&all, &mut selected, &args, &mut seen).unwrap()["ok"],
            true
        );
        let outline = json!({"names":["file_outline"]});
        assert_eq!(
            discover_once(&all, &mut selected, &outline, &mut seen).unwrap()["ok"],
            true
        );
        assert!(selected.as_ref().unwrap().contains("file_outline"));
        assert!(!selected.as_ref().unwrap().contains("browser"));
        assert_eq!(
            discover_once(&all, &mut selected, &outline, &mut seen).unwrap()["repetition_blocked"],
            true
        );
        let before = selected.clone();
        assert!(discover_once(
            &all,
            &mut selected,
            &json!({"names":["unknown"]}),
            &mut seen
        )
        .is_err());
        assert_eq!(selected, before);
        assert!(!selected.unwrap().contains("read_file"));
    }
    #[test]
    fn url_task_selects_browser_without_whole_file_schema() {
        let tools = for_prompt("حلل https://example.test/app ثم تابع");
        assert!(tools.contains("browser"));
        assert!(!tools.contains("read_file"));
        assert!(!for_prompt("Explain browser history").contains("browser"));
        assert!(!for_prompt("file:///private").contains("browser"));
    }
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
    #[test]
    fn quoted_registered_names_activate_once_without_permissions() {
        let all = crate::tools::definitions();
        let mut selected = Some(core());
        let mut seen = BTreeSet::new();
        let raw = json!({"names":["\"run_command\""]});
        let result = discover_once(&all, &mut selected, &raw, &mut seen).unwrap();
        assert_eq!(result["permission_changed"], false);
        assert_eq!(result["name_normalizations"][0]["effective"], "run_command");
        assert!(selected.as_ref().unwrap().contains("run_command"));
        let repeated = discover_once(
            &all,
            &mut selected,
            &json!({"names":["run_command"]}),
            &mut seen,
        )
        .unwrap();
        assert_eq!(repeated["repetition_blocked"], true);
        assert_eq!(raw["names"][0], "\"run_command\"");
    }
    #[test]
    fn malformed_or_unknown_discovery_names_cannot_change_selection() {
        let all = crate::tools::definitions();
        let mut selected = Some(core());
        let before = selected.clone();
        for name in [
            " run_command ",
            "\"missing_tool\"",
            "\"run_command",
            "[\"run_command\"]",
            "\"\\\"run_command\\\"\"",
        ] {
            assert!(discover(&all, &mut selected, &json!({"names":["read_file",name]})).is_err());
            assert_eq!(selected, before);
        }
        assert!(discover(&all, &mut selected, &json!({"offset":999})).is_err());
        assert_eq!(selected, before);
    }
}
