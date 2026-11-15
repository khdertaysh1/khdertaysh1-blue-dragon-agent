//! Canonical local-lab recipes and rechecked observations, never exploit proof.
use serde_json::{json, Value};
fn digest(v: &Value) -> Option<&str> {
    v.as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn verdict(scenario: &str, rows: &[Value]) -> &'static str {
    if rows.len() != 3
        || rows.iter().any(|r| {
            r["complete"] != true
                || !r["status"]
                    .as_u64()
                    .is_some_and(|s| (200..300).contains(&s))
        })
    {
        return "inconclusive_incomplete_observations";
    }
    if ["status", "content_type_sha256", "body_sha256"]
        .iter()
        .any(|k| rows[0][*k] != rows[2][*k])
    {
        return "inconclusive_control_changed";
    }
    let differential = rows[0]["baseline_marker_seen"] == true
        && rows[0]["variant_marker_seen"] == false
        && rows[1]["baseline_marker_seen"] == false
        && rows[1]["variant_marker_seen"] == true
        && rows[2]["baseline_marker_seen"] == true
        && rows[2]["variant_marker_seen"] == false;
    if !differential {
        return "inconclusive_marker_not_differential";
    }
    if scenario == "literal_marker" {
        return "differential_literal_marker_only";
    }
    if rows
        .iter()
        .all(|r| r["raw_probe_sequence_seen"] == true && r["escaped_probe_sequence_seen"] == false)
    {
        "raw_probe_sequence_observed"
    } else if rows
        .iter()
        .all(|r| r["escaped_probe_sequence_seen"] == true && r["raw_probe_sequence_seen"] == false)
    {
        "escaped_probe_sequence_observed"
    } else {
        "inconclusive_mixed_or_missing_encoding"
    }
}
fn checked_rows(result: &Value) -> Option<Vec<Value>> {
    let observations = result["observations"].as_array()?;
    if observations.len() > 3 {
        return None;
    }
    let mut rows = Vec::new();
    for (step, r) in observations.iter().enumerate() {
        if r["step"].as_u64() != Some(step as u64) {
            return None;
        }
        let status = r["status"].as_u64().filter(|s| (100..600).contains(s))?;
        let bytes = r["bytes_captured"].as_u64().filter(|b| *b <= 65536)?;
        let body = digest(&r["body_sha256"])?;
        let content_type = digest(&r["content_type_sha256"])?;
        let keys = [
            "complete",
            "raw_probe_sequence_seen",
            "escaped_probe_sequence_seen",
            "baseline_marker_seen",
            "variant_marker_seen",
        ];
        if keys.iter().any(|k| r[*k].as_bool().is_none()) {
            return None;
        }
        let mut row = json!({"step":step,"status":status,"bytes_captured":bytes,"body_sha256":body,"content_type_sha256":content_type});
        for key in keys {
            row[key] = r[key].clone();
        }
        rows.push(row);
    }
    Some(rows)
}
pub fn report(session: &crate::session::Session) -> Value {
    let mut runs = Vec::new();
    let mut rejected = 0usize;
    let mut skipped = 0usize;
    let mut bytes = 0usize;
    let mut seen = std::collections::BTreeSet::new();
    for observation in session.observations.iter().rev().take(100) {
        if observation.tool != "run_local_lab" {
            continue;
        }
        let Some(content) = session
            .messages
            .get(observation.message_index)
            .and_then(|m| m["content"].as_str())
        else {
            rejected += 1;
            continue;
        };
        if content.len() > 32768 || bytes + content.len() > 655360 {
            skipped += 1;
            continue;
        }
        bytes += content.len();
        let Ok((tool, args, result)) = crate::audit::request_for(session, &observation.id) else {
            rejected += 1;
            continue;
        };
        if tool != "run_local_lab" || args["action"] != "run" {
            continue;
        }
        if observation.request_sha256.is_none()
            || observation.result_sha256.is_none()
            || ["cached", "cached_result", "reused_local_copy"]
                .iter()
                .any(|k| result[*k] == true)
            || result["fresh_evidence"] == false
            || result.get("original_evidence_id").is_some()
            || result.get("reused_evidence_id").is_some()
        {
            rejected += 1;
            continue;
        }
        let Some(id) = args["run_id"].as_str().filter(|id| {
            !id.is_empty()
                && id.len() <= 48
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        }) else {
            rejected += 1;
            continue;
        };
        let Some(url) = result["target"]
            .as_str()
            .and_then(|u| crate::lab_runner::target(u).ok())
        else {
            rejected += 1;
            continue;
        };
        if args["url"]
            != json!({"origin":url.origin().ascii_serialization(),"path":url.path(),"query":{},"fragment":null})
        {
            rejected += 1;
            continue;
        }
        let scenario = args["scenario"].as_str().unwrap_or("literal_marker");
        let Ok(inputs) = crate::lab_runner::probes(scenario) else {
            rejected += 1;
            continue;
        };
        let state = result["state"].as_str().filter(|s| {
            matches!(
                *s,
                "completed"
                    | "denied"
                    | "request_outcome_unknown"
                    | "stopped_unusable_response"
                    | "stopped_before_next_request"
                    | "step_observed"
                    | "awaiting_approval"
            )
        });
        let Some(state) = state else {
            rejected += 1;
            continue;
        };
        if result["run_id"] != id
            || result["target"] != url.as_str()
            || result["recipe"]["scenario"] != scenario
        {
            rejected += 1;
            continue;
        }
        let Some(rows) = checked_rows(&result) else {
            rejected += 1;
            continue;
        };
        let Some(attempted) = result["requests_attempted"]
            .as_u64()
            .filter(|n| *n <= 3 && *n >= rows.len() as u64)
        else {
            rejected += 1;
            continue;
        };
        if (state == "completed"
            && (rows.len() != 3 || attempted != 3 || result["execution_status"] != "observed"))
            || (state == "denied" && (attempted != 0 || !rows.is_empty()))
        {
            rejected += 1;
            continue;
        }
        if seen.contains(id) {
            continue;
        }
        if runs.len() >= 20 {
            skipped += 1;
            continue;
        }
        seen.insert(id.to_owned());
        let conclusion = if state == "completed" {
            verdict(scenario, &rows)
        } else if state == "denied" {
            "not_executed_denied"
        } else {
            "inconclusive_partial_or_unknown"
        };
        runs.push(json!({"run_id":id,"evidence_id":observation.id,"target":url.as_str(),"recorded_state":state,
            "requests_attempted":attempted,"observations":rows,"recomputed_observation":conclusion,
            "recorded_verdict_matches_recomputation":result["verdict"].as_str().map(|v|v==conclusion),
            "hypothesis":"Changing only the fixed q probe may change the captured response marker or encoding; this is not an execution hypothesis.",
            "canonical_recipe":{"scenario":scenario,"method":"GET","query_key":"q","inputs":inputs,"preview_token":"00000000000000000000000000000000","sequence":["baseline","variant","repeated_control"],"request_budget":3,"cookies_sent":false,"redirects_followed":false},
            "current_interact_scope_authorized":session.scope.enabled && session.scope.check(url.as_str(),"interact").is_ok(),
            "next_step":"Review recorded conditions and server state. Any repeat requires a new explicit user request, interact scope, human approval and a fresh run ID; never replay this run automatically.",
            "response_reference":{"message_index":observation.message_index,"field":"content"},
            "automatic_replay_permitted":false,"browser_execution_verified":false,"vulnerability_confirmed":false,"protection_verified":false}));
    }
    json!({"runs":runs,"rejected_evidence":rejected,"skipped_by_budget":skipped,"older_observations_omitted":session.observations.len().saturating_sub(100),"network_requests":0,"actions_replayed":0,"fresh_evidence":false,
        "limits":"Historical original paired local-lab runs only; cached status responses are excluded. Fixed recipe is reconstructed from supported scenario, never copied from arbitrary result prose. Outcomes are recomputed from bounded typed observations, not stored verdict text. Partial or unknown attempts remain inconclusive. Response/body hashes do not prove browser parsing, execution, causality, server or session stability, protection or authenticity against whole-checkpoint rewriting."})
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> crate::session::Session {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "test");
        let args = json!({"action":"run","run_id":"test","url":"http://127.0.0.1:8765/preview/reflected-xss/fixed","scenario":"inert_html"});
        let rows=(0..3).map(|step|json!({"step":step,"status":200,"complete":true,"body_sha256":if step==1 {"b".repeat(64)}else{"a".repeat(64)},"content_type_sha256":"c".repeat(64),"bytes_captured":10,"baseline_marker_seen":step!=1,"variant_marker_seen":step==1,"raw_probe_sequence_seen":false,"escaped_probe_sequence_seen":true})).collect::<Vec<_>>();
        let result = json!({"ok":true,"run_id":"test","target":args["url"],"state":"completed","execution_status":"observed","requests_attempted":3,"observations":rows,"recipe":{"scenario":"inert_html"},"verdict":"PRIVATE_FAKE_VERDICT"});
        s.messages.push(json!({"role":"assistant","tool_calls":[{"id":"c","function":{"name":"run_local_lab","arguments":args.to_string()}}]}));
        s.messages
            .push(json!({"role":"tool","tool_call_id":"c","content":result.to_string()}));
        let mut obs = crate::reasoning::observe(1, "run_local_lab", &result);
        obs.request_sha256 = Some(crate::intelligence::code::hash(args.to_string().as_bytes()));
        s.observations.push(obs);
        s
    }
    fn replace_result(s: &mut crate::session::Session, result: Value) {
        s.messages[1]["content"] = json!(result.to_string());
        s.observations[0].result_sha256 = Some(crate::reasoning::result_hash(&result));
    }
    #[test]
    fn report_recomputes_and_redacts_without_trusting_verdict() {
        let mut s = fixture();
        let report = report(&s);
        assert_eq!(
            report["runs"][0]["recomputed_observation"],
            "escaped_probe_sequence_observed"
        );
        assert_eq!(
            report["runs"][0]["recorded_verdict_matches_recomputation"],
            false
        );
        assert!(!report.to_string().contains("PRIVATE"));
        s.messages[1]["content"] = json!("{}");
        assert!(super::report(&s)["runs"].as_array().unwrap().is_empty());
    }
    #[test]
    fn report_rejects_missing_types_and_keeps_partial_unknown() {
        let s = fixture();
        let original: Value =
            serde_json::from_str(s.messages[1]["content"].as_str().unwrap()).unwrap();
        for key in ["complete", "baseline_marker_seen", "body_sha256", "step"] {
            let mut s = fixture();
            let mut value = original.clone();
            value["observations"][0]
                .as_object_mut()
                .unwrap()
                .remove(key);
            replace_result(&mut s, value);
            assert!(report(&s)["runs"].as_array().unwrap().is_empty());
        }
        let mut s = fixture();
        let mut value = original;
        value["state"] = json!("request_outcome_unknown");
        value["execution_status"] = json!("unknown");
        value["requests_attempted"] = json!(1);
        value["observations"] = json!([]);
        replace_result(&mut s, value);
        assert_eq!(
            report(&s)["runs"][0]["recomputed_observation"],
            "inconclusive_partial_or_unknown"
        );
    }
    #[test]
    fn report_excludes_cached_status_and_rechecks_scope() {
        let mut s = fixture();
        s.scope
            .command("allow http://127.0.0.1:8765/ interact")
            .unwrap();
        assert_eq!(
            report(&s)["runs"][0]["current_interact_scope_authorized"],
            true
        );
        s.scope.command("clear").unwrap();
        assert_eq!(
            report(&s)["runs"][0]["current_interact_scope_authorized"],
            false
        );
        let mut value: Value =
            serde_json::from_str(s.messages[1]["content"].as_str().unwrap()).unwrap();
        value["reused_local_copy"] = json!(true);
        replace_result(&mut s, value);
        assert!(report(&s)["runs"].as_array().unwrap().is_empty());
    }
}
