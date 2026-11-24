//! Bounded coverage and controlled evidence comparison; never a vulnerability oracle.
use crate::reasoning::claims::is_historical as stale_result;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    entries: BTreeMap<String, Row>,
    pub truncated: bool,
}
#[derive(Debug, Serialize, Deserialize)]
struct Row {
    target: String,
    kind: String,
    state: String,
    observations: usize,
    evidence_ids: Vec<String>,
    #[serde(default)]
    observed_evidence_ids: Vec<String>,
    #[serde(default)]
    successful_observations: usize,
    #[serde(default)]
    blocked_observations: usize,
    #[serde(default)]
    unknown_observations: usize,
    #[serde(default)]
    partial_observations: usize,
}
pub fn safe_url(raw: &str) -> Option<String> {
    let mut u = reqwest::Url::parse(raw).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let keys = u
        .query_pairs()
        .map(|(k, _)| k.into_owned())
        .collect::<Vec<_>>();
    u.set_username("").ok()?;
    u.set_password(None).ok()?;
    u.set_fragment(None);
    u.set_query(None);
    if !keys.is_empty() {
        u.query_pairs_mut()
            .extend_pairs(keys.iter().map(|k| (k.as_str(), "REDACTED")));
    }
    Some(u.to_string())
}
fn capture_partial(value: &Value, remaining: &mut usize, depth: usize) -> bool {
    if *remaining == 0 || depth > 12 {
        return true;
    }
    *remaining -= 1;
    match value {
        Value::Object(map) => map.iter().any(|(key, v)| {
            ((key == "truncated" || key == "limited" || key.ends_with("_truncated")) && v == true)
                || (key.ends_with("_omitted") && v.as_u64().is_some_and(|n| n > 0))
                || ((key == "forms" || key == "fields")
                    && v.as_array()
                        .is_some_and(|items| items.len() >= if key == "forms" { 20 } else { 60 })
                    && map.get(&format!("{key}_truncated")) != Some(&Value::Bool(false)))
                || capture_partial(v, remaining, depth + 1)
        }),
        Value::Array(items) => items
            .iter()
            .any(|v| capture_partial(v, remaining, depth + 1)),
        _ => false,
    }
}
impl Ledger {
    fn add(&mut self, url: &str, kind: &str, state: &str, evidence: &str) {
        let Some(target) = safe_url(url) else {
            return;
        };
        // Deduplicate by redacted location/kind: never retain query secrets or
        // create an unbounded row per tracking-token value.
        let key = crate::intelligence::code::hash(format!("{kind}:{target}").as_bytes());
        if !self.entries.contains_key(&key) && self.entries.len() >= 500 {
            self.truncated = true;
            return;
        }
        let row = self.entries.entry(key).or_insert_with(|| Row {
            target,
            kind: kind.into(),
            state: "discovered_not_tested".into(),
            observations: 0,
            evidence_ids: Vec::new(),
            observed_evidence_ids: Vec::new(),
            successful_observations: 0,
            blocked_observations: 0,
            unknown_observations: 0,
            partial_observations: 0,
        });
        if state != "discovered_not_tested"
            && !row.observed_evidence_ids.iter().any(|id| id == evidence)
        {
            row.state = state.into();
            row.observations = row.observations.saturating_add(1);
            if state == "outcome_unknown" {
                row.unknown_observations = row.unknown_observations.saturating_add(1);
            } else if state == "blocked_or_failed" {
                row.blocked_observations = row.blocked_observations.saturating_add(1);
            } else {
                row.successful_observations = row.successful_observations.saturating_add(1);
                if state == "observed_partial_not_security_verified" {
                    row.partial_observations = row.partial_observations.saturating_add(1);
                }
            }
            if row.observed_evidence_ids.len() >= 8 {
                row.observed_evidence_ids.remove(0);
            }
            row.observed_evidence_ids.push(evidence.into());
        }
        if !row.evidence_ids.iter().any(|e| e == evidence) {
            if row.evidence_ids.len() >= 8 {
                row.evidence_ids.remove(0);
            }
            row.evidence_ids.push(evidence.into());
        }
    }
    fn walk(
        &mut self,
        value: &Value,
        kind: &str,
        evidence: &str,
        depth: usize,
        remaining: &mut usize,
    ) {
        if depth > 12 || *remaining == 0 {
            self.truncated = true;
            return;
        }
        *remaining -= 1;
        match value {
            Value::Object(object) => {
                for (key, item) in object {
                    let child = match key.as_str() {
                        "forms" => "form",
                        "network" | "requests" => "api",
                        "scripts" | "javascript_review" => "source",
                        _ => kind,
                    };
                    if matches!(key.as_str(), "url" | "href" | "action" | "requested_url") {
                        if let Some(url) = item.as_str() {
                            self.add(url, child, "discovered_not_tested", evidence);
                        }
                    }
                    self.walk(item, child, evidence, depth + 1, remaining);
                    if *remaining == 0 {
                        self.truncated = true;
                        break;
                    }
                }
            }
            Value::Array(items) => {
                if items.len() > 500 {
                    self.truncated = true;
                }
                for item in items.iter().take(500) {
                    self.walk(item, kind, evidence, depth + 1, remaining);
                    if *remaining == 0 {
                        self.truncated = true;
                        break;
                    }
                }
            }
            Value::String(url) if kind == "source" => {
                self.add(url, kind, "discovered_not_tested", evidence)
            }
            _ => {}
        }
    }
    pub fn record(&mut self, tool: &str, args: &Value, result: &Value, evidence: &str) {
        if stale_result(result) {
            return;
        }
        if matches!(tool, "browser" | "browser_url_inspection") {
            self.walk(result, "page", evidence, 0, &mut 5000);
        }
        let url = args["url"]
            .as_str()
            .or_else(|| result.pointer("/result/url").and_then(Value::as_str))
            .or_else(|| result["requested_url"].as_str());
        if let Some(url) = url {
            let partial = capture_partial(result, &mut 5000, 0);
            let state = if result["execution_status"] == "unknown" {
                "outcome_unknown"
            } else if result["denied"] == true
                || result["ok"] == false
                || result["execution_status"] == "not_executed"
            {
                "blocked_or_failed"
            } else if partial {
                "observed_partial_not_security_verified"
            } else {
                "observed_not_security_verified"
            };
            self.add(
                url,
                if matches!(tool, "web_fetch" | "source_artifact") {
                    "source"
                } else {
                    "page"
                },
                state,
                evidence,
            );
        }
    }
    pub fn report(&self) -> Value {
        json!({"coverage":self.entries.values().collect::<Vec<_>>(),"truncated":self.truncated,"max_entries":500,"discovery_node_budget":5000,"observation_dedup_window":8,"limits":"Discovery is not testing. Repeated query values collapse to redacted locations. Form submissions, endpoint exhaustiveness, login identity, semantic paths and exploitability are not inferred. Raw request/response bodies, cookies, authorization headers and fragments omitted; paths may still need human redaction before sharing."})
    }
}
/// Rebuild a bounded view from verified observations, never trust persisted counters.
pub fn verified_coverage(session: &crate::session::Session) -> Value {
    let mut ledger = Ledger::default();
    let mut unavailable = 0;
    let mut historical = 0;
    let mut oversized = 0;
    let mut scanned = 0;
    let mut budget = 4 * 1024 * 1024usize;
    for observation in session
        .observations
        .iter()
        .rev()
        .take(100)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        if !matches!(
            observation.tool.as_str(),
            "browser" | "browser_url_inspection" | "web_fetch" | "source_artifact"
        ) {
            continue;
        }
        let size = session
            .messages
            .get(observation.message_index)
            .and_then(|m| m["content"].as_str())
            .map(str::len)
            .unwrap_or(0);
        if size > 512 * 1024 || size > budget {
            oversized += 1;
            continue;
        }
        budget -= size;
        scanned += 1;
        let Ok((tool, mut args, result)) = request_for(session, &observation.id) else {
            unavailable += 1;
            continue;
        };
        if stale_result(&result) {
            historical += 1;
            continue;
        }
        // request_for normalizes URLs. Only redacted values enter this overview.
        if let (Some(origin), Some(path)) = (
            args.pointer("/url/origin").and_then(Value::as_str),
            args.pointer("/url/path").and_then(Value::as_str),
        ) {
            if let Ok(mut url) = reqwest::Url::parse(&format!("{origin}{path}")) {
                if let Some(query) = args.pointer("/url/query").and_then(Value::as_object) {
                    for (key, values) in query {
                        for _ in values.as_array().into_iter().flatten() {
                            url.query_pairs_mut().append_pair(key, "REDACTED");
                        }
                    }
                }
                args["url"] = json!(url.as_str());
            }
        }
        ledger.record(&tool, &args, &result, &observation.id);
    }
    let mut report = ledger.report();
    if let Some(rows) = report["coverage"].as_array_mut() {
        for row in rows {
            row["security_test_completed"] = json!(false);
            row["currently_in_read_scope"] = if session.scope.enabled {
                json!(row["target"]
                    .as_str()
                    .is_some_and(|url| session.scope.check(url, "read").is_ok()))
            } else {
                Value::Null
            };
            row["automatic_retry_allowed"] = json!(false);
        }
    }
    report["provenance"] = json!({"mode":"rebuilt_from_verified_recent_observations","observation_window":100,"older_observations_omitted":session.observations.len().saturating_sub(100),"candidates_checked":scanned,"unavailable_or_mismatched_evidence":unavailable,"historical_results_skipped":historical,"oversized_or_over_budget_skipped":oversized,"message_byte_budget":4*1024*1024,"max_message_bytes":512*1024,"scope_configured":session.scope.enabled,"stored_ledger_counters_trusted":false});
    report["test_cases"] = crate::test_coverage::report(session);
    report["fresh_evidence"] = json!(false);
    report["network_requests"] = json!(0);
    report
}
fn normalized_arguments(mut args: Value) -> Result<Value> {
    if let Some(raw) = args["url"].as_str() {
        let u = reqwest::Url::parse(raw)?;
        let mut query = BTreeMap::<String, Vec<String>>::new();
        for (k, v) in u.query_pairs() {
            query
                .entry(k.into_owned())
                .or_default()
                .push(v.into_owned());
        }
        args["url"] = json!({"origin":u.origin().ascii_serialization(),"path":u.path(),"query":query,"fragment":u.fragment()});
    }
    Ok(args)
}
/// Resolve cited metadata from its native request/result before accepting a new
/// finding, including after loading checkpoints made by older versions.
pub(crate) fn verified_finding(
    session: &crate::session::Session,
    args: &Value,
) -> Result<crate::reasoning::Finding> {
    let ids = args["evidence_ids"]
        .as_array()
        .context("evidence_ids required")?;
    if ids.len() > 12 {
        bail!("At most 12 evidence IDs");
    }
    let checked = session
        .observations
        .iter()
        .filter(|o| ids.iter().any(|id| id.as_str() == Some(o.id.as_str())))
        .map(|entry| {
            let mut checked = entry.clone();
            match request_for(session, &entry.id) {
                Ok((tool, _, result)) => {
                    checked.native_facts = crate::reasoning::claims::extract(&tool, &result);
                    checked.ok = !stale_result(&result)
                        && result["ok"] != false
                        && result["denied"] != true
                        && result["execution_status"] != "unknown";
                }
                Err(_) => {
                    checked.native_facts = crate::reasoning::claims::NativeFacts::default();
                    checked.ok = false;
                }
            }
            checked
        })
        .collect::<Vec<_>>();
    crate::reasoning::finding(args, &checked)
}
pub(crate) fn request_for(
    session: &crate::session::Session,
    id: &str,
) -> Result<(String, Value, Value)> {
    let (name, args, result) = paired_request_for(session, id)?;
    Ok((name, normalized_arguments(args)?, result))
}

