//! Bounded recovery overview. Reads state only; never restarts work.
use serde_json::{json, Value};
use std::collections::BTreeSet;
pub fn definition() -> Value {
    json!({"type":"function","function":{"name":"task_continuity","description":"Read bounded unresolved transcript outcomes, historical job references and current runtime job states. Does not replay actions, restart jobs or establish task completion. Use message indexes to recover details before proposing new work.","parameters":{"type":"object","properties":{},"additionalProperties":false}}})
}
pub fn status(session: &crate::session::Session, live: &Value) -> Value {
    let start = session.messages.len().saturating_sub(300);
    let mut skipped_large = 0;
    let mut unknown = Vec::new();
    let mut unknown_seen: usize = 0;
    for (index, message) in session.messages.iter().enumerate().skip(start).rev() {
        if message["role"] != "tool" {
            continue;
        }
        let Some(content) = message["content"].as_str() else {
            continue;
        };
        if content.len() > 64_000 {
            skipped_large += 1;
            continue;
        }
        if serde_json::from_str::<Value>(content)
            .is_ok_and(|value| value["execution_status"] == "unknown")
        {
            unknown_seen += 1;
            if unknown.len() < 20 {
                unknown.push(json!({"message_index":index,"state":"unknown","automatic_retry_allowed":false}));
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut jobs = Vec::new();
    for observation in session.observations.iter().rev().take(300) {
        if !matches!(
            observation.tool.as_str(),
            "start_job" | "job_status" | "job_output" | "cancel_job"
        ) {
            continue;
        }
        let Ok((_, _, result)) = crate::audit::request_for(session, &observation.id) else {
            continue;
        };
        if crate::reasoning::claims::is_historical(&result) || result["ok"] == false {
            continue;
        }
        let Some(id) = result["job_id"]
            .as_str()
            .filter(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        else {
            continue;
        };
        if !seen.insert(id.to_owned()) {
            continue;
        }
        if jobs.len() >= 20 {
            break;
        }
        let current = live["jobs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|job| job["job_id"] == id);
        jobs.push(json!({"job_id":id,"historical_evidence_id":observation.id,"historical_message_index":observation.message_index,
            "current_runtime":current,"runtime_status":if current.is_some(){"present_in_current_runtime"}else{"not_present_in_current_runtime"},
            "previous_outcome_requires_inspection":current.is_none(),"automatic_restart_allowed":false}));
    }
    let research = crate::test_coverage::report(session);
    let cells=research["cases"].as_array().into_iter().flatten().take(20).map(|r|json!({"case_id":r["case_id"],"state":r["state"],"scenario":r["scenario"],"current_scope_and_label_match":r["current_scope_and_label_match"],"automatic_retry_allowed":false})).collect::<Vec<_>>();
    json!({"ok":true,"fresh_evidence":false,"network_requests":0,"actions_replayed":0,"research_cases":cells,"research_cases_omitted":research["cases"].as_array().map_or(0,|r|r.len().saturating_sub(20)),"research_resume":"Revalidated local session cells; inspect test_coverage report and original evidence before new work. No automatic retries.",
        "unknown_outcomes":unknown,"unknown_rows_omitted":unknown_seen.saturating_sub(unknown.len()),"historical_jobs":jobs,"current_runtime_jobs":live["jobs"],
        "open_plan_steps":session.plan.steps.iter().filter(|step|step.status!="completed").count(),
        "plan_status_is_model_reported":true,"prior_run_returned_normally":session.last_report["turn_returned_normally"].as_bool().or_else(|| session.last_report["finished"].as_bool()),
        "task_completion_proven":false,
        "bounds":{"message_window":300,"observation_window":300,"unknown_rows":20,"job_rows":20,"older_messages_omitted":start,"large_tool_messages_skipped":skipped_large},
        "next_step":"Read the referenced original requests/results and inspect current workspace/browser state before proposing retries. A missing live job is not evidence that its previous command succeeded or had no effects.",
        "limits":"Historical tool messages may be incomplete. A normal run return or completed plan label is not task proof. Background jobs are process-local and are never recreated by this read. Browser login/page state is not restored."})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovered_history_never_becomes_a_live_job_or_replayed_action() {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "fixture");
        let id = "0123456789abcdef0123456789abcdef";
        let result = json!({"ok":true,"job_id":id,"state":"running"});
        s.messages.push(json!({"role":"assistant","tool_calls":[{"id":"start","function":{"name":"start_job","arguments":"{\"program\":\"PRIVATE_COMMAND\"}"}}]}));
        s.observations
            .push(crate::reasoning::observe(1, "start_job", &result));
        s.messages
            .push(json!({"role":"tool","tool_call_id":"start","content":result.to_string()}));
        s.messages.push(json!({"role":"assistant","tool_calls":[{"id":"lost","function":{"name":"run_command","arguments":"{}"}}]}));
        assert_eq!(crate::session::recover_tool_history(&mut s), 1);
        let report = status(&s, &crate::jobs::Jobs::default().list());
        assert_eq!(report["unknown_outcomes"][0]["message_index"], 3);
        assert_eq!(
            report["historical_jobs"][0]["runtime_status"],
            "not_present_in_current_runtime"
        );
        assert_eq!(report["actions_replayed"], 0);
        assert_eq!(report["task_completion_proven"], false);
        assert!(!report.to_string().contains("PRIVATE_COMMAND"));
        let live = json!({"jobs":[{"job_id":id,"state":"exited","exit_code":0}]});
        assert_eq!(
            status(&s, &live)["historical_jobs"][0]["current_runtime"]["exit_code"],
            0
        );
    }
    #[test]
    fn continuity_window_and_output_are_bounded_and_omissions_visible() {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "fixture");
        for _ in 0..350 {
            s.messages
                .push(json!({"role":"tool","content":"{\"execution_status\":\"unknown\"}"}));
        }
        let report = status(&s, &json!({"jobs":[]}));
        assert_eq!(report["unknown_outcomes"].as_array().unwrap().len(), 20);
        assert_eq!(report["unknown_rows_omitted"], 280);
        assert_eq!(report["bounds"]["older_messages_omitted"], 50);
    }
    #[test]
    fn user_prose_cannot_create_unknown_execution_records() {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "fixture");
        s.messages
            .push(json!({"role":"user","content":"{\"execution_status\":\"unknown\"}"}));
        assert!(status(&s, &json!({"jobs":[]}))["unknown_outcomes"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}
