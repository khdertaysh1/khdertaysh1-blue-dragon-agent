//! Turn-local evidence and the bounded transition to a completion review.
use crate::intelligence::checks::Contract;
use crate::reasoning::claims;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Mission {
    pub turn_message_index: usize,
    declaration: Option<usize>,
    latest_proofs: Vec<Option<String>>,
    pub checks_met_since_step: Option<usize>,
    pub last_command_attempt: Option<Value>,
    pub latest_step: usize,
    #[serde(default)]
    completion_review_started: bool,
}

impl Mission {
    pub fn new(turn_message_index: usize) -> Self {
        Self {
            turn_message_index,
            ..Self::default()
        }
    }

    pub fn observe(
        &mut self,
        step: usize,
        contract: &Contract,
        fingerprint: Option<&str>,
        tool: &str,
        args: &Value,
        result: &Value,
    ) {
        self.latest_step = step;
        if self.declaration != Some(contract.declared_at) {
            self.declaration = Some(contract.declared_at);
            self.latest_proofs = vec![None; contract.checks.len()];
            self.checks_met_since_step = None;
        }
        let id = result["evidence_id"].as_str();
        let native = id.is_some() && !claims::is_historical(result);
        if native && tool == "run_command" {
            let exit = result["exit_code"].as_i64().filter(|_| {
                result["execution_status"] == "completed"
                    && matches!(
                        result["command_outcome"].as_str(),
                        Some("success" | "nonzero_exit")
                    )
                    && !claims::unverified_execution(result)
            });
            self.last_command_attempt = Some(json!({"step":step,"evidence_id":id,
                "outcome":result["command_outcome"],"exit_code":exit,"execution_confirmed":exit.is_some()}));
        }
        for (i, criterion) in contract.checks.iter().enumerate() {
            if native && tool == criterion.tool && *args == criterion.arguments {
                // A contradictory rerun must supersede an earlier successful proof.
                self.latest_proofs[i] = id
                    .filter(|id| {
                        contract.observations[i]
                            .as_ref()
                            .is_some_and(|p| p.evidence_id == *id)
                    })
                    .map(str::to_owned);
            }
        }
        let met = !contract.checks.is_empty()
            && self
                .latest_proofs
                .iter()
                .zip(&contract.observations)
                .all(|(id, proof)| {
                    proof.as_ref().is_some_and(|p| {
                        id.as_deref() == Some(p.evidence_id.as_str())
                            && fingerprint == Some(p.workspace_fingerprint.as_str())
                    })
                });
        if met {
            self.checks_met_since_step.get_or_insert(step);
        } else {
            self.checks_met_since_step = None;
        }
    }

    pub fn begin_completion_review(&mut self, saved_body_read: bool) -> bool {
        if self.completion_review_started
            || (self.checks_met_since_step.is_none() && !saved_body_read)
        {
            return false;
        }
        self.completion_review_started = true;
        true
    }

    pub fn report(&self) -> Value {
        json!({"mode":"completion_review","completion_review_started":self.completion_review_started,"turn_message_index":self.turn_message_index,
            "latest_tool_step":self.latest_step,"declared_criteria":self.latest_proofs.len(),
            "declared_check_state":if self.latest_proofs.is_empty() {"not_declared"} else if self.checks_met_since_step.is_some() {"observed"} else {"unverified"},
            "checks_met_since_step":self.checks_met_since_step,"last_matching_evidence_ids":self.latest_proofs,
            "last_command_attempt":self.last_command_attempt,
            "limits":"Historical observations at tool dispatch. Declared checks cover bounded native metadata. Command execution, successful exit, and delivery of the user's whole request are distinct. A completion review can finish with an answer or return to remaining work."})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligence::checks::Proof;

    #[test]
    fn unknown_denied_and_historical_results_do_not_confirm_execution() {
        let args = json!({"program":"cargo","args":["test"]});
        let contract = Contract::default();
        for status in ["unknown", "not_executed", "cancelled"] {
            let mut mission = Mission::new(10);
            let r = json!({"evidence_id":"E11","execution_status":status,
                "command_outcome":"cancelled","exit_code":0});
            mission.observe(1, &contract, None, "run_command", &args, &r);
            let attempt = mission.last_command_attempt.as_ref().unwrap();
            assert_eq!(attempt["execution_confirmed"], false);
            assert!(attempt["exit_code"].is_null());
        }
        let mut mission = Mission::new(10);
        let cached = json!({"evidence_id":"E3","cached_result":true,"execution_status":"completed",
            "command_outcome":"success","exit_code":0});
        mission.observe(1, &contract, None, "run_command", &args, &cached);
        assert!(mission.last_command_attempt.is_none());
    }

    #[test]
    fn failing_rerun_proves_execution_and_revokes_candidate() {
        let args = json!({"program":"cargo","args":["test"]});
        let mut contract = Contract::default();
        contract.declare(&json!({"checks":[{"label":"pass","tool":"run_command","arguments":args,"field":"/exit_code","equals":0}]}), 1).unwrap();
        let mut mission = Mission::new(0);
        let pass = json!({"evidence_id":"E2","execution_status":"completed","command_outcome":"success","exit_code":0});
        contract.observe(
            "run_command",
            &args,
            &pass,
            Proof {
                evidence_id: "E2".into(),
                message_index: 2,
                workspace_fingerprint: "w".into(),
                coverage_complete: true,
            },
        );
        mission.observe(2, &contract, Some("w"), "run_command", &args, &pass);
        assert_eq!(mission.checks_met_since_step, Some(2));
        let fail = json!({"ok":false,"evidence_id":"E3","execution_status":"completed","command_outcome":"nonzero_exit","exit_code":1});
        mission.observe(3, &contract, Some("w"), "run_command", &args, &fail);
        assert_eq!(mission.checks_met_since_step, None);
        assert_eq!(
            mission.report()["last_command_attempt"]["execution_confirmed"],
            true
        );
        assert_eq!(mission.report()["last_command_attempt"]["exit_code"], 1);
        let next = Mission::new(10);
        assert!(next.last_command_attempt.is_none());
        assert_eq!(next.report()["declared_check_state"], "not_declared");
    }
}
