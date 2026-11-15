//! Central post-assembly validation. Normalization never grants permission.
use serde_json::{json, Value};
pub struct Prepared {
    pub arguments: Value,
    pub changes: Vec<Value>,
}
fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
fn invalid(tool: &str, field: &str, expected: &str, actual: &str) -> Value {
    json!({"ok":false,"category":"invalid_arguments","execution_status":"not_executed","tool":tool,"field":field,"expected_type":expected,"actual_type":actual,"error":format!("Invalid arguments: {field} expected {expected}, got {actual}"),"example":if tool=="run_command" {json!({"program":"example-program","args":["--help"],"timeout_seconds":30})} else {json!({"note":"Use the advertised tool schema; no supplied values are echoed."})},"retry_hint":"Correct this field once. Do not repeat the same invalid call. No approval or execution occurred."})
}
fn validate(tool: &str, path: &str, v: &Value, s: &Value, depth: usize) -> Result<(), Value> {
    if depth > 24 {
        return Err(invalid(tool, path, "bounded nesting", kind(v)));
    }
    if let Some(expected) = s.get("type") {
        let accepts = |t: &str| t == kind(v) || (t == "number" && v.is_number());
        let ok = expected.as_str().map(accepts).unwrap_or_else(|| {
            expected
                .as_array()
                .is_some_and(|a| a.iter().any(|t| t.as_str().is_some_and(accepts)))
        });
        if !ok {
            return Err(invalid(
                tool,
                path,
                expected.as_str().unwrap_or("one of the advertised types"),
                kind(v),
            ));
        }
    }
    if s["enum"].as_array().is_some_and(|a| !a.contains(v)) {
        return Err(invalid(tool, path, "advertised enum value", kind(v)));
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(options) = s[keyword].as_array() {
            let matches = options
                .iter()
                .filter(|opt| validate(tool, path, v, opt, depth + 1).is_ok())
                .count();
            if matches == 0 || (keyword == "oneOf" && matches != 1) {
                return Err(invalid(
                    tool,
                    path,
                    "advertised schema alternative",
                    kind(v),
                ));
            }
        }
    }
    if let Some(obj) = v.as_object() {
        if let Some(required) = s["required"].as_array() {
            for key in required.iter().filter_map(Value::as_str) {
                if !obj.contains_key(key) {
                    return Err(invalid(
                        tool,
                        &format!("{path}.{key}"),
                        s["properties"][key]["type"]
                            .as_str()
                            .unwrap_or("required field"),
                        "missing",
                    ));
                }
            }
        }
        for (key, value) in obj {
            if let Some(child) = s["properties"].get(key) {
                validate(tool, &format!("{path}.{key}"), value, child, depth + 1)?;
            } else if s["additionalProperties"] == false {
                return Err(invalid(
                    tool,
                    path,
                    "only advertised fields",
                    "unexpected field",
                ));
            }
        }
    }
    if let Some(a) = v.as_array() {
        for (i, item) in a.iter().enumerate() {
            if s["items"].is_object() {
                validate(tool, &format!("{path}[{i}]"), item, &s["items"], depth + 1)?;
            }
        }
    }
    for (low, high, size) in [
        ("minItems", "maxItems", v.as_array().map(Vec::len)),
        (
            "minLength",
            "maxLength",
            v.as_str().map(|s| s.chars().count()),
        ),
    ] {
        if let Some(size) = size {
            if s[low].as_u64().is_some_and(|n| (size as u64) < n)
                || s[high].as_u64().is_some_and(|n| (size as u64) > n)
            {
                return Err(invalid(tool, path, "advertised size bounds", kind(v)));
            }
        }
    }
    if let Some(schemas) = s["allOf"].as_array() {
        for child in schemas {
            validate(tool, path, v, child, depth + 1)?;
        }
    }
    if let Some(n) = v.as_f64() {
        if s["minimum"].as_f64().is_some_and(|x| n < x)
            || s["maximum"].as_f64().is_some_and(|x| n > x)
        {
            return Err(invalid(
                tool,
                path,
                "number within advertised bounds",
                kind(v),
            ));
        }
    }
    Ok(())
}
pub fn prepare(tool: &str, original: &Value, definitions: &Value) -> Result<Prepared, Value> {
    let schema = definitions
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["function"]["name"] == tool)
        .map(|d| &d["function"]["parameters"])
        .ok_or_else(|| invalid(tool, "$", "advertised tool", "unknown tool"))?;
    let mut arguments = original.clone();
    let mut changes = Vec::new();
    if tool == "run_command" {
        if let Some(raw) = arguments["args"].as_str() {
            if raw.len() > 128 * 1024 {
                return Err(invalid(
                    tool,
                    "args",
                    "bounded array<string>",
                    "oversized string",
                ));
            }
            let parsed: Value = serde_json::from_str(raw)
                .map_err(|_| invalid(tool, "args", "array<string>", "invalid JSON string"))?;
            if !parsed.is_array() {
                return Err(invalid(tool, "args", "array<string>", kind(&parsed)));
            }
            arguments["args"] = parsed;
            changes.push(json!({"field":"args","from_type":"string","to_type":"array","operation":"single_json_decode"}));
        }
        if let Some(raw) = arguments["timeout_seconds"].as_str() {
            if raw.is_empty()
                || raw.len() > 3
                || !raw.bytes().all(|c| c.is_ascii_digit())
                || (raw.len() > 1 && raw.starts_with('0'))
            {
                return Err(invalid(
                    tool,
                    "timeout_seconds",
                    "integer 1..300",
                    "ambiguous numeric string",
                ));
            }
            let n = raw.parse::<u64>().map_err(|_| {
                invalid(
                    tool,
                    "timeout_seconds",
                    "integer 1..300",
                    "invalid numeric string",
                )
            })?;
            arguments["timeout_seconds"] = json!(n);
            changes.push(json!({"field":"timeout_seconds","from_type":"string","to_type":"integer","operation":"decimal_parse"}));
        }
    }
    validate(tool, "$", &arguments, schema, 0)?;
    if tool == "run_command" {
        let program = arguments["program"].as_str().unwrap_or("");
        let args = arguments["args"].as_array().unwrap();
        if program.is_empty() || program.len() > 200 || program.contains('\0') {
            return Err(invalid(
                tool,
                "program",
                "nonempty string <=200 bytes without NUL",
                "invalid string",
            ));
        }
        if args.len() > 30 {
            return Err(invalid(
                tool,
                "args",
                "array with <=30 items",
                "oversized array",
            ));
        }
        if args.iter().any(|x| {
            x.as_str()
                .is_none_or(|s| s.len() > 1000 || s.contains('\0'))
        }) {
            return Err(invalid(
                tool,
                "args",
                "strings <=1000 bytes without NUL",
                "invalid element",
            ));
        }
    }
    Ok(Prepared { arguments, changes })
}
/// Deterministic local inspection omits arbitrary strings rather than guessing
/// which credentials, command bodies or stdout fragments might be sensitive.
pub fn private_view(value: &Value) -> Value {
    match value {
        Value::String(s) => json!({"redacted_string":true,"bytes":s.len()}),
        Value::Array(a) => json!(a.iter().map(private_view).collect::<Vec<_>>()),
        Value::Object(o) => {
            let mut out = serde_json::Map::new();
            for (index, (k, v)) in o.iter().enumerate() {
                let safe = matches!(
                    k.as_str(),
                    "program"
                        | "args"
                        | "timeout_seconds"
                        | "ok"
                        | "error"
                        | "category"
                        | "execution_status"
                        | "command_outcome"
                        | "exit_code"
                        | "stdout"
                        | "stderr"
                        | "argument_normalization"
                        | "changes"
                        | "effective_arguments"
                        | "original_preserved_in_tool_call"
                        | "grants_permission"
                        | "field"
                        | "from_type"
                        | "to_type"
                        | "operation"
                        | "expected_type"
                        | "actual_type"
                        | "example"
                        | "retry_hint"
                        | "denied"
                        | "status"
                        | "evidence_id"
                        | "os_error_code"
                );
                out.insert(
                    if safe {
                        k.clone()
                    } else {
                        format!("[redacted field #{index}]")
                    },
                    private_view(v),
                );
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}
pub fn summary(result: &Value) -> String {
    if result["category"] == "invalid_arguments" {
        return format!(
            "Invalid arguments: {} expected {}, got {}",
            result["field"].as_str().unwrap_or("field"),
            result["expected_type"].as_str().unwrap_or("schema type"),
            result["actual_type"].as_str().unwrap_or("unknown")
        );
    }
    if result["denied"] == true || result["status"] == "denied by user" {
        return "Denied by user; not executed".into();
    }
    match result["command_outcome"].as_str() {
        Some("spawn_failure") => format!(
            "Spawn failure (OS code {}); no process started",
            result["os_error_code"]
        ),
        Some("timeout") => "Timeout; partial effects possible, do not replay".into(),
        Some("cancelled") if result["execution_status"] == "not_executed" => {
            "Cancelled; no process started".into()
        }
        Some("cancelled") => "Cancelled; partial effects possible, do not replay".into(),
        Some("nonzero_exit") => format!("Nonzero exit: {}", result["exit_code"]),
        _ => {
            if result["ok"] == false {
                "Tool failed; inspect saved result locally".into()
            } else {
                "Completed".into()
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn prep(v: Value) -> Result<Prepared, Value> {
        prepare("run_command", &v, &crate::tools::definitions())
    }
    #[test]
    fn original_payload_is_normalized_once_without_mutation() {
        let original = json!({"program":"curl","args":"[\"-sS\", \"-I\", \"--connect-timeout\", \"10\", \"--max-time\", \"20\", \"https://flatscout.co.uk\"]","timeout_seconds":"30"});
        let p = prep(original.clone()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.changes.len(), 2);
        assert_eq!(p.arguments["args"][6], "https://flatscout.co.uk");
        assert_eq!(p.arguments["timeout_seconds"], 30);
        assert!(original["args"].is_string());
        assert!(prep(p.arguments)
            .unwrap_or_else(|e| panic!("{e}"))
            .changes
            .is_empty());
    }
    #[test]
    fn invalid_values_never_become_commands() {
        for value in [
            json!({"args":[]}),
            json!({"program":3,"args":[]}),
            json!({"program":"","args":[]}),
            json!({"program":"x","args":"["}),
            json!({"program":"x","args":"\"[]\""}),
            json!({"program":"x","args":[1]}),
            json!({"program":"x","args":"[1]"}),
            json!({"program":"x","args":vec!["x";31]}),
            json!({"program":"x","args":["x".repeat(1001)]}),
            json!({"program":"x","args":[],"timeout_seconds":"301"}),
            json!({"program":"x","args":[],"timeout_seconds":" 30"}),
            json!({"program":"x","args":[],"timeout_seconds":"30.0"}),
            json!({"program":"x","args":[],"timeout_seconds":0}),
            json!({"command":"echo x","args":[]}),
        ] {
            let e = prep(value).err().expect("invalid accepted");
            assert_eq!(e["execution_status"], "not_executed");
            assert!(e.get("expected_type").is_some());
        }
    }
    #[test]
    fn argument_contents_are_not_shell_decoded() {
        let a = json!({"program":"fixture","args":["$(secret)","a b","مرحبا", "-H", "Authorization: SECRET"],"timeout_seconds":30});
        let p = prep(a.clone()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.arguments, a);
        let rendered = private_view(&p.arguments).to_string();
        assert!(!rendered.contains("SECRET"));
        assert!(!rendered.contains("$(secret)"));
        let unknown = private_view(&json!({"SECRET_KEY_NAME":"SECRET_VALUE"})).to_string();
        assert!(!unknown.contains("SECRET"));
    }
}
