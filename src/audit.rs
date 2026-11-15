//! Bounded coverage and controlled evidence comparison; never a vulnerability oracle.
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
        });
        if state != "discovered_not_tested" {
            row.state = state.into();
            row.observations += 1;
        }
        if !row.evidence_ids.iter().any(|e| e == evidence) {
            if row.evidence_ids.len() >= 8 {
                row.evidence_ids.remove(0);
            }
            row.evidence_ids.push(evidence.into());
        }
    }
    fn walk(&mut self, value: &Value, kind: &str, evidence: &str, depth: usize) {
        if depth > 12 {
            self.truncated = true;
            return;
        }
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
                    self.walk(item, child, evidence, depth + 1);
                }
            }
            Value::Array(items) => {
                for item in items.iter().take(500) {
                    self.walk(item, kind, evidence, depth + 1);
                }
            }
            Value::String(url) if kind == "source" => {
                self.add(url, kind, "discovered_not_tested", evidence)
            }
            _ => {}
        }
    }
    pub fn record(&mut self, tool: &str, args: &Value, result: &Value, evidence: &str) {
        if result["cached_result"] == true || result["reused_local_copy"] == true {
            return;
        }
        if matches!(tool, "browser" | "browser_url_inspection") {
            self.walk(result, "page", evidence, 0);
        }
        let url = args["url"]
            .as_str()
            .or_else(|| result.pointer("/result/url").and_then(Value::as_str))
            .or_else(|| result["requested_url"].as_str());
        if let Some(url) = url {
            let state = if result["denied"] == true || result["ok"] == false {
                "blocked_or_failed"
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
        json!({"coverage":self.entries.values().collect::<Vec<_>>(),"truncated":self.truncated,"max_entries":500,"limits":"Discovery is not testing. Repeated query values collapse to redacted locations. Form submissions, endpoint exhaustiveness, login identity, semantic paths and exploitability are not inferred. Raw request/response bodies, cookies, authorization headers and fragments omitted; paths may still need human redaction before sharing."})
    }
}
fn request_for(session: &crate::session::Session, id: &str) -> Result<(String, Value, Value)> {
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
        bail!("Evidence is not a paired tool result");
    }
    let call_id = message["tool_call_id"]
        .as_str()
        .context("Missing call ID")?;
    for prior in session.messages[..observation.message_index].iter().rev() {
        if prior["role"] == "assistant" {
            let call = prior["tool_calls"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|c| c["id"] == call_id)
                .context("Unpaired evidence")?;
            let mut args: Value = serde_json::from_str(
                call["function"]["arguments"]
                    .as_str()
                    .context("Missing arguments")?,
            )?;
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
            let result =
                serde_json::from_str(message["content"].as_str().context("Missing result")?)?;
            return Ok((observation.tool.clone(), args, result));
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
            differences(
                a.get(k).unwrap_or(&Value::Null),
                b.get(k).unwrap_or(&Value::Null),
                &format!("{path}/{}", k.replace('~', "~0").replace('/', "~1")),
                out,
            );
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
            || value["cached_result"] == true
            || value["reused_local_copy"] == true
            || (value["ok"] == false && value["status"].as_u64().is_none())
        {
            bail!("Denied/unknown/cached results cannot form a fresh controlled pair");
        }
    }
    Ok(
        json!({"ok":true,"baseline_evidence":baseline,"variant_evidence":variant,"tool":tool,"changed_argument_fields":changed,"baseline_native":crate::reasoning::claims::extract(&tool,&ar),"variant_native":crate::reasoning::claims::extract(&tool,&br),"baseline_result_sha256":crate::intelligence::code::hash(ar.to_string().as_bytes()),"variant_result_sha256":crate::intelligence::code::hash(br.to_string().as_bytes()),"causality_proven":false,"verdict":"paired_observations_only","limits":"One serialized argument field changed; this does not prove only one semantic factor changed. Account/session, timing, server state and site errors may differ. Repeat controls and inspect native evidence; HTTP 200 and tool success do not prove a vulnerability. Input values and response bodies intentionally omitted."}),
    )
}
pub fn reproduction_report(session: &crate::session::Session) -> Value {
    let evidence=session.observations.iter().rev().take(100).map(|entry| {
        let result=session.messages.get(entry.message_index);
        let hash=result.and_then(|m|m["content"].as_str()).map(|s|crate::intelligence::code::hash(s.as_bytes()));
        let request=request_for(session,&entry.id).ok();
        let input_hash=request.as_ref().map(|(_,args,_)|crate::intelligence::code::hash(args.to_string().as_bytes()));
        let target=request.as_ref().and_then(|(_,args,_)|Some(format!("{}{}",args.pointer("/url/origin")?.as_str()?,args.pointer("/url/path")?.as_str()?)));
        json!({"target":target,"input_sha256":input_hash,"evidence_id":entry.id,"tool":entry.tool,"message_index":entry.message_index,"native_facts":entry.native_facts,"observed_ok":entry.ok,"result_sha256":hash,"local_reproduction":"Use recall_context at message_index to inspect original result; paired assistant call contains exact input. Do not replay unknown or denied actions."})
    }).collect::<Vec<_>>();
    let findings=session.findings.iter().enumerate().map(|(i,f)|json!({"finding_index":i,"status":f.status,"evidence_ids":f.evidence_ids,"assertions":f.assertions,"assertion_verification":f.assertion_verification,"hypothesis_fields_present":!f.next_test.is_empty()&&!f.confirms.is_empty()&&!f.refutes.is_empty(),"narrative":"Private model-authored summary/test plan remains in the local session; review and redact before sharing."})).collect::<Vec<_>>();
    json!({"schema":1,"session_id":session.id,"coverage":session.audit.report(),"evidence":evidence,"evidence_omitted":session.observations.len().saturating_sub(100),"findings":findings,"conditions":{"scope_enforced":session.scope.enabled,"login_identity_verified":false,"same_server_state_verified":false,"raw_bodies_and_query_values_omitted":true},"reproduction_limits":"Redacted evidence index, not a complete standalone exploit report. URLs omit all query values and fragments; raw inputs, request/response bodies and model prose omitted by default. Obtain and review the referenced local input/result before reproducing. Native status 200 or a successful tool does not prove exploitability. Paths may need human redaction."})
}
pub fn definitions() -> Vec<Value> {
    vec![
        json!({"type":"function","function":{"name":"audit_coverage","description":"Read bounded deduplicated discovered/observed/blocked coverage. Does not crawl or claim security coverage.","parameters":{"type":"object","properties":{},"additionalProperties":false}}}),
        json!({"type":"function","function":{"name":"compare_observations","description":"Compare two paired tool evidence IDs only when exactly one serialized argument field differs; returns native metadata and hashes, never causal or vulnerability proof. Account and server state stability are unverified.","parameters":{"type":"object","properties":{"baseline_evidence":{"type":"string"},"variant_evidence":{"type":"string"}},"required":["baseline_evidence","variant_evidence"],"additionalProperties":false}}}),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
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
