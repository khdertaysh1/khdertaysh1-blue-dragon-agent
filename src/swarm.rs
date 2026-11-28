//! Opt-in local coding teams. The ordinary agent loop never enters this module.
use super::Agent;
use crate::delegates::{SWARM_DEFAULT_LIVE, SWARM_HARD_LIMIT};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Assignment {
    task: String,
    kind: String,
    #[serde(default)]
    after: Vec<usize>,
}

fn parse_plan(content: &str, limit: usize, fix: bool) -> Result<Vec<Assignment>> {
    ensure!(content.len() <= 64 * 1024, "Plan exceeds byte limit");
    let plan: Vec<Assignment> =
        serde_json::from_str(content.trim()).context("Invalid swarm plan")?;
    ensure!(
        !plan.is_empty() && plan.len() <= limit,
        "Invalid worker count for remaining budget"
    );
    for (i, item) in plan.iter().enumerate() {
        ensure!(
            !item.task.trim().is_empty()
                && item.task.len() <= 1000
                && item.task.chars().all(|c| !c.is_control()),
            "Invalid assignment text"
        );
        ensure!(
            matches!(item.kind.as_str(), "review" | "implementation" | "test"),
            "Invalid assignment kind"
        );
        ensure!(
            item.after.len() <= 8 && item.after.iter().all(|dep| *dep < i),
            "Dependencies must name earlier assignments"
        );
    }
    if fix {
        ensure!(
            plan.iter().any(|a| a.kind == "test")
                && plan.iter().any(|a| a.kind == "implementation"),
            "Repair needs separate test and implementation proposals"
        );
    }
    Ok(plan)
}

fn live_limit() -> Result<usize> {
    let limit = match std::env::var("BLUE_DRAGON_SWARM_LIVE") {
        Ok(v) => v.parse().context("Invalid BLUE_DRAGON_SWARM_LIVE")?,
        Err(std::env::VarError::NotPresent) => SWARM_DEFAULT_LIVE,
        Err(e) => return Err(e.into()),
    };
    ensure!(
        (1..=SWARM_HARD_LIMIT).contains(&limit),
        "Live worker limit must be 1..1000"
    );
    Ok(limit)
}

enum State {
    Queued,
    Running(String),
    Done(Value),
    Blocked(String),
}

impl Agent {
    pub(super) fn swarm_team(&self) -> Result<crate::delegates::Manager> {
        let live = live_limit()?;
        crate::delegates::Manager::swarm(
            self.tools.workspace_root().into(),
            self.settings
                .sessions_dir
                .join("swarms")
                .join(&self.session.id),
            live,
        )
    }