pub(crate) fn paired_request_for(
    session: &crate::session::Session,
    id: &str,
) -> Result<(String, Value, Value)> {
    if session.observations.iter().filter(|e| e.id == id).count() != 1 {
        bail!("Evidence ID must identify exactly one observation");
    }
    let observation = session
        .observations
        .iter()
        .find(|e| e.id == id)
        .context("Unknown evidence ID")?;
    let message = session
        .messages
        .get(observation.message_index)
        .context("Evidence message unavailable")?;
    if message["role"] != "tool" {
        let origin = observation
            .automatic_request
            .as_ref()
            .context("Evidence is not a paired tool result")?;
        if observation.tool != "browser_url_inspection" || message["role"] != "user" {
            bail!("Invalid automatic evidence provenance");
        }
        let content = message["content"]
            .as_str()
            .context("Missing automatic evidence")?;
        if !content.starts_with(
            "The user requested inspection of the URL above. Blue Dragon automatically collected",
        ) {
            bail!("Automatic evidence wrapper unavailable");
        }
        let (_, body) = content
            .split_once("Evidence:\n")
            .context("Missing automatic result envelope")?;
        let result: Value = serde_json::from_str(body)?;
        if crate::intelligence::code::hash(result.to_string().as_bytes()) != origin.result_sha256
            || result["requested_url"].as_str() != Some(origin.url.as_str())
        {
            bail!("Automatic evidence request/result integrity mismatch");
        }
        crate::browser::validate_url(&origin.url)?;
        return Ok((observation.tool.clone(), json!({"url":origin.url}), result));
    }
    if observation.automatic_request.is_some() {
        bail!("Automatic provenance does not belong to a tool-result message");
    }
    let call_id = message["tool_call_id"]
        .as_str()
        .context("Missing call ID")?;
    if call_id.is_empty() {
        bail!("Empty evidence call ID");
    }
    for (prior_index, prior) in session.messages[..observation.message_index]
        .iter()
        .enumerate()
        .rev()
    {
        if !matches!(prior["role"].as_str(), Some("assistant" | "tool")) {
            bail!("Evidence crosses a conversation boundary");
        }
        if prior["role"] == "assistant" {
            let call = prior["tool_calls"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|c| c["id"] == call_id)
                .context("Unpaired evidence")?;
            if call["function"]["name"] != observation.tool
                || prior["tool_calls"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|c| c["id"] == call_id)
                    .count()
                    != 1
            {
                bail!("Ambiguous or mismatched evidence tool call");
            }
            let count = session.messages[prior_index + 1..]
                .iter()
                .take_while(|m| m["role"] == "tool")
                .filter(|m| m["tool_call_id"] == call_id)
                .count();
            if count != 1 {
                bail!("Ambiguous duplicate result for one tool call");
            }
            let raw_args: Value = serde_json::from_str(
                call["function"]["arguments"]
                    .as_str()
                    .context("Missing arguments")?,
            )?;
            if observation.request_sha256.as_ref().is_some_and(|hash| {
                *hash != crate::intelligence::code::hash(raw_args.to_string().as_bytes())
            }) {
                bail!("Original request integrity mismatch");
            }

            let result: Value =
                serde_json::from_str(message["content"].as_str().context("Missing result")?)?;
            if result
                .get("evidence_id")
                .is_some_and(|embedded| embedded.as_str() != Some(id))
            {
                bail!("Embedded evidence identity mismatch");
            }
            if observation
                .result_sha256
                .as_ref()
                .is_some_and(|hash| *hash != crate::reasoning::result_hash(&result))
            {
                bail!("Native result integrity mismatch");
            }
            return Ok((observation.tool.clone(), raw_args, result));
        }
    }
    bail!("Missing original tool request")
}
fn differences(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    if a == b || out.len() > 2 {
        return;
    }
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        let keys = a
            .keys()
            .chain(b.keys())
            .collect::<std::collections::BTreeSet<_>>();
        for k in keys {
            let child = format!("{path}/{}", k.replace('~', "~0").replace('/', "~1"));
            match (a.get(k), b.get(k)) {
                (Some(a), Some(b)) => differences(a, b, &child, out),
                _ => out.push(child), // Missing and explicit null are different inputs.
            }
            if out.len() > 2 {
                break;
            }
        }
    } else if let (Some(a), Some(b)) = (a.as_array(), b.as_array()) {
        for i in 0..a.len().max(b.len()) {
            let child = format!("{path}/{i}");
            match (a.get(i), b.get(i)) {
                (Some(a), Some(b)) => differences(a, b, &child, out),
                _ => out.push(child),
            }
            if out.len() > 2 {
                break;
            }
        }
    } else {
        out.push(path.into());
    }
}
pub fn compare(session: &crate::session::Session, args: &Value) -> Result<Value> {
    let baseline = args["baseline_evidence"]
        .as_str()
        .context("baseline_evidence required")?;
    let variant = args["variant_evidence"]
        .as_str()
        .context("variant_evidence required")?;
    if baseline == variant {
        bail!("Use two distinct observed results");
    }
    let (tool, a, ar) = request_for(session, baseline)?;
    let (other, b, br) = request_for(session, variant)?;
    if tool != other {
        bail!("Comparison requires the same tool");
    }
    if a.pointer("/url/origin") != b.pointer("/url/origin")
        || a.pointer("/url/path") != b.pointer("/url/path")
    {
        bail!("URL comparisons must retain origin/path; vary one query/fragment input");
    }
    let mut changed = Vec::new();
    differences(&a, &b, "", &mut changed);
    if changed.len() != 1 {
        bail!(
            "Expected exactly one changed argument field, observed {}",
            changed.len()
        );
    }
    for value in [&ar, &br] {
        if value["denied"] == true
            || value["execution_status"] == "unknown"
            || stale_result(value)
            || (value["ok"] == false && value["status"].as_u64().is_none())
        {
            bail!("Denied/unknown/cached results cannot form a fresh controlled pair");
        }
    }
    Ok(
        json!({"ok":true,"baseline_evidence":baseline,"variant_evidence":variant,"tool":tool,"changed_argument_fields":changed,"baseline_native":crate::reasoning::claims::extract(&tool,&ar),"variant_native":crate::reasoning::claims::extract(&tool,&br),"baseline_result_sha256":crate::intelligence::code::hash(ar.to_string().as_bytes()),"variant_result_sha256":crate::intelligence::code::hash(br.to_string().as_bytes()),"causality_proven":false,"verdict":"paired_observations_only","limits":"One serialized argument field changed; this does not prove only one semantic factor changed. Account/session, timing, server state and site errors may differ. Repeat controls and inspect native evidence; HTTP 200 and tool success do not prove a vulnerability. Input values and response bodies intentionally omitted."}),
    )
}
fn reproduction_steps(session: &crate::session::Session, id: &str) -> Value {
    let Ok((tool, _, result)) = request_for(session, id) else {
        return json!({"state":"blocked_missing_original_request","automatic_replay_allowed":false});
    };
    let index = session
        .observations
        .iter()
        .find(|o| o.id == id)
        .unwrap()
        .message_index;
    if let Some(native) = session
        .observations
        .iter()
        .find(|o| o.id == id)
        .and_then(|o| o.automatic_request.as_ref())
    {
        return json!({"state":"manual_review_required","automatic_replay_allowed":false,"tool":tool,
            "original_arguments_sha256":crate::intelligence::code::hash(json!({"url":native.url}).to_string().as_bytes()),
            "request_reference":{"observation_id":id,"field":"automatic_request.url","storage":"local_session_checkpoint"},
            "response_reference":{"message_index":index,"field":"content","native_result_sha256":native.result_sha256},
            "steps":["Retrieve the original URL from the local checkpoint metadata; the user-message wrapper alone is not provenance.","Verify the native result hash, current user scope and account/session conditions.","Collect a new approved control before a one-factor variant; do not automatically replay active browser actions."],
            "unverified_conditions":["account_identity","server_state","semantic_impact"]});
    }
    let call_index = session.messages[..index]
        .iter()
        .rposition(|m| m["role"] == "assistant")
        .unwrap();
    let call_id = &session.messages[index]["tool_call_id"];
    let call = session.messages[call_index]["tool_calls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| &c["id"] == call_id)
        .unwrap();
    let raw_args = call["function"]["arguments"].as_str().unwrap();
    let blocked = result["denied"] == true
        || result["execution_status"] == "unknown"
        || stale_result(&result);
    json!({"state":if blocked {"blocked_historical_or_unexecuted"} else {"manual_review_required"},
        "automatic_replay_allowed":false,"tool":tool,
        "original_arguments_sha256":crate::intelligence::code::hash(raw_args.as_bytes()),
        "request_reference":{"message_index":call_index,"field":"tool_calls","select_by_result_call_id_at":index},
        "response_reference":{"message_index":index,"field":"content"},
        "steps":[
            "Retrieve the referenced local assistant request and tool response with recall_context; verify the original argument hash before editing a copy.",
            "Review and redact URLs, inputs, headers, cookies and response data before sharing; exported references do not include their private values.",
            "Re-establish the authorized origin, path, action class and account. Confirm current conditions and explicit approval for any state change.",
            "Inspect uncertain prior effects before any retry. Historical, denied or unknown results are not an independent baseline.",
            "Record a fresh normal control and one-factor variant with identical session conditions where possible; retain actual request/response evidence and repeat controls.",
            "Report the observed difference, competing explanations and proven impact separately; status 200 or tool success is not vulnerability proof."
        ],"unverified_conditions":["account_identity","server_state","request_wire_equivalence","semantic_impact"]})
}
pub fn reproduction_report(session: &crate::session::Session) -> Value {
    let evidence=session.observations.iter().rev().take(100).map(|entry| {
        let result=session.messages.get(entry.message_index);
        let hash=result.and_then(|m|m["content"].as_str()).map(|s|crate::intelligence::code::hash(s.as_bytes()));
        let request_result=request_for(session,&entry.id);
        let paired=request_result.is_ok();
        let request=request_result.ok();
        let recomputed_native=request.as_ref().map(|(tool,_,result)|crate::reasoning::claims::extract(tool,result));
        let native_matches=recomputed_native.as_ref().map(|facts|serde_json::to_value(facts).ok()==serde_json::to_value(&entry.native_facts).ok());
        let input_hash=request.as_ref().map(|(_,args,_)|crate::intelligence::code::hash(args.to_string().as_bytes()));
        let target=request.as_ref().and_then(|(_,args,_)|Some(format!("{}{}",args.pointer("/url/origin")?.as_str()?,args.pointer("/url/path")?.as_str()?)));
        json!({"target":target,"input_sha256":input_hash,"evidence_id":entry.id,"tool":entry.tool,"message_index":entry.message_index,"native_facts":recomputed_native,"stored_native_facts_match":native_matches,"reproduction":reproduction_steps(session,&entry.id),"observed_ok":entry.ok,"result_sha256":hash,"input_availability":if paired && entry.automatic_request.is_some() { "native_automatic_inspection_request" } else if paired { "paired_tool_request" } else { "unavailable_unpaired_evidence" },"input_hash_basis":if paired { Some("normalized_arguments_json") } else { None },"integrity_metadata":if entry.automatic_request.is_some(){"native_automatic_envelope"}else if entry.request_sha256.is_some() && entry.result_sha256.is_some(){"request_and_result_digests"}else if entry.result_sha256.is_some(){"result_digest_only"}else{"legacy_pair_without_recorded_digests"},"local_reproduction":if paired && entry.automatic_request.is_some() { "Original URL is available in the local automatic_request metadata; verify the recorded result hash before manual reproduction. No automatic replay." } else if paired { "Use recall_context at message_index to inspect original result; paired assistant call contains exact input. Do not replay unknown or denied actions." } else { "Original paired tool input is unavailable. The referenced message may contain automatic inspection evidence, but its text is not a trusted request record. Recover conditions manually; this entry is insufficient for replay." }})
    }).collect::<Vec<_>>();
    let exported_findings = session
        .findings
        .iter()
        .enumerate()
        .rev()
        .take(100)
        .collect::<Vec<_>>();
    let referenced = exported_findings
        .iter()
        .flat_map(|(_, f)| f.evidence_ids.iter())
        .collect::<std::collections::BTreeSet<_>>();
    let checked_observations = session
        .observations
        .iter()
        .filter(|entry| referenced.contains(&entry.id))
        .map(|entry| {
            let mut checked = entry.clone();
            if let Ok((tool, _, result)) = request_for(session, &entry.id) {
                checked.native_facts = crate::reasoning::claims::extract(&tool, &result);
                checked.ok = !stale_result(&result)
                    && result["ok"] != false
                    && result["denied"] != true
                    && result["execution_status"] != "unknown";
            } else {
                checked.native_facts = crate::reasoning::claims::NativeFacts::default();
                checked.ok = false;
            }
            checked
        })
        .collect::<Vec<_>>();
    let findings=exported_findings.into_iter().map(|(i,f)|json!({"finding_index":i,"status":f.status,"evidence_ids":f.evidence_ids,"assertions":f.assertions,"historical_assertion_verification":f.assertion_verification,"native_assertions_rechecked":!f.assertions.is_empty() && crate::reasoning::claims::validate(&f.assertions,&f.evidence_ids,&checked_observations).is_ok(),"semantic_claim_verified":false,"hypothesis_fields_present":!f.next_test.is_empty()&&!f.confirms.is_empty()&&!f.refutes.is_empty(),"narrative":"Private model-authored summary/test plan remains in the local session; review and redact before sharing."})).collect::<Vec<_>>();
    json!({"schema":2,"session_id":session.id,"source_discovery":crate::source_report::report(session),"source_investigations":crate::source_report::investigations(session),"local_lab_comparisons":crate::lab_report::report(session),"coverage":verified_coverage(session),"evidence":evidence,"evidence_omitted":session.observations.len().saturating_sub(100),"findings":findings,"findings_omitted":session.findings.len().saturating_sub(100),"conditions":{"scope_enforced":session.scope.enabled,"login_identity_verified":false,"same_server_state_verified":false,"raw_bodies_and_query_values_omitted":true},"reproduction_limits":"Redacted evidence index, not a complete standalone exploit report. URLs omit all query values and fragments; raw inputs, request/response bodies and model prose omitted by default. Obtain and review the referenced local input/result before reproducing. Native status 200 or a successful tool does not prove exploitability. Paths may need human redaction."})
}
pub fn definitions() -> Vec<Value> {
    vec![
        crate::audit_engines::definition(),
        crate::test_coverage::definition(),
        json!({"type":"function","function":{"name":"audit_coverage","description":"Read bounded deduplicated discovered/observed/blocked coverage. Does not crawl or claim security coverage.","parameters":{"type":"object","properties":{},"additionalProperties":false}}}),
        json!({"type":"function","function":{"name":"compare_observations","description":"Compare two paired tool evidence IDs only when exactly one serialized argument field differs; returns native metadata and hashes, never causal or vulnerability proof. Account and server state stability are unverified.","parameters":{"type":"object","properties":{"baseline_evidence":{"type":"string"},"variant_evidence":{"type":"string"}},"required":["baseline_evidence","variant_evidence"],"additionalProperties":false}}}),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pair_fixture() -> crate::session::Session {
        let root = tempfile::tempdir().unwrap();
        let mut session = crate::session::new(root.path(), "fixture");
        for (i, offset) in [0, 10].iter().enumerate() {
            let call = format!("c{i}");
            session.messages.push(json!({"role":"assistant","tool_calls":[{"id":call,"function":{"name":"source_artifact","arguments":json!({"action":"read","artifact_id":"fixture","offset":offset}).to_string()}}]}));
            let result = json!({"ok":true,"text":"local source excerpt"});
            session.observations.push(crate::reasoning::observe(
                session.messages.len(),
                "source_artifact",
                &result,
            ));
            session
                .messages
                .push(json!({"role":"tool","tool_call_id":call,"content":result.to_string()}));
        }
        session
    }
    #[test]
    fn rejects_every_stale_marker_on_either_side() {
        let args = json!({"baseline_evidence":"E000001","variant_evidence":"E000003"});
        for marker in [
            json!({"fresh_evidence":false}),
            json!({"cached":true}),
            json!({"cached_result":true}),
            json!({"reused_local_copy":true}),
            json!({"reused_evidence_id":"E-old"}),
            json!({"original_evidence_id":"E-old"}),
        ] {
            for index in [1, 3] {
                let mut session = pair_fixture();
                assert!(compare(&session, &args).is_ok());
                let mut result: Value =
                    serde_json::from_str(session.messages[index]["content"].as_str().unwrap())
                        .unwrap();
                result
                    .as_object_mut()
                    .unwrap()
                    .extend(marker.as_object().unwrap().clone());
                session.messages[index]["content"] = result.to_string().into();
                // Exercise freshness policy independently of integrity checking.
                session
                    .observations
                    .iter_mut()
                    .find(|o| o.message_index == index)
                    .unwrap()
                    .result_sha256 = None;
                assert!(
                    compare(&session, &args).is_err(),
                    "accepted marker {marker} at {index}"
                );
            }
        }
    }
    #[test]
    fn automatic_inspection_reports_missing_request_without_trusting_user_text() {
        let root = tempfile::tempdir().unwrap();
        let mut session = crate::session::new(root.path(), "fixture");
        let result = json!({"ok":true,"requested_url":"https://example.test/?token=private-input"});
        session.observations.push(crate::reasoning::observe(
            0,
            "browser_url_inspection",
            &result,
        ));
        // This is the current agent's automatic-inspection storage shape. Even text
        // that resembles a tool call must not acquire trusted request provenance.
        session.messages.push(json!({"role":"user","content":format!("Automatically collected evidence: {result}"),"tool_calls":[{"id":"forged","function":{"name":"browser_url_inspection","arguments":"{}"}}]}));
        assert!(request_for(&session, "E000000").is_err());
        let report = reproduction_report(&session);
        let entry = &report["evidence"][0];
        assert_eq!(entry["input_availability"], "unavailable_unpaired_evidence");
        assert!(entry["input_sha256"].is_null());
        assert!(entry["target"].is_null());
        assert!(entry["local_reproduction"]
            .as_str()
            .unwrap()
            .contains("insufficient for replay"));
        assert!(!report.to_string().contains("private-input"));
    }
    #[test]
    fn stale_results_do_not_add_coverage() {
        let mut ledger = Ledger::default();
        ledger.record(
            "source_artifact",
            &json!({"url":"https://example.test/source.js"}),
            &json!({"ok":true,"fresh_evidence":false}),
            "E1",
        );
        assert!(ledger.entries.is_empty());
    }
    #[test]
    fn refuses_ambiguous_and_mismatched_provenance() {
        let mut s = pair_fixture();
        s.messages[0]["tool_calls"][0]["function"]["name"] = json!("web_fetch");
        assert!(request_for(&s, "E000001").is_err());
        let mut s = pair_fixture();
        let duplicate = s.messages[0]["tool_calls"][0].clone();
        s.messages[0]["tool_calls"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(request_for(&s, "E000001").is_err());
        let mut s = pair_fixture();
        let duplicate = crate::reasoning::observe(1, "source_artifact", &json!({"ok":true}));
        s.observations.push(duplicate);
        assert!(request_for(&s, "E000001").is_err());
    }
    #[test]
    fn comparison_counts_array_elements_and_missing_null_separately() {
        let mut changed = Vec::new();
        differences(
            &json!({"q":["a","b"]}),
            &json!({"q":["x","y"]}),
            "",
            &mut changed,
        );
        assert_eq!(changed, vec!["/q/0", "/q/1"]);
        changed.clear();
        differences(&json!({"q":null}), &json!({}), "", &mut changed);
        assert_eq!(changed, vec!["/q"]);
        let mut s = pair_fixture();
        for (index, values) in [(0, json!(["a", "b"])), (2, json!(["x", "y"]))] {
            s.messages[index]["tool_calls"][0]["function"]["arguments"] =
                json!({"values":values}).to_string().into();
        }
        assert!(compare(
            &s,
            &json!({"baseline_evidence":"E000001","variant_evidence":"E000003"})
        )
        .is_err());
    }
    #[test]
    fn report_revalidates_native_facts_and_emits_private_request_references() {
        let mut s = pair_fixture();
        s.observations[0].native_facts.http_status = Some(200);
        s.findings.push(crate::reasoning::finding(&json!({"status":"supported","summary":"Stored status","evidence_ids":["E000001"],"limitation":"metadata only","assertions":[{"evidence_id":"E000001","kind":"http_status","value":200}]}), &s.observations).unwrap());
        let report = reproduction_report(&s);
        assert_eq!(report["findings"][0]["native_assertions_rechecked"], false);
        assert_eq!(report["findings"][0]["semantic_claim_verified"], false);
        let entry = &report["evidence"][1];
        assert_eq!(entry["stored_native_facts_match"], false);
        assert!(entry["native_facts"]["http_status"].is_null());
        let recipe = &entry["reproduction"];
        assert_eq!(recipe["request_reference"]["message_index"], 0);
        assert_eq!(recipe["response_reference"]["message_index"], 1);
        assert_eq!(recipe["automatic_replay_allowed"], false);
        assert_eq!(recipe["steps"].as_array().unwrap().len(), 6);
        let raw = s.messages[0]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap();
        assert_eq!(
            recipe["original_arguments_sha256"],
            crate::intelligence::code::hash(raw.as_bytes())
        );
        s.messages[1]["content"] = json!({"ok":true,"fresh_evidence":false}).to_string().into();
        s.observations[0].result_sha256 = None; // Legacy checkpoint freshness check.
        assert_eq!(
            reproduction_report(&s)["evidence"][1]["reproduction"]["state"],
            "blocked_historical_or_unexecuted"
        );
    }
    #[test]
    fn historical_status_cannot_revalidate_a_saved_assertion() {
        for marker in [
            json!({"cached":true}),
            json!({"cached_result":true}),
            json!({"fresh_evidence":false}),
            json!({"reused_local_copy":true}),
            json!({"reused_evidence_id":"E-old"}),
            json!({"original_evidence_id":"E-old"}),
        ] {
            let mut s = pair_fixture();
            let result = json!({"ok":true,"status":200});
            s.messages[1]["content"] = result.to_string().into();
            s.observations[0] = crate::reasoning::observe(1, "source_artifact", &result);
            s.findings.push(crate::reasoning::finding(&json!({"status":"supported","summary":"Native status","evidence_ids":["E000001"],"limitation":"Not impact","assertions":[{"evidence_id":"E000001","kind":"http_status","value":200}]}), &s.observations).unwrap());
            assert_eq!(
                reproduction_report(&s)["findings"][0]["native_assertions_rechecked"],
                true
            );
            let mut historical = result.clone();
            historical
                .as_object_mut()
                .unwrap()
                .extend(marker.as_object().unwrap().clone());
            s.messages[1]["content"] = historical.to_string().into();
            s.observations[0].result_sha256 = None;
            let report = reproduction_report(&s);
            assert_eq!(
                report["findings"][0]["native_assertions_rechecked"], false,
                "accepted {marker}"
            );
            assert!(report["evidence"][1]["native_facts"]["http_status"].is_null());
        }
    }
    #[test]
    fn coverage_does_not_recount_recent_evidence_and_preserves_failures() {
        let mut ledger = Ledger::default();
        let args = json!({"url":"https://example.test/"});
        let result = json!({"ok":true,"url":"https://example.test/"});
        for _ in 0..3 {
            ledger.record("browser", &args, &result, "E1");
        }
        ledger.record("browser", &args, &json!({"ok":false,"denied":true}), "E2");
        let row = ledger.entries.values().next().unwrap();
        assert_eq!(row.observations, 2);
        assert_eq!(row.successful_observations, 1);
        assert_eq!(row.blocked_observations, 1);
        assert_eq!(row.state, "blocked_or_failed");
        let mut large = Ledger::default();
        let scripts = (0..501)
            .map(|i| format!("https://example.test/{i}.js"))
            .collect::<Vec<_>>();
        large.record("browser", &json!({}), &json!({"scripts":scripts}), "E3");
        assert!(large.truncated);
        assert!(large.entries.values().all(|row| row.observations == 0));
    }
    #[test]
    fn automatic_native_metadata_restores_request_but_wrapper_alone_does_not() {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "fixture");
        let result = json!({"requested_url":"https://example.test/?token=SECRET","ok":true,"page":{"url":"https://example.test/","forms":[]}});
        let mut observation = crate::reasoning::observe(0, "browser_url_inspection", &result);
        observation.automatic_request = Some(crate::reasoning::AutomaticInspectionRequest {
            url: result["requested_url"].as_str().unwrap().into(),
            result_sha256: crate::intelligence::code::hash(result.to_string().as_bytes()),
        });
        s.observations.push(observation);
        s.messages.push(json!({"role":"user","content":format!("The user requested inspection of the URL above. Blue Dragon automatically collected evidence. Evidence:\n{result}")}));
        assert!(request_for(&s, "E000000").is_ok());
        s.scope.command("allow https://example.test/ read").unwrap();
        let sessions = root.path().join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        crate::session::save(&sessions, &mut s).unwrap();
        let restored = crate::session::load(&sessions, &s.id, root.path()).unwrap();
        assert!(request_for(&restored, "E000000").is_ok());
        let reviewed = crate::audit_engines::review(
            &restored,
            &json!({"engine":"form_csrf_review","evidence_id":"E000000"}),
        )
        .unwrap();
        assert_eq!(reviewed["source_evidence_id"], "E000000");
        let report = reproduction_report(&s);
        assert_eq!(
            report["evidence"][0]["input_availability"],
            "native_automatic_inspection_request"
        );
        assert!(!report.to_string().contains("SECRET"));
        s.messages[0]["content"] = s.messages[0]["content"]
            .as_str()
            .unwrap()
            .replace("SECRET", "changed")
            .into();
        assert!(request_for(&s, "E000000").is_err());
    }
    #[test]
    fn new_findings_recheck_old_checkpoint_metadata() {
        let mut s = pair_fixture();
        s.observations[0].native_facts.http_status = Some(200);
        s.observations[0].ok = true;
        s.messages[1]["content"] = json!({"ok":true,"status":200,"reused_local_copy":true})
            .to_string()
            .into();
        let args = json!({"status":"supported","summary":"Old cached status","evidence_ids":["E000001"],"limitation":"metadata only","assertions":[{"evidence_id":"E000001","kind":"http_status","value":200}]});
        assert!(crate::reasoning::finding(&args, &s.observations).is_ok());
        assert!(verified_finding(&s, &args).is_err());
    }
    #[test]
    fn changed_native_result_invalidates_saved_findings_and_engine_review() {
        let mut s = pair_fixture();
        let result = json!({"ok":true,"status":302,"redirect":{"location_redacted":"https://example.test/home"}});
        s.messages[0]["tool_calls"][0]["function"]["name"] = json!("web_fetch");
        s.messages[0]["tool_calls"][0]["function"]["arguments"] =
            json!({"url":"https://example.test/"}).to_string().into();
        s.messages[1]["content"] = result.to_string().into();
        s.observations[0] = crate::reasoning::observe(1, "web_fetch", &result);
        s.scope.command("allow https://example.test/ read").unwrap();
        let finding = json!({"status":"supported","summary":"Native redirect status","evidence_ids":["E000001"],"limitation":"Metadata only","assertions":[{"evidence_id":"E000001","kind":"http_status","value":302}]});
        s.findings.push(verified_finding(&s, &finding).unwrap());
        let engine = json!({"engine":"redirect_review","evidence_id":"E000001"});
        assert!(crate::audit_engines::review(&s, &engine).is_ok());
        assert_eq!(
            reproduction_report(&s)["findings"][0]["native_assertions_rechecked"],
            true
        );
        // Status remains the same: alteration of another native field still invalidates the evidence.
        let mut altered = result;
        altered["redirect"]["location_redacted"] = json!("https://other.test/");
        s.messages[1]["content"] = altered.to_string().into();
        assert!(verified_finding(&s, &finding).is_err());
        assert!(crate::audit_engines::review(&s, &engine).is_err());
        assert_eq!(
            reproduction_report(&s)["findings"][0]["native_assertions_rechecked"],
            false
        );
    }
    #[test]
    fn distinct_results_in_one_tool_batch_remain_pairable() {
        let mut s = pair_fixture();
        let second_call = s.messages[2]["tool_calls"][0].clone();
        s.messages[0]["tool_calls"]
            .as_array_mut()
            .unwrap()
            .push(second_call);
        s.messages.remove(2);
        s.observations[1].message_index = 2;
        assert!(request_for(&s, "E000001").is_ok());
        assert!(request_for(&s, "E000003").is_ok());
    }
    #[test]
    fn integrity_accepts_native_envelopes_and_labels_legacy_pairs() {
        for native in [json!({"ok":true,"text":"native"}), json!("scalar")] {
            let mut s = pair_fixture();
            s.observations[0] = crate::reasoning::observe(1, "source_artifact", &native);
            let mut envelope = if native.is_object() {
                native
            } else {
                json!({"result":native})
            };
            envelope["evidence_id"] = json!("E000001");
            s.messages[1]["content"] = envelope.to_string().into();
            assert!(request_for(&s, "E000001").is_ok());
            s.observations[0].result_sha256 = None;
            assert!(request_for(&s, "E000001").is_ok());
            assert_eq!(
                reproduction_report(&s)["evidence"][1]["integrity_metadata"],
                "legacy_pair_without_recorded_digests"
            );
        }
    }
    #[test]
    fn evidence_rejects_changed_request_result_and_duplicate_response() {
        let mut s = pair_fixture();
        let original: Value = serde_json::from_str(
            s.messages[0]["tool_calls"][0]["function"]["arguments"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        s.observations[0].request_sha256 = Some(crate::intelligence::code::hash(
            original.to_string().as_bytes(),
        ));
        assert!(request_for(&s, "E000001").is_ok());
        s.messages[0]["tool_calls"][0]["function"]["arguments"] =
            json!({"action":"read","artifact_id":"other","offset":0})
                .to_string()
                .into();
        assert!(request_for(&s, "E000001").is_err());
        let mut s = pair_fixture();
        s.messages[1]["content"] = json!({"ok":true,"text":"changed"}).to_string().into();
        assert!(request_for(&s, "E000001").is_err());
        let mut s = pair_fixture();
        let duplicate = s.messages[1].clone();
        s.messages.insert(2, duplicate);
        assert!(request_for(&s, "E000001").is_err());
    }
    #[test]
    fn embedded_identity_and_conversation_boundaries_are_checked_for_legacy_data() {
        let mut s = pair_fixture();
        s.observations[0].result_sha256 = None;
        s.messages[1]["content"] = json!({"ok":true,"evidence_id":"some-other-id"})
            .to_string()
            .into();
        assert!(request_for(&s, "E000001").is_err());
        let mut s = pair_fixture();
        s.messages
            .insert(1, json!({"role":"user","content":"Unrelated new turn"}));
        s.observations[0].message_index = 2;
        assert!(request_for(&s, "E000001").is_err());
    }
    #[test]
    fn verified_coverage_drops_changed_evidence_and_saved_counters() {
        let mut s = pair_fixture();
        s.messages[0]["tool_calls"][0]["function"]["arguments"] =
            json!({"url":"https://example.test/allowed?q=PRIVATE"})
                .to_string()
                .into();
        s.scope
            .command("allow https://example.test/allowed read")
            .unwrap();
        s.audit.record(
            "web_fetch",
            &json!({"url":"https://fabricated.test/"}),
            &json!({"ok":true}),
            "invented",
        );
        let report = verified_coverage(&s);
        assert_eq!(report["coverage"].as_array().unwrap().len(), 1);
        assert_eq!(report["coverage"][0]["currently_in_read_scope"], true);
        assert_eq!(report["coverage"][0]["security_test_completed"], false);
        assert!(!report.to_string().contains("PRIVATE"));
        assert!(!report.to_string().contains("fabricated"));
        s.scope.command("clear").unwrap();
        assert_eq!(
            verified_coverage(&s)["coverage"][0]["currently_in_read_scope"],
            false
        );
        s.messages[1]["content"] = json!({"ok":true,"text":"altered"}).to_string().into();
        let report = verified_coverage(&s);
        assert!(report["coverage"].as_array().unwrap().is_empty());
        assert_eq!(
            report["provenance"]["unavailable_or_mismatched_evidence"],
            1
        );
    }
    #[test]
    fn coverage_separates_unknown_partial_and_not_executed() {
        let mut ledger = Ledger::default();
        let args = json!({"url":"https://example.test/"});
        ledger.record(
            "browser",
            &args,
            &json!({"ok":false,"execution_status":"unknown"}),
            "a",
        );
        ledger.record(
            "browser",
            &args,
            &json!({"ok":true,"result":{"forms":[{"fields_truncated":true}]}}),
            "b",
        );
        ledger.record(
            "browser",
            &args,
            &json!({"execution_status":"not_executed"}),
            "c",
        );
        let row = &ledger.report()["coverage"][0];
        assert_eq!(row["unknown_observations"], 1);
        assert_eq!(row["partial_observations"], 1);
        assert_eq!(row["successful_observations"], 1);
        assert_eq!(row["blocked_observations"], 1);
        assert_eq!(row["observations"], 3);
    }
    #[test]
    fn verified_coverage_reports_skipped_large_and_historical_inputs() {
        let mut s = pair_fixture();
        let old = json!({"ok":true,"fresh_evidence":false});
        s.messages[1]["content"] = old.to_string().into();
        s.observations[0] = crate::reasoning::observe(1, "source_artifact", &old);
        s.messages[3]["content"] = json!("x".repeat(512 * 1024 + 1));
        let report = verified_coverage(&s);
        assert!(report["coverage"].as_array().unwrap().is_empty());
        assert_eq!(report["provenance"]["historical_results_skipped"], 1);
        assert_eq!(report["provenance"]["oversized_or_over_budget_skipped"], 1);
    }
    #[test]
    fn coverage_deduplicates_and_excludes_query_secrets() {
        let mut l = Ledger::default();
        for token in ["secret-a", "secret-b"] {
            l.record(
                "web_fetch",
                &json!({"url":format!("https://example.test/api?token={token}")}),
                &json!({"ok":true,"status":200}),
                "E1",
            );
        }
        let text = l.report().to_string();
        assert!(!text.contains("secret-"));
        assert_eq!(l.entries.len(), 1);
        assert!(text.contains("not_security_verified"));
    }
    #[test]
    fn report_omits_private_inputs_bodies_and_narrative() {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "fixture");
        s.messages.push(json!({"role":"assistant","tool_calls":[{"id":"c","function":{"name":"web_fetch","arguments":json!({"url":"https://example.test/api?token=private-input"}).to_string()}}]}));
        let output = json!({"ok":true,"status":200,"text":"private-response-body"});
        s.observations
            .push(crate::reasoning::observe(1, "web_fetch", &output));
        s.messages
            .push(json!({"role":"tool","tool_call_id":"c","content":output.to_string()}));
        let report = reproduction_report(&s).to_string();
        assert!(!report.contains("private-input"));
        assert!(!report.contains("private-response-body"));
        assert!(report.contains("https://example.test/api"));
    }
    #[test]
    fn pair_checks_field_changes_without_claiming_causality() {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "fixture");
        for (i, url) in [
            "https://example.test/?q=normal",
            "https://example.test/?q=test",
        ]
        .iter()
        .enumerate()
        {
            let call = format!("c{i}");
            s.messages.push(json!({"role":"assistant","tool_calls":[{"id":call,"function":{"name":"web_fetch","arguments":json!({"url":url}).to_string()}}]}));
            let result = json!({"ok":true,"status":200,"text":"not proof"});
            s.observations.push(crate::reasoning::observe(
                s.messages.len(),
                "web_fetch",
                &result,
            ));
            s.messages
                .push(json!({"role":"tool","tool_call_id":call,"content":result.to_string()}));
        }
        let args = json!({"baseline_evidence":"E000001","variant_evidence":"E000003"});
        assert_eq!(compare(&s, &args).unwrap()["causality_proven"], false);
        s.messages[2]["tool_calls"][0]["function"]["arguments"] =
            json!({"url":"https://example.test/?q=test&extra=x"})
                .to_string()
                .into();
        assert!(compare(&s, &args).is_err());
    }
}
