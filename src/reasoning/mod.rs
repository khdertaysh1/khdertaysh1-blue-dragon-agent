//! Independent task control and evidence, not model weights or hidden reasoning.
pub mod checkpoint;
pub mod claims;
pub mod context;
pub mod evidence_memory;
pub mod loops;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    pub message_index: usize,
    pub tool: String,
    pub ok: bool,
    pub excerpt: String,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub native_facts: claims::NativeFacts,
    #[serde(default)]
    pub automatic_request: Option<AutomaticInspectionRequest>,
    #[serde(default)]
    pub request_sha256: Option<String>,
    #[serde(default)]
    pub result_sha256: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomaticInspectionRequest {
    pub url: String,
    pub result_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub status: String,
    pub summary: String,
    pub evidence_ids: Vec<String>,
    pub limitation: String,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub next_test: String,
    #[serde(default)]
    pub confirms: String,
    #[serde(default)]
    pub refutes: String,
    #[serde(default)]
    pub assertions: Vec<claims::Assertion>,
    #[serde(default)]
    pub assertion_verification: String,
}
pub fn finding(args: &Value, observations: &[Observation]) -> Result<Finding> {
    let mut item: Finding = serde_json::from_value(args.clone())?;
    if !matches!(item.status.as_str(), "hypothesis" | "supported" | "blocked")
        || item.summary.trim().is_empty()
        || item.summary.len() > 2000
        || item.limitation.len() > 2000
        || item.evidence_ids.len() > 12
    {
        bail!("Invalid finding: use hypothesis/supported/blocked, bounded summary/limitation and at most 12 evidence IDs");
    }
    for field in [
        &item.rationale,
        &item.next_test,
        &item.confirms,
        &item.refutes,
    ] {
        if field.len() > 2000 || (item.status == "hypothesis" && field.trim().is_empty()) {
            bail!("Hypotheses require bounded rationale, next_test, confirms and refutes fields");
        }
    }
    for id in &item.evidence_ids {
        if !observations.iter().any(|e| &e.id == id) {
            bail!("Unknown evidence ID {id}; cite real tool observations");
        }
    }
    if item.status == "supported"
        && !item
            .evidence_ids
            .iter()
            .any(|id| observations.iter().any(|e| &e.id == id && e.ok))
    {
        bail!("Supported findings need a successful observed tool result. This is a citation check, not semantic proof of the claim.");
    }
    claims::validate(&item.assertions, &item.evidence_ids, observations)?;
    item.assertion_verification = if item.assertions.is_empty() {
        "Citation checks only; summary prose is not semantically verified"
    } else {
        "Exact native metadata assertions matched; summary prose, test coverage and exploitability are not semantically verified"
    }.into();
    Ok(item)
}
pub fn result_hash(result: &Value) -> String {
    // The evidence_id is attached by the agent after the native result is
    // observed. Scalar tool results are wrapped by the same envelope step.
    let mut native = if result.is_object() {
        result.clone()
    } else {
        json!({"result":result})
    };
    native.as_object_mut().unwrap().remove("evidence_id");
    crate::intelligence::code::hash(native.to_string().as_bytes())
}
pub fn observe(index: usize, tool: &str, result: &Value) -> Observation {
    Observation {
        id: format!("E{index:06}"),
        message_index: index,
        tool: tool.into(),
        ok: !claims::is_historical(result)
            && result["ok"] != false
            && !claims::unverified_execution(result),
        excerpt: context::clip(&result.to_string(), 1000),
        origin: None,
        native_facts: claims::extract(tool, result),
        automatic_request: None,
        request_sha256: None,
        result_sha256: Some(result_hash(result)),
    }
}
pub fn briefing(task: &str) -> &'static str {
    let lower = task.to_lowercase();
    if crate::browser::inspection::bare_url(task).is_some()
        || ["xss", "vulnerability", "security", "ثغر", "افحص", "سايبر"]
            .iter()
            .any(|w| lower.contains(w))
    {
        "Task mode: security/browser review. For explanation-only questions, explain without running scans. The current user request takes precedence over old plans. Use supplied scope and observed evidence. Distinguish source/sink hints, failed checks and actual execution proofs. Inspect relevant forms/scripts; do not invent network results. Cite evidence IDs and exact reproductions. Missing evidence means unknown, not safe or vulnerable. Preserve approvals and target boundaries."
    } else if ["fix", "error", "bug", "fail", "صلح", "خطأ", "مشكلة", "زبط"]
        .iter()
        .any(|w| lower.contains(w))
    {
        "Task mode: diagnose and fix. Establish the observed failure, inspect relevant source, form a testable explanation, make the smallest justified change with approval, and verify the same failure. If an approach produces identical errors, change the hypothesis or inspect new evidence rather than repeat it."
    } else {
        "Task mode: general engineering. For a question answer directly; for multi-step work maintain a concise plan, inspect before changing, and verify relevant outcomes when permitted. Do not create work outside the request. Cite observed evidence for completed actions and state checks that were blocked or not run."
    }
}
pub fn definitions() -> Vec<Value> {
    vec![
        json!({"type":"function","function":{"name":"recall_context","description":"Read an exact bounded excerpt from this session's original transcript. Use message_index from a compacted observation/evidence ID to recover omitted details. This does not rerun a tool.","parameters":{"type":"object","properties":{"message_index":{"type":"integer","minimum":0},"offset":{"type":"integer","minimum":0},"count":{"type":"integer","minimum":1,"maximum":12000}},"required":["message_index"],"additionalProperties":false}}}),
        json!({"type":"function","function":{"name":"record_finding","description":"Save an evidence-linked finding. Status hypothesis/supported/blocked. hypothesis additionally requires rationale, next_test, confirms and refutes; a test plan is not permission to execute. Supported requires real successful evidence IDs but is still a model interpretation, not an engine guarantee of exploitability or correctness. Include uncertainty and failed checks in limitation. Optional assertions validate only exact native command exit codes or HTTP statuses against cited observations, not the meaning or truth of summary prose.","parameters":{"type":"object","properties":{"status":{"type":"string","enum":["hypothesis","supported","blocked"]},"summary":{"type":"string"},"evidence_ids":{"type":"array","items":{"type":"string"}},"limitation":{"type":"string"},"rationale":{"type":"string"},"next_test":{"type":"string"},"confirms":{"type":"string"},"refutes":{"type":"string"},"assertions":{"type":"array","maxItems":12,"items":{"type":"object","properties":{"evidence_id":{"type":"string"},"kind":{"type":"string","enum":["command_exit_code","http_status"]},"value":{"type":"integer"}},"required":["evidence_id","kind","value"],"additionalProperties":false}}},"required":["status","summary","evidence_ids","limitation"],"additionalProperties":false}}}),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hypotheses_require_falsifiable_next_steps() {
        let mut args = json!({"status":"hypothesis","summary":"Input might reach a sink","evidence_ids":[],"limitation":"Static only"});
        assert!(finding(&args, &[]).is_err());
        args["rationale"] = json!("A syntactic assignment candidate was found");
        args["next_test"] = json!("Compare normal and one changed input in an authorized fixture");
        args["confirms"] = json!("Native canary appears only in controlled variant");
        args["refutes"] = json!("Input is rendered as inert text in this test");
        assert_eq!(finding(&args, &[]).unwrap().status, "hypothesis");
    }
    #[test]
    fn structured_findings_reject_mismatch_and_generate_their_own_label() {
        let evidence = vec![observe(1, "run_command", &json!({"ok":true,"exit_code":0}))];
        let mut args = json!({"status":"supported","summary":"Observed command exit","evidence_ids":["E000001"],"limitation":"Does not prove coverage","assertions":[{"kind":"command_exit_code","value":1,"evidence_id":"E000001"}],"assertion_verification":"Everything proven"});
        assert!(finding(&args, &evidence).is_err());
        args["assertions"][0]["value"] = json!(0);
        let accepted = finding(&args, &evidence).unwrap();
        assert!(accepted
            .assertion_verification
            .contains("not semantically verified"));
        assert!(!accepted
            .assertion_verification
            .contains("Everything proven"));
        args["assertions"] = json!([]);
        assert!(finding(&args, &evidence)
            .unwrap()
            .assertion_verification
            .starts_with("Citation checks only"));
    }
    #[test]
    fn claims_cannot_cite_invented_or_failed_evidence_as_support() {
        let evidence = vec![
            observe(2, "read_file", &json!({"content":"observed"})),
            observe(3, "run_command", &json!({"ok":false,"error":"denied"})),
        ];
        let mut args = json!({"status":"supported","summary":"Claim","evidence_ids":["made-up"],"limitation":"Not semantic verification"});
        assert!(finding(&args, &evidence).is_err());
        args["evidence_ids"] = json!(["E000003"]);
        assert!(finding(&args, &evidence).is_err());
        args["evidence_ids"] = json!(["E000002"]);
        assert!(finding(&args, &evidence).is_ok());
    }
}
