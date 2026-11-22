//! Validate the command/discovery contract against the exact request advertisement.
//! No field aliases, shell conversion, JSON repair or permission grant.
use serde_json::{json, Value};
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
fn failure(tool: &str, field: &str, expected: &str, actual: &str) -> Value {
    json!({"ok":false,"category":"invalid_arguments","execution_status":"not_executed","tool":tool,"field":field,"expected_type":expected,"actual_type":actual,"error":format!("Invalid arguments: {field} expected {expected}, got {actual}"),"retry_hint":"Correct this tool's arguments using its advertised schema. No approval or execution occurred. Do not repeat completed or denied actions."})
}
fn check(tool: &str, value: &Value, schema: &Value, field: &str) -> Result<(), Value> {
    if let Some(expected) = schema["type"].as_str() {
        if kind(value) != expected && !(expected == "number" && kind(value) == "integer") {
            return Err(failure(tool, field, expected, kind(value)));
        }
    }
    if schema["enum"]
        .as_array()
        .is_some_and(|a| !a.contains(value))
    {
        return Err(failure(tool, field, "advertised enum value", kind(value)));
    }
    if let Some(t) = value.as_str() {
        let n = t.chars().count() as u64;
        if schema["minLength"].as_u64().is_some_and(|m| n < m)
            || schema["maxLength"].as_u64().is_some_and(|m| n > m)
        {
            return Err(failure(tool, field, "advertised string bounds", "string"));
        }
    }
    if let Some(obj) = value.as_object() {
        if let Some(req) = schema["required"].as_array() {
            for key in req.iter().filter_map(Value::as_str) {
                if !obj.contains_key(key) {
                    return Err(failure(
                        tool,
                        &format!("{field}.{key}"),
                        "required field",
                        "missing",
                    ));
                }
            }
        }
        for (key, val) in obj {
            let safe =
                if key.len() <= 64 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    key.as_str()
                } else {
                    "<unrecognized>"
                };
            let path = format!("{field}.{safe}");
            if let Some(s) = schema["properties"].get(key) {
                check(tool, val, s, &path)?;
            } else if schema["additionalProperties"] == false {
                let mut e = failure(tool, &path, "only advertised fields", "unexpected field");
                e["allowed_fields"] = json!(schema["properties"]
                    .as_object()
                    .map(|m| m.keys().collect::<Vec<_>>())
                    .unwrap_or_default());
                return Err(e);
            }
        }
    }
    if let Some(values) = value.as_array() {
        let min = schema["minItems"].as_u64().unwrap_or(0);
        let max = schema["maxItems"].as_u64().unwrap_or(u64::MAX);
        if (values.len() as u64) < min || (values.len() as u64) > max {
            let mut e = failure(tool, field, "advertised size bounds", "array");
            e["actual_count"] = json!(values.len());
            e["minItems"] = json!(min);
            e["maxItems"] = json!(max);
            return Err(e);
        }
        for (i, v) in values.iter().enumerate() {
            if let Some(s) = schema.get("items") {
                check(tool, v, s, &format!("{field}[{i}]"))?;
            }
        }
    }
    if let Some(n) = value.as_f64() {
        if schema["minimum"].as_f64().is_some_and(|x| n < x)
            || schema["maximum"].as_f64().is_some_and(|x| n > x)
        {
            let mut e = failure(tool, field, "advertised numeric range", kind(value));
            e["minimum"] = schema["minimum"].clone();
            e["maximum"] = schema["maximum"].clone();
            return Err(e);
        }
    }
    Ok(())
}
/// Only unambiguous compatibility conversions; original arguments stay in history.
pub fn normalize(tool: &str, args: &Value) -> (Value, Value) {
    let mut out = args.clone();
    let mut changes = Vec::new();
    if tool == "run_command" {
        if let Some(raw) = args["args"].as_str().filter(|s| s.len() <= 40000) {
            if let Ok(v) = serde_json::from_str::<Value>(raw) {
                if v.as_array().is_some_and(|a| {
                    a.len() <= 30
                        && a.iter()
                            .all(|v| v.as_str().is_some_and(|s| s.len() <= 1000))
                }) {
                    out["args"] = v;
                    changes.push(json!({"field":"$.args","from":"JSON string","to":"array<string>","decode_passes":1}));
                }
            }
        }
        if let Some(raw) = args["timeout_seconds"].as_str() {
            if !raw.is_empty()
                && raw.bytes().all(|b| b.is_ascii_digit())
                && (raw == "0" || !raw.starts_with('0'))
            {
                if let Ok(n) = raw.parse::<u64>() {
                    if (1..=300).contains(&n) {
                        out["timeout_seconds"] = json!(n);
                        changes.push(json!({"field":"$.timeout_seconds","from":"decimal string","to":"integer"}));
                    }
                }
            }
        }
    }
    (out, Value::Array(changes))
}
pub fn validate(tool: &str, args: &Value, advertised: &Value) -> Result<(), Value> {
    let Some(d) = advertised
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["function"]["name"] == tool)
    else {
        let mut e = failure(tool, "$", "advertised tool", "unknown tool");
        e["category"] = json!("tool_not_advertised");
        return Err(e);
    };
    check(tool, args, &d["function"]["parameters"], "$")?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_command_schema_and_negative_controls() {
        let all = crate::tools::definitions();
        let good = json!({"program":"printf","args":["ok"],"timeout_seconds":30});
        assert!(validate("run_command", &good, &all).is_ok());
        assert_eq!(
            validate("run_command", &good, &json!([])).unwrap_err()["expected_type"],
            "advertised tool"
        );
        let mut bad = good.clone();
        bad["cwd"] = json!("SECRET_VALUE");
        let e = validate("run_command", &bad, &all).unwrap_err();
        assert_eq!(e["field"], "$.cwd");
        assert!(!e.to_string().contains("SECRET_VALUE"));
        for bad in [
            json!({"args":[]}),
            json!({"program":42,"args":[]}),
            json!({"program":"printf","args":"[broken"}),
            json!({"program":"printf","args":[7]}),
            json!({"program":"printf","args":[],"timeout_seconds":301}),
        ] {
            assert!(validate("run_command", &bad, &all).is_err());
        }
    }
    #[test]
    fn discovery_count_is_observed_not_guessed() {
        let d = json!([crate::tool_selection::definition()]);
        for count in [0, 9] {
            let e = validate(
                "discover_tools",
                &json!({"names":vec!["run_command";count]}),
                &d,
            )
            .unwrap_err();
            assert_eq!(e["actual_count"], count);
            assert_eq!(e["minItems"], 1);
            assert_eq!(e["maxItems"], 8);
        }
    }
}
