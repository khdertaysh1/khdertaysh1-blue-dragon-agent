//! Exact native-result acceptance checks; model prose never satisfies them.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Criterion {
    pub label: String,
    pub tool: String,
    pub arguments: Value,
    pub field: String,
    pub equals: Value,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Contract {
    pub checks: Vec<Criterion>,
    pub observations: Vec<Option<Proof>>,
    pub declared_at: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proof {
    pub evidence_id: String,
    pub message_index: usize,
    pub workspace_fingerprint: String,
    pub coverage_complete: bool,
}
impl Contract {
    pub fn declare(&mut self, args: &Value, index: usize) -> Result<Value> {
        let checks: Vec<Criterion> = serde_json::from_value(args["checks"].clone())?;
        if checks.len() > 6 {
            bail!("At most six acceptance checks");
        }
        for c in &checks {
            let valid = match c.tool.as_str() {
                "run_command" => {
                    c.field == "/exit_code"
                        && c.equals == 0
                        && c.arguments["program"].is_string()
                        && c.arguments["args"]
                            .as_array()
                            .is_some_and(|a| a.iter().all(Value::is_string))
                }
                "file_info" => {
                    c.arguments["path"].is_string()
                        && match c.field.as_str() {
                            "/directory" | "/readonly" => c.equals.is_boolean(),
                            "/bytes" => c.equals.as_u64().is_some(),
                            _ => false,
                        }
                }
                _ => false,
            };
            if c.label.trim().is_empty()
                || c.label.len() > 500
                || c.arguments.to_string().len() > 4000
                || !valid
            {
                bail!("Check must bind exact built-in arguments and allowlisted native metadata. stdout, fetched text, memory and model assertions cannot prove a check.");
            }
        }
        self.observations = vec![None; checks.len()];
        self.checks = checks;
        self.declared_at = index;
        Ok(
            json!({"ok":true,"checks":self.checks,"note":"Contract declaration grants no execution permission and is not proof of completion."}),
        )
    }
    pub fn observe(&mut self, name: &str, args: &Value, result: &Value, proof: Proof) {
        if proof.message_index > self.declared_at
            && name == "run_command"
            && result["execution_status"] == "completed"
            && result["command_outcome"] == "nonzero_exit"
            && result["exit_code"].as_i64().is_some_and(|code| code != 0)
            && !crate::reasoning::claims::is_historical(result)
            && !crate::reasoning::claims::unverified_execution(result)
        {
            for (check, slot) in self.checks.iter().zip(&mut self.observations) {
                if check.tool == name && check.arguments == *args {
                    *slot = None;
                }
            }
        }
        if proof.message_index <= self.declared_at
            || result["ok"] == false
            || result["denied"] == true
            || result["cached_result"] == true
            || result["fresh_evidence"] == false
            || result
                .get("execution_status")
                .is_some_and(|s| !matches!(s.as_str(), Some("completed" | "succeeded" | "success")))
            || result["repetition_blocked"] == true
        {
            return;
        }
        for (n, c) in self.checks.iter().enumerate() {
            if c.tool == name && c.arguments == *args && result.pointer(&c.field) == Some(&c.equals)
            {
                self.observations[n] = Some(proof.clone());
            }
        }
    }
    pub fn invalidate(&mut self) {
        self.observations.fill(None);
    }
    pub fn status(&self, fingerprint: Option<&str>) -> Value {
        let rows=self.checks.iter().enumerate().map(|(i,c)|{
   let proof=self.observations.get(i).and_then(Option::as_ref);
   let observed=proof.is_some_and(|p|fingerprint==Some(p.workspace_fingerprint.as_str()));
   json!({"criterion":c,"observed":observed,"evidence":proof,"freshness":if observed && proof.is_some_and(|p|p.coverage_complete){"matches_bounded_workspace_snapshot"}else if observed{"snapshot_coverage_limited"}else{"missing_or_stale"}})
  }).collect::<Vec<_>>();
        let satisfied = !rows.is_empty() && rows.iter().all(|r| r["observed"] == true);
        json!({"declaration_state":if rows.is_empty(){"not_declared"}else{"declared"},"checks":rows,"all_declared_checks_observed":satisfied,"semantic_task_correctness_proven":false,"note":"Exact tool/input/native-result checks with bounded workspace freshness. Model-authored criteria can still be insufficient; no prose, historical memory or stdout banner is treated as task success."})
    }
    pub fn needs_review(&self, fingerprint: Option<&str>, _changed: bool) -> bool {
        if self.checks.is_empty() {
            false
        } else {
            self.status(fingerprint)["all_declared_checks_observed"] != true
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_or_historical_native_fields_never_satisfy_acceptance() {
        for status in [
            "failed",
            "denied",
            "not_executed",
            "cancelled",
            "unknown",
            "stale",
        ] {
            let mut c = Contract::default();
            c.declare(&json!({"checks":[{"label":"test","tool":"run_command","arguments":{"program":"fixture","args":[]},"field":"/exit_code","equals":0}]}),0).unwrap();
            c.observe(
                "run_command",
                &json!({"program":"fixture","args":[]}),
                &json!({"exit_code":0,"execution_status":status}),
                Proof {
                    evidence_id: "E1".into(),
                    message_index: 1,
                    workspace_fingerprint: "current".into(),
                    coverage_complete: true,
                },
            );
            assert!(c.needs_review(Some("current"), false), "{status}");
        }
    }
    #[test]
    fn prose_wrong_arguments_and_predeclaration_evidence_do_not_satisfy_check() {
        let mut c = Contract::default();
        let args = json!({"checks":[{"label":"Tests","tool":"run_command","arguments":{"program":"cargo","args":["test"]},"field":"/exit_code","equals":0}]});
        c.declare(&args, 5).unwrap();
        c.observe(
            "run_command",
            &json!({"program":"echo","args":["passed"]}),
            &json!({"ok":true,"exit_code":0}),
            Proof {
                evidence_id: "E6".into(),
                message_index: 6,
                workspace_fingerprint: "snap".into(),
                coverage_complete: true,
            },
        );
        assert!(c.needs_review(Some("snap"), false));
        c.observe(
            "run_command",
            &json!({"program":"cargo","args":["test"]}),
            &json!({"ok":true,"stdout":"passed"}),
            Proof {
                evidence_id: "E7".into(),
                message_index: 7,
                workspace_fingerprint: "snap".into(),
                coverage_complete: true,
            },
        );
        assert!(c.needs_review(Some("snap"), false));
        c.observe(
            "run_command",
            &json!({"program":"cargo","args":["test"]}),
            &json!({"ok":true,"exit_code":0}),
            Proof {
                evidence_id: "E4".into(),
                message_index: 4,
                workspace_fingerprint: "snap".into(),
                coverage_complete: true,
            },
        );
        assert!(c.needs_review(Some("snap"), false));
        c.observe(
            "run_command",
            &json!({"program":"cargo","args":["test"]}),
            &json!({"ok":true,"exit_code":0}),
            Proof {
                evidence_id: "E8".into(),
                message_index: 8,
                workspace_fingerprint: "snap".into(),
                coverage_complete: true,
            },
        );
        assert!(!c.needs_review(Some("snap"), false));
        assert!(c.needs_review(Some("edited"), false));
        c.invalidate();
        assert!(c.needs_review(Some("snap"), false));
    }
    #[test]
    fn absent_checks_are_neither_failure_nor_proof() {
        let c = Contract::default();
        assert!(!c.needs_review(Some("changed"), true));
        assert_eq!(c.status(None)["declaration_state"], "not_declared");
        assert_eq!(c.status(None)["all_declared_checks_observed"], false);
        assert_eq!(c.status(None)["semantic_task_correctness_proven"], false);
    }
    #[test]
    fn untrusted_text_cannot_be_a_native_check() {
        let mut c = Contract::default();
        assert!(c.declare(&json!({"checks":[{"label":"Fake","tool":"run_command","arguments":{"program":"echo","args":[]},"field":"/stdout","equals":"passed"}]}),0).is_err());
    }
}
