//! Independent task control and evidence, not model weights or hidden reasoning.
pub mod context;
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
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub status: String,
    pub summary: String,
    pub evidence_ids: Vec<String>,
    pub limitation: String,
}
pub fn finding(args: &Value, observations: &[Observation]) -> Result<Finding> {
    let item: Finding = serde_json::from_value(args.clone())?;
    if !matches!(item.status.as_str(), "hypothesis" | "supported" | "blocked")
        || item.summary.trim().is_empty()
        || item.summary.len() > 2000
        || item.limitation.len() > 2000
        || item.evidence_ids.len() > 12
    {
        bail!("Invalid finding: use hypothesis/supported/blocked, bounded summary/limitation and at most 12 evidence IDs");
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
    Ok(item)
}
pub fn observe(index: usize, tool: &str, result: &Value) -> Observation {
    Observation {
        id: format!("E{index:06}"),
        message_index: index,
        tool: tool.into(),
        ok: result["ok"] != false
            && result["denied"] != true
            && result["execution_status"] != "unknown",
        excerpt: context::clip(&result.to_string(), 1000),
        origin: None,
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
        json!({"type":"function","function":{"name":"record_finding","description":"Save an evidence-linked finding. Status hypothesis/supported/blocked. Supported requires real successful evidence IDs but is still a model interpretation, not an engine guarantee of exploitability or correctness. Include uncertainty and failed checks in limitation.","parameters":{"type":"object","properties":{"status":{"type":"string","enum":["hypothesis","supported","blocked"]},"summary":{"type":"string"},"evidence_ids":{"type":"array","items":{"type":"string"}},"limitation":{"type":"string"}},"required":["status","summary","evidence_ids","limitation"],"additionalProperties":false}}}),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
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
