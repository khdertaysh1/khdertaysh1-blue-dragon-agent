//! Exact native metadata checks. These never prove prose claims or exploitability.
use super::Observation;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NativeFacts {
    pub command_exit_code: Option<i64>,
    pub http_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_status_ready: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub evidence_id: String,
    pub kind: Kind,
    pub value: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    CommandExitCode,
    HttpStatus,
}

/// Shared provenance policy for reports, coverage and native assertions.
/// Presence of a source evidence ID denotes reuse even when its value is null.
pub fn is_historical(result: &Value) -> bool {
    result["cached"] == true
        || result["cached_result"] == true
        || result["reused_local_copy"] == true
        || result["fresh_evidence"] == false
        || result.get("reused_evidence_id").is_some()
        || result.get("original_evidence_id").is_some()
}

pub fn unverified_execution(result: &Value) -> bool {
    result["denied"] == true
        || matches!(
            result["execution_status"].as_str(),
            Some("unknown" | "not_executed" | "executed_state_mismatch")
        )
        || result["repetition_blocked"] == true
}

#[cfg(test)]
#[test]
fn skipped_actions_cannot_support_observations_or_native_facts() {
    for status in ["not_executed", "unknown", "executed_state_mismatch"] {
        let result = serde_json::json!({"execution_status":status,"exit_code":0});
        assert!(!super::observe(1, "run_command", &result).ok);
        assert!(extract("run_command", &result).command_exit_code.is_none());
    }
}

pub fn extract(tool: &str, result: &Value) -> NativeFacts {
    let mut facts = NativeFacts::default();
    // Unknown execution and denied/cached results never establish a new observation.
    if unverified_execution(result) || is_historical(result) {
        return facts;
    }
    match tool {
        "browser" if result["observation_kind"] == "browser_status" && result["ok"] != false => {
            facts.browser_status_ready = result["ready"].as_bool();
        }
        "run_command" => facts.command_exit_code = result["exit_code"].as_i64(),
        "web_fetch" | "source_artifact" => {
            facts.http_status = result["status"]
                .as_u64()
                .filter(|v| (100..=599).contains(v))
                .map(|v| v as u16);
        }
        _ => {}
    }
    facts
}

pub fn validate(
    assertions: &[Assertion],
    citations: &[String],
    observations: &[Observation],
) -> Result<()> {
    if assertions.len() > 12 {
        bail!("At most 12 native assertions are allowed");
    }
    for assertion in assertions {
        if !citations.contains(&assertion.evidence_id) {
            bail!(
                "Assertion evidence must also be cited: {}",
                assertion.evidence_id
            );
        }
        let observation = observations
            .iter()
            .find(|o| o.id == assertion.evidence_id)
            .ok_or_else(|| {
                anyhow::anyhow!("Unknown assertion evidence: {}", assertion.evidence_id)
            })?;
        let actual = match assertion.kind {
            Kind::CommandExitCode => observation.native_facts.command_exit_code,
            Kind::HttpStatus => observation.native_facts.http_status.map(i64::from),
        };
        if actual != Some(assertion.value) {
            bail!(
                "Native assertion does not match recorded metadata for {}",
                assertion.evidence_id
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn assertion(kind: Kind, value: i64) -> Assertion {
        Assertion {
            evidence_id: "E000001".into(),
            kind,
            value,
        }
    }
    #[test]
    fn exact_native_metadata_not_stdout_or_success_flag() {
        let e = super::super::observe(
            1,
            "run_command",
            &json!({"ok":true,"exit_code":7,"stdout":"all tests passed exit_code=0"}),
        );
        let ids = vec![e.id.clone()];
        assert!(validate(
            &[assertion(Kind::CommandExitCode, 0)],
            &ids,
            std::slice::from_ref(&e)
        )
        .is_err());
        assert!(validate(
            &[assertion(Kind::CommandExitCode, 7)],
            &ids,
            std::slice::from_ref(&e)
        )
        .is_ok());
        assert!(validate(
            &[assertion(Kind::HttpStatus, 200)],
            &ids,
            std::slice::from_ref(&e)
        )
        .is_err());
    }
    #[test]
    fn statuses_include_failed_requests_without_claiming_success() {
        let e = super::super::observe(
            1,
            "web_fetch",
            &json!({"ok":false,"status":403,"text":"status 200"}),
        );
        assert!(validate(
            &[assertion(Kind::HttpStatus, 403)],
            std::slice::from_ref(&e.id),
            std::slice::from_ref(&e)
        )
        .is_ok());
    }
    #[test]
    fn missing_unknown_denied_and_wrong_tool_fail_closed() {
        for (tool, result) in [
            ("read_file", json!({"exit_code":0})),
            ("run_command", json!({"ok":true})),
            (
                "run_command",
                json!({"exit_code":0,"execution_status":"unknown"}),
            ),
            ("run_command", json!({"exit_code":0,"denied":true})),
        ] {
            let e = super::super::observe(1, tool, &result);
            assert!(validate(
                &[assertion(Kind::CommandExitCode, 0)],
                std::slice::from_ref(&e.id),
                std::slice::from_ref(&e)
            )
            .is_err());
        }
    }
    #[test]
    fn uncited_evidence_rejected_and_site_origin_allowed() {
        let mut e = super::super::observe(1, "run_command", &json!({"exit_code":0}));
        assert!(validate(
            &[assertion(Kind::CommandExitCode, 0)],
            &[],
            std::slice::from_ref(&e)
        )
        .is_err());
        e.origin = Some("https://example.test".into());
        assert!(validate(
            &[assertion(Kind::CommandExitCode, 0)],
            std::slice::from_ref(&e.id),
            std::slice::from_ref(&e)
        )
        .is_ok());
    }
    #[test]
    fn old_observations_deserialize_without_inventing_facts() {
        let e: Observation = serde_json::from_value(json!({"id":"E000001","message_index":1,"tool":"run_command","ok":true,"excerpt":"exit_code 0"})).unwrap();
        assert!(validate(
            &[assertion(Kind::CommandExitCode, 0)],
            std::slice::from_ref(&e.id),
            std::slice::from_ref(&e)
        )
        .is_err());
    }
}
