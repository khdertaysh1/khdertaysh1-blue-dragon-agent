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
    json!({"ok":false,"category":"invalid_arguments","error_code":"schema_validation_failed","execution_status":"not_executed","tool":tool,"field":field,"expected_type":expected,"actual_type":actual,"error":format!("Invalid arguments: {field} expected {expected}, got {actual}"),"example":if tool=="run_command" {json!({"program":"example-program","args":["--help"],"timeout_seconds":30})} else {json!({"note":"Use the advertised tool schema; no supplied values are echoed."})},"retry_hint":"Correct this field for this tool once. Successful discovery does not correct another tool. Do not repeat the same invalid call. No approval or execution occurred."})
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
                let mut error = invalid(tool, path, "advertised size bounds", kind(v));
                error[if v.is_array() {
                    "actual_count"
                } else {
                    "actual_length"
                }] = json!(size);
                for key in [low, high] {
                    if s[key].is_u64() {
                        error[key] = s[key].clone();
                    }
                }
                return Err(error);
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
            let mut error = invalid(tool, path, "number within advertised bounds", kind(v));
            for key in ["minimum", "maximum"] {
                if s[key].is_number() {
                    error[key] = s[key].clone();
                }
            }
            return Err(error);
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
    private_view_mode(value, false)
}
pub fn private_argument_view(value: &Value) -> Value {
    private_view_mode(value, true)
}
fn private_view_mode(value: &Value, hide_scalars: bool) -> Value {
    match value {
        Value::Number(_) | Value::Bool(_) if hide_scalars => {
            json!({"redacted_value":true,"actual_type":kind(value)})
        }
        Value::String(s) => json!({"redacted_string":true,"bytes":s.len()}),
        Value::Array(a) => json!(a
            .iter()
            .map(|v| private_view_mode(v, hide_scalars))
            .collect::<Vec<_>>()),
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
                        | "tool"
                        | "error_code"
                        | "actual_count"
                        | "actual_length"
                        | "minItems"
                        | "maxItems"
                        | "minLength"
                        | "maxLength"
                        | "minimum"
                        | "maximum"
                        | "correction_attempt"
                        | "correction_limit"
                );
                out.insert(
                    if safe {
                        k.clone()
                    } else {
                        format!("[redacted field #{index}]")
                    },
                    private_view_mode(v, hide_scalars),
                );
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}
/// Structural facts from the saved original call, not inferred from error prose.
pub fn argument_shape(tool: &str, original: &Value) -> Value {
    let fields: &[&str] = match tool {
        "run_command" => &["program", "args", "timeout_seconds"],
        "discover_tools" => &["names", "offset"],
        _ => &[],
    };
    let mut out = serde_json::Map::new();
    for field in fields {
        let value = original.get(*field);
        let mut shape = json!({"actual_type":value.map(kind).unwrap_or("missing")});
        if let Some(array) = value.and_then(Value::as_array) {
            shape["actual_count"] = json!(array.len());
        }
        out.insert((*field).into(), shape);
    }
    Value::Object(out)
}
/// Only native schema field paths and fixed diagnostic vocabulary may be shown.
/// Never whitelist arbitrary strings merely because their key says "error".
pub fn diagnostic_view(tool: &str, result: &Value) -> Value {
    let mut out = private_view(result);
    if !result.is_object() {
        return out;
    }
    for (key, allowed) in [
        ("category", &["invalid_arguments"][..]),
        ("error_code", &["schema_validation_failed"][..]),
        (
            "execution_status",
            &["not_executed", "completed", "unknown"][..],
        ),
        (
            "command_outcome",
            &[
                "success",
                "denied",
                "spawn_failure",
                "nonzero_exit",
                "timeout",
                "cancelled",
            ][..],
        ),
    ] {
        if result[key].as_str().is_some_and(|v| allowed.contains(&v)) {
            out[key] = result[key].clone();
        }
    }
    if result["category"] != "invalid_arguments" {
        return out;
    }
    let mut definitions = crate::tools::definitions();
    definitions
        .as_array_mut()
        .unwrap()
        .push(crate::tool_selection::definition());
    let Some(schema) = definitions
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["function"]["name"] == tool)
        .map(|d| &d["function"]["parameters"])
    else {
        return out;
    };
    out["tool"] = json!(tool);
    let types = [
        "null",
        "boolean",
        "integer",
        "number",
        "string",
        "array",
        "object",
        "missing",
        "bounded nesting",
        "one of the advertised types",
        "advertised enum value",
        "advertised schema alternative",
        "required field",
        "only advertised fields",
        "unexpected field",
        "advertised size bounds",
        "number within advertised bounds",
        "advertised tool",
        "unknown tool",
        "bounded array<string>",
        "oversized string",
        "array<string>",
        "invalid JSON string",
        "integer 1..300",
        "ambiguous numeric string",
        "invalid numeric string",
        "nonempty string <=200 bytes without NUL",
        "invalid string",
        "array with <=30 items",
        "oversized array",
        "strings <=1000 bytes without NUL",
        "invalid element",
    ];
    for key in ["expected_type", "actual_type"] {
        if result[key].as_str().is_some_and(|v| types.contains(&v)) {
            out[key] = result[key].clone();
        }
    }
    if let Some(path) = result["field"]
        .as_str()
        .filter(|p| safe_schema_path(p, schema))
    {
        out["field"] = json!(path);
    }
    for key in [
        "actual_count",
        "actual_length",
        "minItems",
        "maxItems",
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
        "correction_attempt",
        "correction_limit",
    ] {
        if result[key].is_u64() {
            out[key] = result[key].clone();
        }
    }
    // Reconstruct; a forged error/retry string must never be echoed.
    out["error"] = if out["field"].is_string()
        && out["expected_type"].is_string()
        && out["actual_type"].is_string()
    {
        json!(summary(&out))
    } else {
        json!("Invalid arguments; unrecognized diagnostic details remain redacted")
    };
    out["retry_hint"] = json!("Correct this tool's invalid arguments within its bounded correction allowance. Discovery alone does not correct another tool. No approval or execution occurred.");
    out
}
fn safe_schema_path(path: &str, schema: &Value) -> bool {
    if path.len() > 256 {
        return false;
    }
    let mut rest = path.strip_prefix('$').unwrap_or(path);
    let mut current = schema;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('[') {
            let Some(end) = r.find(']') else {
                return false;
            };
            let index = &r[..end];
            if index.is_empty()
                || !index.bytes().all(|b| b.is_ascii_digit())
                || current.get("items").is_none()
            {
                return false;
            }
            current = &current["items"];
            rest = &r[end + 1..];
        } else {
            rest = rest.strip_prefix('.').unwrap_or(rest);
            let end = rest.find(['.', '[']).unwrap_or(rest.len());
            let Some(child) = current["properties"].get(&rest[..end]) else {
                return false;
            };
            current = child;
            rest = &rest[end..];
        }
    }
    true
}
/// Called only on this turn's original dispatched result, never a model claim.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CorrectionOrigin {
    NotDispatched,
    Native,
    External,
}
pub fn correction_succeeded_from_dispatch(
    tool: &str,
    args: &Value,
    result: &Value,
    origin: CorrectionOrigin,
) -> bool {
    if origin == CorrectionOrigin::NotDispatched {
        return false;
    }
    if !result.is_object()
        || result["ok"] == false
        || result.get("error").is_some_and(|v| !v.is_null())
        || crate::reasoning::claims::is_historical(result)
        || [
            "denied",
            "failed",
            "stale",
            "cancelled",
            "repetition_blocked",
        ]
        .iter()
        .any(|k| result[*k] == true)
        || result
            .get("execution_status")
            .is_some_and(|v| v != "completed")
        || ["status", "state"].iter().any(|k| {
            if tool == "git_status" && *k == "status" && origin == CorrectionOrigin::Native {
                return false;
            }
            result[*k].as_str().is_some_and(|s| {
                let lower = s.to_ascii_lowercase();
                [
                    "fail",
                    "denied",
                    "unknown",
                    "stale",
                    "cancel",
                    "blocked",
                    "not_executed",
                    "interrupted",
                    "timeout",
                    "pending",
                ]
                .iter()
                .any(|bad| lower.contains(bad))
            })
        })
    {
        return false;
    }
    if origin == CorrectionOrigin::External {
        return result["ok"] == true;
    }
    // These native reads intentionally return a content/hash contract without ok.
    // Explicit ok=true must not bypass checking their actual contract either.
    match tool {
        "read_file" | "read_file_window" => {
            let Some(text) = result["content"].as_str() else {
                return false;
            };
            let hash_key = if tool == "read_file" {
                "sha256"
            } else {
                "window_sha256"
            };
            if result[hash_key] != crate::intelligence::code::hash(text.as_bytes())
                || result["path"].as_str().is_none_or(str::is_empty)
            {
                return false;
            }
            if tool == "read_file" {
                return true;
            }
            let (Some(start), Some(end), Some(size), Some(count)) = (
                result["byte_offset"].as_u64(),
                result["end_byte_offset"].as_u64(),
                result["file_bytes"].as_u64(),
                args["byte_count"].as_u64(),
            ) else {
                return false;
            };
            result["path"] == args["path"]
                && result["byte_offset"] == args["byte_offset"]
                && start.checked_add(text.len() as u64) == Some(end)
                && end <= size
                && count > 0
                && (text.len() as u64) <= count
                && result["eof"].as_bool() == Some(end == size)
                && (if end == size {
                    result["next_byte_offset"].is_null()
                } else {
                    result["next_byte_offset"].as_u64() == Some(end)
                })
        }
        "browser" if args["action"] == "status" => {
            result["ready"].is_boolean()
                && match result["backend"].as_str() {
                    Some("none") => {
                        result["ready"] == false
                            && result["reconnect_pending"].is_boolean()
                            && result["setup"].is_string()
                    }
                    Some("cdp") => {
                        result["port"].as_u64().is_some_and(|n| n > 0 && n <= 65535)
                            && result["dedicated_tab"].is_string()
                            && result["launched_by_blue_dragon"].is_boolean()
                    }
                    Some("extension") => {
                        result["port"].as_u64().is_some_and(|n| n > 0 && n <= 65535)
                            && result["connection_epoch"].is_u64()
                            && result["page_and_login_continuity_verified"] == false
                    }
                    _ => false,
                }
        }
        "search_file" => search_contract(args, result),
        "list_files" => result["files"].as_array().is_some_and(|rows| {
            rows.len() <= 300
                && rows
                    .iter()
                    .all(|r| r.as_str().is_some_and(|p| !p.is_empty()))
                && rows.windows(2).all(|w| w[0].as_str() <= w[1].as_str())
                && result["truncated"].as_bool() == Some(rows.len() >= 300)
        }),
        "file_info" => {
            result["path"] == args["path"]
                && result["bytes"].is_u64()
                && result["directory"].is_boolean()
                && result["readonly"].is_boolean()
        }
        "read_lines" => {
            let (Some(start), Some(count), Some(rows)) = (
                args["start"].as_u64(),
                args["count"].as_u64(),
                result["lines"].as_array(),
            ) else {
                return false;
            };
            result["path"] == args["path"]
                && result["start"].as_u64() == Some(start)
                && start > 0
                && count > 0
                && count <= 200
                && rows.len() as u64 <= count
                && rows.iter().enumerate().all(|(i, r)| {
                    r["line"].as_u64() == start.checked_add(i as u64) && r["text"].is_string()
                })
        }
        "search_files" => result["matches"].as_array().is_some_and(|rows| {
            rows.len() <= 80
                && rows.iter().all(|r| {
                    r["path"].as_str().is_some_and(|s| !s.is_empty())
                        && r["line"].as_u64().is_some_and(|n| n > 0)
                        && r["preview"]
                            .as_str()
                            .is_some_and(|s| s.chars().count() <= 240)
                })
                && result["truncated"].as_bool() == Some(rows.len() >= 80)
        }),
        "git_diff" => result["diff"].is_string(), // native git exit status was checked before Result::Ok
        "git_status" => result["status"].is_string(), // status is payload, never an execution enum
        "project_map" => {
            result["root"].as_str().is_some_and(|s| !s.is_empty())
                && result["files_inspected"]
                    .as_u64()
                    .is_some_and(|n| n <= 1200)
                && result["size_bytes_estimate"].is_u64()
                && result["languages"]
                    .as_object()
                    .is_some_and(|o| o.values().all(Value::is_u64))
                && result["manifests"]
                    .as_array()
                    .is_some_and(|a| a.iter().all(Value::is_string))
                && result["sample_files"]
                    .as_array()
                    .is_some_and(|a| a.iter().all(Value::is_string))
                && result["truncated"].is_boolean()
        }
        "file_outline" => outline_contract(args, result),
        "list_jobs" => result["jobs"].as_array().is_some_and(|jobs| {
            jobs.len() <= 20
                && jobs.iter().all(|job| {
                    job["ok"] == true
                        && job["job_id"].as_str().is_some_and(|s| {
                            s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
                        })
                })
        }),
        _ => result["ok"] == true,
    }
}
fn outline_contract(args: &Value, r: &Value) -> bool {
    let (Some(offset), Some(total), Some(items)) = (
        r["offset"].as_u64(),
        r["collected_matching_items"].as_u64(),
        r["items"].as_array(),
    ) else {
        return false;
    };
    let end = offset.checked_add(items.len() as u64);
    r["path"] == args["path"]
        && r["source_sha256"]
            .as_str()
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        && r["source_bytes"].as_u64().is_some_and(|n| n <= 128000)
        && r["language"].is_string()
        && r["structure_available"].is_boolean()
        && r["parse_errors"].is_boolean()
        && offset == args["offset"].as_u64().unwrap_or(0)
        && items.len() as u64 <= args["limit"].as_u64().unwrap_or(20)
        && end.is_some_and(|n| n <= total)
        && r["all_collected_items_returned"].as_bool() == Some(offset == 0 && end == Some(total))
        && if end == Some(total) {
            r["next_offset"].is_null()
        } else {
            r["next_offset"].as_u64() == end
        }
}
fn search_contract(args: &Value, r: &Value) -> bool {
    let (Some(start), Some(end), Some(size), Some(scan), Some(cap), Some(query), Some(rows)) = (
        r["byte_offset"].as_u64(),
        r["scanned_end_byte_offset"].as_u64(),
        r["file_bytes"].as_u64(),
        args["scan_bytes"].as_u64(),
        args["max_matches"].as_u64(),
        args["query"].as_str(),
        r["matches"].as_array(),
    ) else {
        return false;
    };
    if r["path"] != args["path"]
        || r["byte_offset"] != args["byte_offset"]
        || query.is_empty()
        || cap == 0
        || cap > 80
        || rows.len() as u64 > cap
        || end < start
        || end > size
        || end - start > scan
    {
        return false;
    }
    let mut previous = start;
    for hit in rows {
        let (Some(a), Some(b)) = (hit["byte_offset"].as_u64(), hit["end_byte_offset"].as_u64())
        else {
            return false;
        };
        if a < previous || a.checked_add(query.len() as u64) != Some(b) || b > end {
            return false;
        }
        previous = b;
    }
    let next = r["next_byte_offset"].as_u64();
    let complete = r["next_byte_offset"].is_null();
    if r["complete_from_offset"].as_bool() != Some(complete)
        || r["whole_file_searched"].as_bool() != Some(start == 0 && complete)
    {
        return false;
    }
    match r["stop_reason"].as_str() {
        Some("eof") => complete && end == size,
        Some("match_limit") => !complete && rows.len() as u64 == cap && next == Some(previous),
        Some("scan_limit") => {
            !complete && end < size && next.is_some_and(|n| n >= previous && n <= end)
        }
        _ => false,
    }
}
pub fn summary(result: &Value) -> String {
    if result["category"] == "invalid_arguments" {
        let mut text = format!(
            "Invalid arguments: {} expected {}, got {}",
            result["field"].as_str().unwrap_or("field"),
            result["expected_type"].as_str().unwrap_or("schema type"),
            result["actual_type"].as_str().unwrap_or("unknown")
        );
        if result["actual_count"].is_u64() {
            text.push_str(&format!(
                " (actual_count={}, minItems={}, maxItems={})",
                result["actual_count"], result["minItems"], result["maxItems"]
            ));
        }
        return text;
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
    fn correction_succeeded(tool: &str, args: &Value, result: &Value) -> bool {
        correction_succeeded_from_dispatch(tool, args, result, CorrectionOrigin::Native)
    }
    #[test]
    fn native_correction_contracts_require_dispatch_and_reject_failure_metadata() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("app.js"),
            "const needle = 'عربي';\nfunction found() { return needle; }\n",
        )
        .unwrap();
        let tools = crate::tools::ToolBox::new(
            root.path().canonicalize().unwrap(),
            crate::tools::ApprovalMode::ReadOnly,
        );
        let cases = [
            (
                "search_file",
                json!({"path":"app.js","query":"needle","byte_offset":0,"scan_bytes":1024,"max_matches":5}),
            ),
            ("list_files", json!({"path":"."})),
            ("read_lines", json!({"path":"app.js","start":1,"count":2})),
            ("file_info", json!({"path":"app.js"})),
            ("search_files", json!({"query":"needle"})),
            ("file_outline", json!({"path":"app.js"})),
            ("project_map", json!({})),
            ("list_jobs", json!({})),
        ];
        for (tool, args) in cases {
            let result = crate::tools::invoke_result(&tools, tool, &args).unwrap();
            assert!(
                correction_succeeded(tool, &args, &result),
                "{tool}: {result}"
            );
            for origin in [CorrectionOrigin::NotDispatched, CorrectionOrigin::External] {
                assert!(!correction_succeeded_from_dispatch(
                    tool, &args, &result, origin
                ));
            }
            for flag in [
                "failed",
                "denied",
                "stale",
                "cancelled",
                "cached",
                "cached_result",
                "repetition_blocked",
            ] {
                let mut bad = result.clone();
                bad[flag] = json!(true);
                assert!(!correction_succeeded(tool, &args, &bad), "{tool}: {bad}");
            }
            for status in [
                "failed",
                "denied",
                "unknown",
                "stale",
                "cancelled",
                "not_executed",
            ] {
                let mut bad = result.clone();
                bad["execution_status"] = json!(status);
                assert!(!correction_succeeded(tool, &args, &bad));
            }
            let mut historical = result.clone();
            historical["fresh_evidence"] = json!(false);
            assert!(!correction_succeeded(tool, &args, &historical));
            let mut bad = result.clone();
            bad["ok"] = json!(false);
            assert!(!correction_succeeded(tool, &args, &bad));
            assert!(
                !correction_succeeded(tool, &args, &json!({"ok":true})),
                "{tool}"
            );
        }
    }
    #[test]
    fn native_search_contract_checks_utf8_ranges_and_partial_scans() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("app.js"),
            format!("{}عربي عربي", "x".repeat(1100)),
        )
        .unwrap();
        let tools = crate::tools::ToolBox::new(
            root.path().canonicalize().unwrap(),
            crate::tools::ApprovalMode::ReadOnly,
        );
        for (budget, cap) in [(1024, 5), (2048, 1), (2048, 5)] {
            let args = json!({"path":"app.js","query":"عربي","byte_offset":0,"scan_bytes":budget,"max_matches":cap});
            let result = crate::tools::invoke_result(&tools, "search_file", &args).unwrap();
            assert!(
                correction_succeeded("search_file", &args, &result),
                "{result}"
            );
            for key in ["scanned_end_byte_offset", "file_bytes", "byte_offset"] {
                let mut bad = result.clone();
                bad[key] = if key == "file_bytes" {
                    json!(0)
                } else {
                    json!(u64::MAX)
                };
                assert!(!correction_succeeded("search_file", &args, &bad), "{bad}");
            }
            let mut bad = result.clone();
            bad["whole_file_searched"] = json!(!result["whole_file_searched"].as_bool().unwrap());
            assert!(!correction_succeeded("search_file", &args, &bad));
        }
    }
    #[test]
    fn browser_status_completion_is_not_dom_readiness() {
        let args = json!({"action":"status"});
        let result = json!({"ready":false,"backend":"none","reconnect_pending":false,"setup":"Approval required"});
        assert!(correction_succeeded("browser", &args, &result));
        assert!(!correction_succeeded(
            "browser",
            &json!({"action":"snapshot"}),
            &result
        ));
        assert!(!correction_succeeded_from_dispatch(
            "browser",
            &args,
            &result,
            CorrectionOrigin::NotDispatched
        ));
        for flag in ["failed", "denied", "cached", "stale"] {
            let mut bad = result.clone();
            bad[flag] = json!(true);
            assert!(!correction_succeeded("browser", &args, &bad));
        }
        for field in ["backend", "ready", "reconnect_pending", "setup"] {
            let mut bad = result.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(!correction_succeeded("browser", &args, &bad));
        }
    }
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
    #[test]
    fn safe_diagnostics_keep_counts_but_never_echo_arbitrary_strings() {
        for count in [0, 9] {
            let original = json!({"names":vec!["PRIVATE";count]});
            let e = prepare(
                "discover_tools",
                &original,
                &json!([crate::tool_selection::definition()]),
            )
            .err()
            .unwrap();
            assert_eq!(
                argument_shape("discover_tools", &original)["names"]["actual_count"],
                count
            );
            assert_eq!(e["actual_count"], count);
            assert_eq!(e["minItems"], 1);
            assert_eq!(e["maxItems"], 8);
            let view = diagnostic_view("discover_tools", &e);
            assert_eq!(view["field"], "$.names");
            assert_eq!(view["actual_type"], "array");
            assert_eq!(view["error_code"], "schema_validation_failed");
            assert_eq!(view["actual_count"], count);
            assert!(view["error"].as_str().unwrap().contains("actual_count="));
            assert!(!view.to_string().contains("PRIVATE"));
        }
        assert!(
            !private_argument_view(&json!({"args":[73529,"SECRET_VALUE"]}))
                .to_string()
                .contains("73529")
        );
        let secret = "SECRET_VALUE";
        let forged = json!({"category":"invalid_arguments","error":secret,"retry_hint":secret,
            "field":"$.SECRET_VALUE","expected_type":secret,"actual_type":secret,"execution_status":secret,
            "program":secret,"args":[secret],"stdout":secret,"nested":{"field":"$.args","error":secret}});
        let view = diagnostic_view("run_command", &forged);
        assert!(!view.to_string().contains(secret));
        assert!(view["field"].is_object());
        assert_eq!(
            diagnostic_view("run_command", &prep(json!({"args":[]})).err().unwrap())["actual_type"],
            "missing"
        );
        assert!(!safe_schema_path(
            "$.args[0].secret",
            &json!({"properties":{"args":{"items":{"type":"string"}}}})
        ));
    }
    #[test]
    fn corrected_native_reads_require_valid_fresh_contracts() {
        let args = json!({"path":"app.js","byte_offset":0,"byte_count":64});
        let mut window = json!({"path":"app.js","content":"abc","window_sha256":crate::intelligence::code::hash(b"abc"),"byte_offset":0,"end_byte_offset":3,"file_bytes":3,"eof":true,"next_byte_offset":null});
        let file = json!({"path":"app.js","content":"abc","sha256":crate::intelligence::code::hash(b"abc")});
        for (tool, base) in [("read_file_window", window.clone()), ("read_file", file)] {
            assert!(correction_succeeded(tool, &args, &base));
            for patch in [
                json!({"ok":false}),
                json!({"denied":true}),
                json!({"stale":true}),
                json!({"failed":true}),
                json!({"cancelled":true}),
                json!({"cached_result":true}),
                json!({"cached":true}),
                json!({"fresh_evidence":false}),
                json!({"original_evidence_id":null}),
                json!({"status":"denied by user"}),
                json!({"state":"stale"}),
                json!({"error":"failed"}),
            ] {
                let mut bad = base.clone();
                bad["ok"] = json!(true);
                for (k, v) in patch.as_object().unwrap() {
                    bad[k] = v.clone();
                }
                assert!(!correction_succeeded(tool, &args, &bad), "{tool}: {bad}");
            }
            for status in [
                "failed",
                "stale",
                "unknown",
                "denied",
                "not_executed",
                "cancelled",
                "running",
            ] {
                let mut bad = base.clone();
                bad["ok"] = json!(true);
                bad["execution_status"] = json!(status);
                assert!(!correction_succeeded(tool, &args, &bad));
            }
            let mut corrupt = base.clone();
            corrupt["content"] = json!("changed");
            assert!(!correction_succeeded(tool, &args, &corrupt));
        }
        window["end_byte_offset"] = json!(2);
        assert!(!correction_succeeded("read_file_window", &args, &window));
        assert!(!correction_succeeded(
            "other_tool",
            &args,
            &json!({"content":"abc"})
        ));
        assert!(!correction_succeeded(
            "read_file",
            &args,
            &json!({"ok":true})
        ));
    }
}
