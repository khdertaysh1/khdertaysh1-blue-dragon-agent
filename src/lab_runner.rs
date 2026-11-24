//! Explicitly approved, loopback-only, bounded Between Bytes marker workflow.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};
const MARKERS: [&str; 3] = [
    "BLUE_BASE_MARKER",
    "BLUE_VARIANT_MARKER",
    "BLUE_BASE_MARKER",
];
pub(crate) fn probes(scenario: &str) -> Result<[String; 3]> {
    match scenario {
        "literal_marker" => Ok(MARKERS.map(str::to_owned)),
        "inert_html" => Ok(MARKERS.map(|marker| format!("<blue-probe>{marker}</blue-probe>"))),
        _ => bail!("Use literal_marker or inert_html; arbitrary payloads are not accepted"),
    }
}
fn encoded_probe(probe: &str) -> String {
    probe
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
pub fn definition() -> Value {
    json!({"type":"function","function":{"name":"run_local_lab","description":"Opt-in Between Bytes loopback reflected-marker workflow. run requires explicit interact scope and human approval for exactly three harmless GETs (baseline/variant/control). No browser execution, credentials, redirects or exploit claim. A persisted run ID is never replayed, including interrupted/unknown outcomes. status only reads the saved bounded report. Use a fresh run ID only after a new user request to repeat.","parameters":{"type":"object","properties":{"action":{"type":"string","enum":["run","status"]},"run_id":{"type":"string"},"scenario":{"type":"string","enum":["literal_marker","inert_html"],"description":"Default literal_marker; inert_html adds a fixed custom tag without scripts/events/links to observe encoding, not browser execution."},"url":{"type":"string","description":"Exact HTTP 127.0.0.1:port/preview/reflected-xss/vulnerable or /fixed, no query or fragment"}},"required":["action","run_id"],"additionalProperties":false}}})
}
pub(crate) fn target(raw: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw)?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(
            url.path(),
            "/preview/reflected-xss/vulnerable" | "/preview/reflected-xss/fixed"
        )
    {
        bail!(
            "Use the exact local Between Bytes preview URL without query, credentials or fragment"
        );
    }
    Ok(url)
}
fn save(path: &Path, state: &Value) -> Result<()> {
    let mut temp =
        tempfile::NamedTempFile::new_in(path.parent().context("Missing run directory")?)?;
    serde_json::to_writer(&mut temp, state)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
fn historical(mut state: Value) -> Value {
    state["fresh_evidence"] = json!(false);
    state["reused_local_copy"] = json!(true);
    state["network_requests_this_call"] = json!(0);
    state
}
pub fn run(tools: &crate::tools::ToolBox, directory: &Path, args: &Value) -> Result<Value> {
    let id = args["run_id"].as_str().context("run_id required")?;
    if id.is_empty()
        || id.len() > 48
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        bail!("Invalid run_id");
    }
    let action = args["action"].as_str().context("action required")?;
    if !matches!(action, "run" | "status") {
        bail!("Use run/status");
    }
    let path = directory.join(format!("{id}.json"));
    if path.exists() {
        if fs::metadata(&path)?.len() > 32768 {
            bail!("Run checkpoint exceeds limit; no replay permitted");
        }
        let bytes = fs::read(&path)?;
        return Ok(historical(
            serde_json::from_slice(&bytes)
                .context("Invalid run checkpoint; no replay permitted")?,
        ));
    }
    if action == "status" {
        bail!("No saved run");
    }
    let scenario = args["scenario"].as_str().unwrap_or("literal_marker");
    let probe_inputs = probes(scenario)?;
    let url = target(args["url"].as_str().context("url required")?)?;
    if !tools.scope.enabled {
        bail!("Explicit interact scope is required");
    }
    tools.scope.check(url.as_str(), "interact")?;
    fs::create_dir_all(directory)?;
    if fs::read_dir(directory)?.take(21).count() >= 20 {
        bail!("Session lab run limit reached (20); inspect existing reports");
    }
    // Durable exclusive reservation exists before approval/network. Even a corrupt
    // or empty reservation blocks replay after process failure.
    let reservation = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    reservation.sync_all()?;
    drop(reservation);
    let mut state = json!({"ok":false,"run_id":id,"target":url.as_str(),"state":"awaiting_approval","request_budget":3,"requests_attempted":0,"observations":[],"automatic_replay_permitted":false,"browser_execution_observed":false,"vulnerability_confirmed":false,"protection_verified":false,"recipe":{"scenario":scenario,"method":"GET","query_key":"q","inputs":probe_inputs,"preview_token":"00000000000000000000000000000000","cookies_sent":false,"redirects_followed":false,"sequence":["baseline","variant","repeated_control"]},"limits":"Harmless fixed probes only; raw or escaped source sequences do not establish HTML parsing or script execution. No browser execution, causal proof, session stability, vulnerability or protection established. SHA256 checks consistency, not authenticity."});
    save(&path, &state)?;
    if !tools.approve("Run local lab marker comparison (interact)", &format!("{}\nExactly 3 sequential GETs; 5-second timeout and 64 KiB body limit each. No cookies, proxy or redirect following. No automatic retry, including after restart. Scenario: {}. Inputs: {:?}. Fixed harmless markers/custom tags only; no scripts, events or links. This is not browser execution.",url,scenario,probe_inputs))? {
        state["state"]=json!("denied");state["execution_status"]=json!("not_executed");state["denied"]=json!(true);save(&path,&state)?;return Ok(state);
    }
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()?;
    for (index, marker) in probe_inputs.iter().enumerate() {
        if !tools.allows_url_inspection() || tools.scope.check(url.as_str(), "interact").is_err() {
            state["state"] = json!("stopped_before_next_request");
            save(&path, &state)?;
            return Ok(state);
        }
        let mut request = url.clone();
        request
            .query_pairs_mut()
            .append_pair("q", marker)
            .append_pair("token", "00000000000000000000000000000000");
        state["state"] = json!("request_outcome_unknown");
        state["execution_status"] = json!("unknown");
        state["requests_attempted"] = json!(index + 1);
        state["pending_step"] = json!(index);
        save(&path, &state)?;
        tools.progress(format!("Local lab {}: request {}/3", id, index + 1));
        let observation = (|| -> Result<Value> {
            let response = client.get(request).send()?;
            let status = response.status().as_u16();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            let mut body = Vec::new();
            response.take(65537).read_to_end(&mut body)?;
            let complete = body.len() <= 65536;
            body.truncate(65536);
            let text = std::str::from_utf8(&body).ok();
            Ok(
                json!({"step":index,"status":status,"content_type_sha256":crate::intelligence::code::hash(content_type.as_bytes()),"body_sha256":crate::intelligence::code::hash(&body),"bytes_captured":body.len(),"complete":complete && text.is_some(),"raw_probe_sequence_seen":text.is_some_and(|t|t.contains(marker)),"escaped_probe_sequence_seen":text.is_some_and(|t|t.contains(&encoded_probe(marker))),"baseline_marker_seen":text.is_some_and(|t|t.contains(MARKERS[0])),"variant_marker_seen":text.is_some_and(|t|t.contains(MARKERS[1]))}),
            )
        })();
        match observation {
            Ok(observation) => {
                let usable = observation["complete"] == true
                    && observation["status"]
                        .as_u64()
                        .is_some_and(|s| (200..300).contains(&s));
                state["observations"]
                    .as_array_mut()
                    .unwrap()
                    .push(observation);
                state["pending_step"] = Value::Null;
                state["execution_status"] = json!("observed");
                state["state"] = json!(if usable {
                    "step_observed"
                } else {
                    "stopped_unusable_response"
                });
                save(&path, &state)?;
                if !usable {
                    return Ok(state);
                }
            }
            Err(_) => {
                state["error_class"] = json!("transport_or_body_read_failed_no_retry");
                save(&path, &state)?;
                return Ok(state);
            }
        }
    }
    let rows = state["observations"].as_array().unwrap();
    let stable = ["status", "content_type_sha256", "body_sha256"]
        .iter()
        .all(|key| rows[0][*key] == rows[2][*key]);
    let differential = rows[0]["baseline_marker_seen"] == true
        && rows[0]["variant_marker_seen"] == false
        && rows[1]["baseline_marker_seen"] == false
        && rows[1]["variant_marker_seen"] == true
        && rows[2]["baseline_marker_seen"] == true
        && rows[2]["variant_marker_seen"] == false;
    let raw_only = rows
        .iter()
        .all(|r| r["raw_probe_sequence_seen"] == true && r["escaped_probe_sequence_seen"] == false);
    let escaped_only = rows
        .iter()
        .all(|r| r["escaped_probe_sequence_seen"] == true && r["raw_probe_sequence_seen"] == false);
    state["verdict"] = json!(if !stable {
        "inconclusive_control_changed"
    } else if scenario == "inert_html" {
        if !differential {
            "inconclusive_marker_not_differential"
        } else if raw_only {
            "raw_probe_sequence_observed"
        } else if escaped_only {
            "escaped_probe_sequence_observed"
        } else {
            "inconclusive_mixed_or_missing_encoding"
        }
    } else if differential {
        "differential_literal_marker_only"
    } else {
        "inconclusive_marker_not_differential"
    });
    state["control_body_and_metadata_consistent"] = json!(stable);
    state["state"] = json!("completed");
    state["ok"] = json!(true);
    save(&path, &state)?;
    Ok(state)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probes_are_fixed_inert_and_do_not_accept_arbitrary_payloads() {
        assert!(probes("arbitrary").is_err());
        let inputs = probes("inert_html").unwrap();
        assert_eq!(inputs[0], "<blue-probe>BLUE_BASE_MARKER</blue-probe>");
        assert_eq!(inputs[0], inputs[2]);
        assert_ne!(inputs[0], inputs[1]);
        assert_eq!(
            encoded_probe(&inputs[0]),
            "&lt;blue-probe&gt;BLUE_BASE_MARKER&lt;/blue-probe&gt;"
        );
        assert_eq!(probes("literal_marker").unwrap()[0], MARKERS[0]);
    }
    #[test]
    fn local_targets_are_narrow_and_scope_cannot_be_bypassed() {
        for raw in [
            "https://example.test/preview/reflected-xss/fixed",
            "http://localhost:8000/preview/reflected-xss/fixed",
            "http://127.0.0.1:8000/other",
            "http://127.0.0.1:8000/preview/reflected-xss/fixed?q=x",
            "http://user@127.0.0.1:8000/preview/reflected-xss/fixed",
        ] {
            assert!(target(raw).is_err());
        }
        let root = tempfile::tempdir().unwrap();
        let mut tools =
            crate::tools::ToolBox::new(root.path().into(), crate::tools::ApprovalMode::ReadOnly);
        tools
            .scope
            .command("allow http://127.0.0.1:8000/ read")
            .unwrap();
        assert!(run(&tools,&root.path().join("runs"),&json!({"action":"run","run_id":"test","url":"http://127.0.0.1:8000/preview/reflected-xss/fixed"})).is_err());
        assert!(!root.path().join("runs").exists());
    }
    #[test]
    fn interrupted_and_corrupt_reservations_never_replay() {
        let root = tempfile::tempdir().unwrap();
        let tools =
            crate::tools::ToolBox::new(root.path().into(), crate::tools::ApprovalMode::ReadOnly);
        fs::write(
            root.path().join("old.json"),
            br#"{"state":"request_outcome_unknown","requests_attempted":1}"#,
        )
        .unwrap();
        let result = run(&tools, root.path(), &json!({"action":"run","run_id":"old"})).unwrap();
        assert_eq!(result["state"], "request_outcome_unknown");
        assert_eq!(result["network_requests_this_call"], 0);
        assert_eq!(result["fresh_evidence"], false);
        fs::write(root.path().join("bad.json"), b"").unwrap();
        assert!(run(&tools, root.path(), &json!({"action":"run","run_id":"bad"})).is_err());
    }
}