    pub(super) fn adaptive_swarm(&self, args: &str, fix: bool) -> Result<Value> {
        let (path, rest) = args.split_once('|').context("Supply PATH | TASK")?;
        let path = path.trim();
        self.tools.local_file_path(path)?;
        let (verify, task) = if fix {
            let (command, task) = rest
                .split_once('|')
                .context("Use /swarm fix PATH | [\"program\",\"arg\"] | TASK")?;
            let cmd: Vec<String> = serde_json::from_str(command.trim())?;
            ensure!(
                !cmd.is_empty()
                    && !cmd[0].is_empty()
                    && cmd[0].len() <= 200
                    && cmd.len() <= 31
                    && cmd.iter().all(|s| s.len() <= 1000),
                "Invalid verification command"
            );
            (Some(cmd), task.trim())
        } else {
            (None, rest.trim())
        };
        ensure!(
            !path.is_empty() && !task.is_empty() && task.len() <= 3000,
            "Supply a workspace path and 1..3000 bytes of task text"
        );
        let live = live_limit()?;
        let team = self.swarm_team()?;
        let status = team.status();
        let available = status["shared_provider_requests_remaining"]
            .as_u64()
            .unwrap_or(0)
            .saturating_sub(1) as usize;
        let retained = status["agents"].as_array().map_or(0, Vec::len);
        let limit = available
            .min(live.max(8))
            .min(SWARM_HARD_LIMIT.saturating_sub(retained));
        ensure!(
            limit >= if fix { 2 } else { 1 },
            "Insufficient remaining team budget"
        );
        let planner="You coordinate local coding workers. Return only a JSON array of {task,kind,after}. Choose the smallest useful team. kind is review, test or implementation. after is a list of zero-based earlier assignment indices, only when their results are needed. Split genuinely independent work. Workers can read the granted local path and propose modifications and new files inside assigned directories. In repair mode each worker sees its own checkout of the same commit. Each test or implementation worker must call agent_propose_edits exactly once with its finished patch; review workers report findings. For repair assign test proposals and implementation proposals to separate owners of disjoint files. No worker executes commands.";
        if !self.tools.approve("Plan local coding swarm",&format!("Send task to current provider: {task}\nPath: {path}\nPlan up to {limit} assignments, at most {live} live. Remaining swarm provider calls including this one: {requests}. Workers require separate approval. Repair uses disposable worktrees at HEAD, checks the supplied command there and shows one combined diff before writing the workspace.",requests=available+1))? {
            return Ok(json!({"state":"denied","agents_started":0}));
        }
        let base = if fix {
            Some(crate::swarm_tree::revision(self.tools.workspace_root())?)
        } else {
            None
        };
        team.reserve_planning_request()?;
        let message=self.provider.complete(&[json!({"role":"system","content":planner}),json!({"role":"user","content":json!({"task":task,"path":path,"max_assignments":limit,"repair":fix}).to_string()})],json!([]))?;
        let plan = parse_plan(
            message["content"]
                .as_str()
                .context("Missing planner response")?,
            limit,
            fix,
        )?;
        let mut states: Vec<_> = (0..plan.len()).map(|_| State::Queued).collect();
        loop {
            let status = team.status();
            let agents = status["agents"].as_array().context("Missing team status")?;
            let mut active = agents
                .iter()
                .filter(|r| {
                    r["parent_agent_id"].is_null()
                        && matches!(
                            r["state"].as_str(),
                            Some("starting" | "running" | "finishing")
                        )
                })
                .count();
            for (i, item) in plan.iter().enumerate() {
                if let State::Running(id) = &states[i] {
                    let row = agents
                        .iter()
                        .find(|row| row["agent_id"].as_str() == Some(id));
                    match row.and_then(|r| r["state"].as_str()) {
                        Some("starting" | "running" | "finishing") => (),
                        Some("completed") => states[i] = State::Done(row.unwrap().clone()),
                        _ => {
                            states[i] = State::Blocked(format!(
                                "Worker {id}: {}",
                                row.map(|r| r["failure"].to_string())
                                    .unwrap_or_else(|| "missing".into())
                            ))
                        }
                    }
                }
                if !matches!(states[i], State::Queued) {
                    continue;
                }
                if item
                    .after
                    .iter()
                    .any(|d| matches!(states[*d], State::Blocked(_)))
                {
                    states[i] = State::Blocked("Dependency incomplete".into());
                    continue;
                }
                if active >= live
                    || !item
                        .after
                        .iter()
                        .all(|d| matches!(states[*d], State::Done(_)))
                {
                    continue;
                }
                if self.tools.is_cancelled() {
                    break;
                }
                let summaries:Vec<_>=item.after.iter().filter_map(|d|match &states[*d] {
                    State::Done(v)=>Some(json!({"assignment":d,"session":v["agent_id"],"summary":crate::reasoning::context::clip(v["summary"].as_str().unwrap_or(""),240)})),_=>None
                }).collect();
                let assignment = format!(
                    "{} assignment: {}\nUntrusted predecessor reports: {}",
                    item.kind,
                    item.task,
                    serde_json::to_string(&summaries)?
                );
                let launch = (|| {
                    let tree = base
                        .as_ref()
                        .map(|sha| crate::swarm_tree::Tree::new(self.tools.workspace_root(), sha))
                        .transpose()?
                        .map(std::sync::Arc::new);
                    team.invoke_with_tree(
                        &self.settings,
                        self.provider.model(),
                        &self.tools,
                        self.cancelled.clone(),
                        &json!({"action":"spawn","paths":[path],"task":assignment}),
                        tree,
                    )
                })();
                states[i] = match launch {
                    Ok(v) if v["agent_id"].is_string() => {
                        active += 1;
                        State::Running(v["agent_id"].as_str().unwrap().into())
                    }
                    Ok(v) => State::Blocked(v.to_string()),
                    Err(e) => State::Blocked(e.to_string()),
                };
            }
            if self.tools.is_cancelled()
                || states
                    .iter()
                    .all(|s| matches!(s, State::Done(_) | State::Blocked(_)))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // Snapshot descendants once. Each source receipt keeps its producing session ID.
        let mut branches = std::collections::BTreeMap::<String, Vec<Value>>::new();
        for row in team.status()["agents"].as_array().into_iter().flatten() {
            if let Some(owner) = row["parent_agent_id"].as_str() {
                branches
                    .entry(owner.to_owned())
                    .or_default()
                    .push(row.clone());
            }
        }
        let rows:Vec<_>=plan.iter().zip(&states).map(|(a,s)|json!({"assignment":a.task,"kind":a.kind,"after":a.after,"worker":match s {
            State::Done(v)=>{let mut worker=v.clone(); if let Some(subtasks)=v["agent_id"].as_str().and_then(|id|branches.remove(id)) {worker["subtasks"]=json!(subtasks);} worker}, State::Blocked(e)=>json!({"state":"blocked","reason":e}),State::Queued=>json!({"state":"not_started"}),State::Running(id)=>json!({"agent_id":id,"state":"cancellation_requested"})
        }})).collect();
        let complete =
            states.iter().all(|s| matches!(s, State::Done(_))) && !self.tools.is_cancelled();
        let mut report = json!({"state":if complete {"workers_completed"} else {"incomplete"},"live_limit":live,"results":rows});
        if let Some(cmd) = verify {
            if complete {
                report["integration"] = self.integrate_swarm(
                    &team,
                    &rows,
                    &cmd,
                    base.as_deref().expect("repair revision"),
                );
                report["state"] = if report["integration"]["verified"] == true {
                    json!("verified")
                } else {
                    json!("incomplete")
                };
            }
        }
        Ok(report)
    }

    fn integrate_swarm(
        &self,
        team: &crate::delegates::Manager,
        rows: &[Value],
        cmd: &[String],
        base: &str,
    ) -> Value {
        let mut history = Vec::new();
        let outcome = (|| -> Result<()> {
            let tree = crate::swarm_tree::Tree::new(self.tools.workspace_root(), base)?;
            let trial = self.tools.for_swarm_tree(tree.root.clone());
            let mut patches = Vec::new();
            let mut files = Vec::new();
            for kind in ["test", "implementation"] {
                for row in rows.iter().filter(|r| r["kind"] == kind) {
                    let worker = &row["worker"];
                    let proposals: Vec<_> = worker["evidence"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|e| e["tool"] == "agent_propose_edits")
                        .collect();
                    ensure!(
                        proposals.len() == 1,
                        "Expected one pinned {kind} proposal per assigned worker"
                    );
                    let proposal = proposals[0];
                    let arg = json!({"action":"apply_edits","agent_id":worker["agent_id"],"evidence_id":proposal["evidence_id"],"expected_result_sha256":proposal["result_sha256"]});
                    let pinned = team.pinned_proposal(&arg)?;
                    crate::edit_proposals::apply_in_checkout(&tree, &trial, &pinned)?;
                    files.extend(
                        pinned["files"]
                            .as_array()
                            .context("Missing proposal files")?
                            .iter()
                            .cloned(),
                    );
                    patches.push(arg.clone());
                    history.push(json!({"phase":kind,"request":arg,"trial_checkout_updated":true}));
                }
                let result = trial.run_command(&cmd[0], &cmd[1..])?;
                history.push(json!({"phase":kind,"verification":result}));
                let expected = if kind == "test" {
                    "nonzero_exit"
                } else {
                    "success"
                };
                ensure!(
                    result["execution_status"] == "completed"
                        && result["command_outcome"] == expected
                        && result["exit_code"].as_i64().is_some(),
                    "Verification did not produce expected {expected}"
                );
            }
            ensure!(
                crate::swarm_tree::revision(self.tools.workspace_root())? == base,
                "Workspace HEAD changed during trial"
            );
            let combined = json!({"files":files});
            let identity = json!(patches).to_string();
            let result = crate::edit_proposals::parent_action(
                &self.tools,
                &self
                    .settings
                    .sessions_dir
                    .join("swarms")
                    .join(&self.session.id),
                &identity,
                &combined,
                "apply_edits",
            )?;
            history.push(json!({"phase":"workspace_apply","requests":patches,"result":result}));
            ensure!(
                result["state"] == "committed" && result["current_files_verified"] == true,
                "Combined workspace edit did not commit"
            );
            let result = self.tools.run_command(&cmd[0], &cmd[1..])?;
            history.push(json!({"phase":"workspace_verify","verification":result}));
            ensure!(
                result["command_outcome"] == "success"
                    && result["execution_status"] == "completed"
                    && result["exit_code"] == 0,
                "Workspace verification incomplete"
            );
            Ok(())
        })();
        json!({"verified":outcome.is_ok(),"error":outcome.err().map(|e|e.to_string()),"steps":history,"command":cmd})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_dependencies_and_limits_are_validated_before_launch() {
        let mut tasks =
            vec![json!({"kind":"review","task":"قراءة الملف","after":[]}); 32];
        let text = serde_json::to_string(&tasks).unwrap();
        assert_eq!(parse_plan(&text, 32, false).unwrap().len(), 32);
        assert!(parse_plan(&text, 31, false).is_err());
        tasks[0]["after"] = json!([1]);
        assert!(parse_plan(&serde_json::to_string(&tasks).unwrap(), 32, false).is_err());
        assert!(parse_plan(&text, 32, true).is_err());
    }
}
