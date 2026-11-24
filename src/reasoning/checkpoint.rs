//! Extractive continuity index. Never replaces original instructions or evidence.
use crate::session::Session;
use serde_json::{json, Value};
/// Capture the preceding dialogue before the new message becomes the latest goal.
/// Use only real user turn boundaries; runtime-generated user-role guidance is excluded.
pub fn prior_dialogue(session: &Session) -> Option<Value> {
    let mut turns = Vec::new();
    let mut end = session.messages.len();
    for &index in session.user_turns.iter().rev().take(4) {
        if index >= end {
            continue;
        }
        let messages = &session.messages[index..end];
        end = index;
        if messages[0]["role"] != "user" {
            continue;
        }
        let Some(request) = messages[0]["content"].as_str() else {
            continue;
        };
        let reply = messages
            .iter()
            .rev()
            .filter(|m| {
                m["role"] == "assistant"
                    && m.get("tool_calls")
                        .is_none_or(|c| c.is_null() || c.as_array().is_some_and(Vec::is_empty))
            })
            .filter_map(|m| m["content"].as_str())
            .find(|text| {
                !text.trim().is_empty() && !text.starts_with("INCOMPLETE HISTORICAL PROVIDER TEXT")
            });
        turns.push(
            json!({"user_index":index,"request":super::context::clip(request, 320),
            "historical_reply":reply.map(|text| super::context::clip(text, 200))}),
        );
    }
    if turns.is_empty() {
        return None;
    }
    turns.reverse();
    Some(
        json!({"turns":turns,"previous_status":session.last_report["task_status"],
        "stop_reason":session.last_report["error"].as_str().map(|e|super::context::clip(e,180)),
        "last_evidence":session.observations.last().map(|o| &o.id)}),
    )
}

pub fn derive(session: &Session) -> Value {
    let goal = session.user_turns.last().and_then(|i| {
        session
            .messages
            .get(*i)
            .filter(|m| m["role"] == "user")
            .map(|m| (*i, m))
    });
    let goal = goal.map(|(i,m)|json!({"message_index":i,"sha256":crate::intelligence::code::hash(m.to_string().as_bytes()),"excerpt":super::context::clip(m["content"].as_str().unwrap_or(""),700),"original_available":true}));
    let findings = session.findings.iter().enumerate().rev().take(4).map(|(i,f)|json!({"finding_index":i,"status_model_reported":f.status,"summary_excerpt":super::context::clip(&f.summary,160),"rationale_excerpt":super::context::clip(&f.rationale,120),"confirms_excerpt":super::context::clip(&f.confirms,120),"refutes_excerpt":super::context::clip(&f.refutes,120),"next_test_excerpt":super::context::clip(&f.next_test,120),"evidence_ids":f.evidence_ids})).collect::<Vec<_>>();
    let recent = session.observations.iter().rev().take(8).map(|o|json!({"evidence_id":o.id,"message_index":o.message_index,"tool":o.tool,"native_return_ok":o.ok,"request_sha256":o.request_sha256,"result_sha256":o.result_sha256})).collect::<Vec<_>>();
    let decisions=session.messages.iter().enumerate().rev().filter(|(_,m)|m["role"]=="assistant" && m["tool_calls"].as_array().is_none_or(Vec::is_empty)).take(2).map(|(i,m)|json!({"message_index":i,"historical_unverified_assistant_excerpt":super::context::clip(m["content"].as_str().unwrap_or(""),160)})).collect::<Vec<_>>();
    json!({"schema":1,"derived_untrusted":true,"not_instructions_or_fresh_evidence":true,"goal":goal,"recent_user_message_refs":session.user_turns.iter().rev().take(16).collect::<Vec<_>>(),"tracked_user_turns":session.user_turns.len(),"omitted_user_refs":session.user_turns.len().saturating_sub(16),"constraints":"Original user/system messages remain authoritative and remain exact on disk; older repeated user paragraphs may be losslessly encoded in request projection; this index is not a replacement summary.","historical_decisions":decisions,"hypotheses":findings,"total_findings":session.findings.len(),"recent_evidence_refs":recent,"total_observations":session.observations.len(),"fresh_read_requirements":session.fresh_source_requests,"failed_observation_refs":session.observations.iter().rev().filter(|o|!o.ok).take(4).map(|o|&o.id).collect::<Vec<_>>(),"next_step_model_proposed":session.plan.steps.iter().find(|s|s.status!="completed").map(|s|super::context::clip(&s.title,180)),"scope_sha256":crate::intelligence::code::hash(serde_json::to_string(&session.scope).unwrap_or_default().as_bytes()),"limitations":"Extractive index, not a semantic summary or authorization. Excerpts can be incomplete or wrong. Exact originals, all user-turn indexes and full findings remain in the session; use recall_context/findings. No provider or tool request was executed."})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derived_index_preserves_originals_and_marks_old_assistant_text() {
        let mut s = crate::session::new(std::path::Path::new("."), "fixture");
        s.messages = vec![
            json!({"role":"user","content":"لا تعد الإجراء المجهول"}),
            json!({"role":"assistant","content":"Earlier unverified conclusion"}),
        ];
        s.user_turns.push(0);
        let before = s.messages.clone();
        let value = derive(&s);
        assert_eq!(before, s.messages);
        assert_eq!(value["derived_untrusted"], true);
        assert_eq!(value["goal"]["message_index"], 0);
        assert!(value["historical_decisions"][0]
            .get("historical_unverified_assistant_excerpt")
            .is_some());
    }
}
