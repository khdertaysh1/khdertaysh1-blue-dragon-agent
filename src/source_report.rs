//! Redacted source coverage from verified recorded discovery results, not live crawling.
use serde_json::{json, Value};
use std::collections::BTreeSet;
fn hash(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
}
pub fn report(session: &crate::session::Session) -> Value {
    let mut roots = Vec::new();
    let mut seen = BTreeSet::new();
    let mut rejected = 0;
    let mut skipped = 0;
    let mut bytes = 0;
    for observation in session.observations.iter().rev().take(100) {
        if observation.tool != "source_artifact" {
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
        if content.len() > 128000 || bytes + content.len() > 2 * 1024 * 1024 {
            skipped += 1;
            continue;
        }
        bytes += content.len();
        let Ok((tool, args, result)) = crate::audit::request_for(session, &observation.id) else {
            rejected += 1;
            continue;
        };
        if tool != "source_artifact" || args["action"] != "discover" {
            continue;
        }
        if observation.request_sha256.is_none()
            || observation.result_sha256.is_none()
            || result["ok"] != true
            || result["denied"] == true
            || result["execution_status"] == "unknown"
            || ["cached", "cached_result", "reused_local_copy"]
                .iter()
                .any(|k| result[*k] == true)
            || result.get("original_evidence_id").is_some()
            || result.get("reused_evidence_id").is_some()
        {
            rejected += 1;
            continue;
        }
        let Some(root) = hash(&args["artifact_id"]) else {
            rejected += 1;
            continue;
        };
        if result["root_artifact_id"] != root {
            rejected += 1;
            continue;
        }
        if seen.contains(root) {
            continue;
        }
        let Some(references) = result["references"].as_array().filter(|r| r.len() <= 80) else {
            rejected += 1;
            continue;
        };
        if roots.len() >= 10 {
            skipped += 1;
            continue;
        }
        let states = [
            "missing_local_source",
            "cache_index_match_not_revalidated",
            "outside_current_read_scope",
            "unresolved_or_unsupported_reference",
            "duplicate_reference",
        ];
        let mut counts = [0usize; 5];
        let mut rows = Vec::new();
        let mut valid = true;
        let mut indices = BTreeSet::new();
        for reference in references {
            let Some(index) = reference["reference_index"].as_u64().filter(|n| *n < 80) else {
                valid = false;
                break;
            };
            if !indices.insert(index) {
                valid = false;
                break;
            }
            let Some(state) = reference["state"]
                .as_str()
                .and_then(|s| states.iter().position(|v| *v == s))
            else {
                valid = false;
                break;
            };
            counts[state] += 1;
            if rows.len() >= 20 {
                continue;
            }
            let target = reference["target_redacted"]
                .as_str()
                .and_then(crate::audit::safe_url);
            let current_scope = target
                .as_ref()
                .map(|url| session.scope.enabled && session.scope.check(url, "read").is_ok());
            rows.push(json!({"reference_index":index,"recorded_state":states[state],"target_redacted":target,"source_byte_offset":reference["source_byte_offset"].as_u64(),"indexed_artifact_id":hash(&reference["indexed_artifact_id"]),"current_read_scope_authorized":current_scope,"automatic_fetch_allowed":false,"loaded_by_browser_verified":false,"next_step":if state==0 || state==1 {"Re-run discovery on the root, then explicitly select and approve one reference if still needed."} else {"Reinspect the source and current scope; do not infer missing content or widen scope automatically."}}));
        }
        if !valid {
            rejected += 1;
            continue;
        }
        seen.insert(root.to_owned());
        roots.push(json!({"root_artifact_id":root,"evidence_id":observation.id,"message_index":observation.message_index,"basis":"verified_recorded_local_analysis","current_cache_revalidated":false,"collector_limited":result["limited"].as_bool(),"uncollected_reference_count":result["uncollected_reference_count"].as_u64(),"counts_recomputed_from_recorded_references":{"missing":counts[0],"index_matches":counts[1],"outside_scope":counts[2],"unresolved":counts[3],"duplicates":counts[4]},"references":rows,"reference_rows_omitted":references.len().saturating_sub(20),"reinspection":{"tool":"source_artifact","arguments":{"action":"discover","artifact_id":root},"network_requests":0},"response_reference":{"message_index":observation.message_index,"field":"content"},"runtime_coverage_complete":false}));
    }
    json!({"roots":roots,"rejected_evidence":rejected,"skipped_by_budget":skipped,"older_observations_omitted":session.observations.len().saturating_sub(100),"bounds":{"observations":100,"roots":10,"reference_rows_per_root":20,"result_bytes":128000,"total_result_bytes":2097152},"network_requests":0,"actions_replayed":0,"fresh_evidence":false,"limits":"Latest verified discovery per root within the bounded observation window, not current cache or site verification. Intrinsically historical local analysis is permitted; reused/cached envelopes and missing digests are excluded. Counts are recalculated from recorded reference states, not live requests. Query values/raw source text/model prose are omitted. Paths may still need manual redaction. Reinspect before an explicitly approved single-reference fetch. No browser loading, execution, security coverage or authenticity against whole-checkpoint rewriting is established."})
}
/// A redacted index of historical call investigations; never semantic findings.
pub fn investigations(session: &crate::session::Session) -> Value {
    let mut rows = Vec::new();
    let mut rejected = 0usize;
    let mut skipped = 0usize;
    let mut bytes = 0usize;
    for observation in session.observations.iter().rev().take(100) {
        if observation.tool != "source_artifact" {
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
        if content.len() > 128000 || bytes + content.len() > 2097152 {
            skipped += 1;
            continue;
        }
        bytes += content.len();
        let Ok((tool, args, result)) = crate::audit::request_for(session, &observation.id) else {
            rejected += 1;
            continue;
        };
        if tool != "source_artifact" || args["action"] != "bridge_call" {
            continue;
        }
        if observation.request_sha256.is_none()
            || observation.result_sha256.is_none()
            || result["ok"] != true
            || result["denied"] == true
            || result["execution_status"] == "unknown"
            || ["cached", "cached_result", "reused_local_copy"]
                .iter()
                .any(|key| result[*key] == true)
            || result.get("original_evidence_id").is_some()
            || result.get("reused_evidence_id").is_some()
        {
            rejected += 1;
            continue;
        }
        let Some(caller) = hash(&args["artifact_id"]) else {
            rejected += 1;
            continue;
        };
        let Some(offset) = args["offset"].as_u64() else {
            rejected += 1;
            continue;
        };
        let Some(mapped) = result["mapping_available"].as_bool() else {
            rejected += 1;
            continue;
        };
        if result["call_byte_offset"] != offset
            || result[if mapped {
                "caller_artifact_id"
            } else {
                "artifact_id"
            }] != caller
        {
            rejected += 1;
            continue;
        }
        if rows.len() >= 20 {
            skipped += 1;
            continue;
        }
        let callee = if mapped {
            hash(&result["callee_artifact_id"])
        } else {
            None
        };
        if mapped && callee.is_none() {
            rejected += 1;
            continue;
        }
        // A missing limit flag is unknown, not a complete analysis.
        let complete = mapped
            && result["import_chain"]["limited"] == false
            && result["callee_flow"]["limited"] == false;
        let reason = result["reason"].as_str().filter(|reason| {
            matches!(
                *reason,
                "non_module_caller_binding_scope_not_supported"
                    | "caller_analysis_incomplete"
                    | "possible_local_shadow_or_reassignment"
                    | "import_chain_incomplete_or_unresolved"
                    | "possible_import_chain_binding_write"
                    | "no_unique_named_cross_file_definition"
                    | "definition_not_direct_named_module_export"
                    | "unsupported_arguments_parameters_or_arity"
                    | "duplicate_parameter_name"
            )
        });
        rows.push(json!({"evidence_id":observation.id,"caller_artifact_id":caller,"callee_artifact_id":callee,
            "call_byte_offset":offset,"status":if !mapped {"blocked"} else if complete {"syntactic_candidate"} else {"limited_or_unknown_candidate"},
            "blocked_reason":reason,"recorded_chain_limited":result["import_chain"]["limited"].as_bool(),
            "recorded_callee_flow_limited":result["callee_flow"]["limited"].as_bool(),
            "request_result_pair_verified":true,"current_cache_revalidated":false,"semantic_dataflow_verified":false,
            "response_reference":{"message_index":observation.message_index,"field":"content"},
            "next_step":"Review the paired local request/result and exact source ranges. Reinspect cached sources explicitly; no action is replayed by report export."}));
    }
    json!({"rows":rows,"rejected_evidence":rejected,"skipped_by_budget":skipped,
        "older_observations_omitted":session.observations.len().saturating_sub(100),
        "network_requests":0,"actions_replayed":0,"fresh_evidence":false,
        "bounds":{"observations":100,"rows":20,"result_bytes":128000,"total_result_bytes":2097152},
        "limits":"Verified recorded local syntax investigations only. No current source, scope, runtime dispatch, semantic flow, vulnerability or protection verification. Source symbols, snippets and model prose are omitted. Missing limitation flags remain unknown. Rejected evidence can include other source operations in the bounded window; hashes establish consistency, not authenticity against whole-checkpoint rewriting."})
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> crate::session::Session {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "test");
        s.scope.command("allow https://example.test/ read").unwrap();
        let id = "a".repeat(64);
        let args = json!({"action":"discover","artifact_id":id});
        s.messages.push(json!({"role":"assistant","tool_calls":[{"id":"c","function":{"name":"source_artifact","arguments":args.to_string()}}]}));
        let result = json!({"ok":true,"root_artifact_id":id,"fresh_evidence":false,"limited":true,"counts":{"unique_missing_in_scope":999},"references":[{"reference_index":0,"state":"missing_local_source","target_redacted":"https://example.test/x.js?token=PRIVATE","source_byte_offset":5}]});
        let mut observed = crate::reasoning::observe(1, "source_artifact", &result);
        observed.request_sha256 =
            Some(crate::intelligence::code::hash(args.to_string().as_bytes()));
        s.observations.push(observed);
        s.messages
            .push(json!({"role":"tool","tool_call_id":"c","content":result.to_string()}));
        s
    }
    fn bridge_fixture(result: Value) -> crate::session::Session {
        let mut s = fixture();
        let args = json!({"action":"bridge_call","artifact_id":"a".repeat(64),"query":"PRIVATE_SYMBOL","offset":7});
        s.messages[0]["tool_calls"][0]["function"]["arguments"] = json!(args.to_string());
        s.messages[1]["content"] = json!(result.to_string());
        s.observations[0] = crate::reasoning::observe(1, "source_artifact", &result);
        s.observations[0].request_sha256 =
            Some(crate::intelligence::code::hash(args.to_string().as_bytes()));
        s
    }
    #[test]
    fn investigation_report_distinguishes_unknown_and_rejects_modified_or_reused() {
        let result = json!({"ok":true,"mapping_available":true,"caller_artifact_id":"a".repeat(64),"callee_artifact_id":"b".repeat(64),"call_byte_offset":7,"fresh_evidence":false,"import_chain":{"limited":false},"callee_flow":{"limited":false,"snippet":"PRIVATE_SOURCE"}});
        let mut s = bridge_fixture(result.clone());
        let output = investigations(&s);
        assert_eq!(output["rows"][0]["status"], "syntactic_candidate");
        assert!(!output.to_string().contains("PRIVATE"));
        s.messages[1]["content"] = json!("{}");
        assert!(investigations(&s)["rows"].as_array().unwrap().is_empty());
        for key in [
            "cached",
            "cached_result",
            "reused_local_copy",
            "original_evidence_id",
            "reused_evidence_id",
        ] {
            let mut changed = result.clone();
            changed[key] = json!(true);
            assert!(investigations(&bridge_fixture(changed))["rows"]
                .as_array()
                .unwrap()
                .is_empty());
        }
        let mut missing = result.clone();
        missing["callee_flow"] = json!({});
        assert_eq!(
            investigations(&bridge_fixture(missing))["rows"][0]["status"],
            "limited_or_unknown_candidate"
        );
        let mut limited = result.clone();
        limited["callee_flow"]["limited"] = json!(true);
        assert_eq!(
            investigations(&bridge_fixture(limited))["rows"][0]["status"],
            "limited_or_unknown_candidate"
        );
        let mut s = bridge_fixture(result);
        s.observations[0].request_sha256 = None;
        assert!(investigations(&s)["rows"].as_array().unwrap().is_empty());
    }
    #[test]
    fn investigation_report_keeps_blocked_outcomes_without_private_reasons() {
        let result = json!({"ok":true,"mapping_available":false,"artifact_id":"a".repeat(64),"call_byte_offset":7,"reason":"PRIVATE_REASON"});
        let s = bridge_fixture(result);
        let output = investigations(&s);
        assert_eq!(output["rows"][0]["status"], "blocked");
        assert_eq!(output["rows"][0]["blocked_reason"], Value::Null);
        assert!(!output.to_string().contains("PRIVATE"));
    }
    #[test]
    fn report_rechecks_pairs_recomputes_counts_and_redacts() {
        let mut s = fixture();
        let output = report(&s);
        assert_eq!(output["roots"].as_array().unwrap().len(), 1);
        assert_eq!(
            output["roots"][0]["counts_recomputed_from_recorded_references"]["missing"],
            1
        );
        assert_eq!(output["roots"][0]["collector_limited"], true);
        assert!(!output.to_string().contains("PRIVATE"));
        s.scope.command("clear").unwrap();
        assert_eq!(
            report(&s)["roots"][0]["references"][0]["current_read_scope_authorized"],
            false
        );
        s.messages[1]["content"] = json!("{}");
        assert_eq!(report(&s)["roots"].as_array().unwrap().len(), 0);
        assert_eq!(report(&s)["rejected_evidence"], 1);
    }
    #[test]
    fn legacy_and_reused_results_are_not_promoted() {
        let mut s = fixture();
        s.observations[0].request_sha256 = None;
        assert_eq!(report(&s)["roots"].as_array().unwrap().len(), 0);
        for flag in ["cached", "cached_result", "reused_local_copy"] {
            let mut s = fixture();
            let mut result: Value =
                serde_json::from_str(s.messages[1]["content"].as_str().unwrap()).unwrap();
            result[flag] = json!(true);
            s.messages[1]["content"] = json!(result.to_string());
            s.observations[0].result_sha256 = Some(crate::reasoning::result_hash(&result));
            assert_eq!(report(&s)["roots"].as_array().unwrap().len(), 0);
        }
    }
}
