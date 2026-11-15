//! Planned test cells with revalidated evidence versions; no execution or percentages.
use crate::session::Session;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    cells: Vec<Cell>,
    #[serde(default)]
    hypothesis_groups: Vec<HypothesisGroup>,
}
#[derive(Debug, Serialize, Deserialize)]
struct HypothesisGroup {
    id: String,
    alternatives: Vec<FindingLink>,
    case_ids: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
struct Cell {
    id: String,
    target: String,
    method: String,
    account_label: String,
    scenario: String,
    versions: Vec<Version>,
    #[serde(default)]
    findings: Vec<FindingLink>,
}
#[derive(Debug, Serialize, Deserialize)]
struct FindingLink {
    index: usize,
    sha256: String,
}
#[derive(Debug, Serialize, Deserialize)]
struct Version {
    evidence_id: String,
    request_sha256: String,
    result_sha256: String,
}
fn hash(s: &str) -> String {
    crate::intelligence::code::hash(s.as_bytes())
}
fn target(raw: &str) -> Result<String> {
    if raw.len() > 2048 {
        bail!("Target too long");
    }
    let u = reqwest::Url::parse(raw)?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
    {
        bail!("Use origin/path only; no credentials, query or fragment");
    }
    Ok(u.to_string())
}
fn label(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
fn level(scenario: &str) -> &'static str {
    if scenario == "http_response" {
        "read"
    } else {
        "interact"
    }
}

fn required_level(method: &str, scenario: &str) -> &'static str {
    if matches!(method, "GET" | "HEAD" | "OPTIONS") {
        level(scenario)
    } else {
        "mutate"
    }
}

/// Attached only to native GET producers before the result digest is recorded.
/// This is a scope-label binding, never authentication/session identity proof.
pub fn stamp(scope: &crate::scope::Policy, tool: &str, args: &Value, result: &mut Value) {
    let scenario = match tool {
        "web_fetch" => "http_response",
        "run_local_lab" if args["action"] == "run" => {
            args["scenario"].as_str().unwrap_or("literal_marker")
        }
        _ => return,
    };
    let Some(raw) = args["url"].as_str() else {
        return;
    };
    let Ok(url) = target(raw) else { return };
    let Some(account) = scope.declared_account(&url, level(scenario)) else {
        return;
    };
    result["test_coverage_context"] = json!({"schema":1,"account_tag":hash(&account),"method":"GET","scenario":scenario,"target":url,"authentication_verified":false});
}

fn checked(
    session: &Session,
    cell: &Cell,
    version: &Version,
    labs: &Value,
) -> Result<&'static str> {
    let observation = session
        .observations
        .iter()
        .find(|o| o.id == version.evidence_id)
        .context("Evidence unavailable")?;
    let content = session
        .messages
        .get(observation.message_index)
        .and_then(|m| m["content"].as_str())
        .context("Missing result")?;
    if content.len() > 512 * 1024 {
        bail!("Evidence exceeds per-result budget");
    }
    if observation.request_sha256.as_deref() != Some(&version.request_sha256)
        || observation.result_sha256.as_deref() != Some(&version.result_sha256)
    {
        bail!("Evidence version changed");
    }
    if let Some(call) = session.messages[..observation.message_index]
        .iter()
        .rev()
        .find(|m| m["role"] == "assistant")
    {
        if call["tool_calls"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|c| {
                c["function"]["arguments"]
                    .as_str()
                    .is_some_and(|a| a.len() > 8192)
            })
        {
            bail!("Request arguments exceed revalidation budget");
        }
    }
    let (tool, args, result) = crate::audit::request_for(session, &version.evidence_id)?;
    if crate::reasoning::claims::is_historical(&result) {
        bail!("Cached or historical substitute is not a new test");
    }
    let url = reqwest::Url::parse(&cell.target)?;
    if args["url"]
        != json!({"origin":url.origin().ascii_serialization(),"path":url.path(),"query":{},"fragment":null})
    {
        bail!("Request target mismatch");
    }
    let context = &result["test_coverage_context"];
    if context["schema"] != 1
        || context["account_tag"] != hash(&cell.account_label)
        || context["method"] != cell.method
        || context["scenario"] != cell.scenario
        || context["target"] != cell.target
        || context["authentication_verified"] != false
    {
        bail!("Missing or mismatched native account/method/scenario binding");
    }
    if cell.method != "GET" {
        bail!("No verified native producer for this method");
    }
    if tool == "web_fetch" && cell.scenario == "http_response" {
        if result["status"] == "denied by user"
            || result["denied"] == true
            || result["execution_status"] == "not_executed"
        {
            return Ok("blocked");
        }
        if result["execution_status"] == "unknown" {
            return Ok("inconclusive");
        }
        // A captured HTTP error is still a performed response-capture test,
        // never a security success. Body completeness must be explicit.
        if result["status"]
            .as_u64()
            .is_some_and(|s| (100..600).contains(&s))
            && result["truncated"] == false
            && result["text"].is_string()
        {
            return Ok("tested");
        }
        return Ok("inconclusive");
    }
    if tool == "run_local_lab" && matches!(cell.scenario.as_str(), "literal_marker" | "inert_html")
    {
        let run = labs["runs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["evidence_id"] == version.evidence_id)
            .context("Lab evidence not independently validated or outside report budget")?;
        if run["canonical_recipe"]["scenario"] != cell.scenario {
            bail!("Lab scenario mismatch");
        }
        let verdict = run["recomputed_observation"].as_str().unwrap_or("");
        return Ok(if verdict == "not_executed_denied" {
            "blocked"
        } else if verdict.starts_with("inconclusive") {
            "inconclusive"
        } else {
            "tested"
        });
    }
    bail!("Unsupported evidence producer/scenario")
}

pub fn apply(session: &mut Session, args: &Value) -> Result<Value> {
    match args["action"].as_str().unwrap_or("") {
        "plan" => {
            let target = target(args["url"].as_str().context("url required")?)?;
            let method = args["method"].as_str().context("method required")?;
            let account = args["account_label"]
                .as_str()
                .context("account_label required")?;
            let scenario = args["scenario"].as_str().context("scenario required")?;
            if !matches!(
                method,
                "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
            ) || !label(account)
                || !matches!(scenario, "http_response" | "literal_marker" | "inert_html")
            {
                bail!("Invalid method, opaque account label, or supported scenario");
            }
            let id = hash(&json!([target, method, account, scenario]).to_string());
            if session.test_coverage.cells.iter().any(|c| c.id == id) {
                return Ok(json!({"case_id":id,"already_planned":true}));
            }
            if session.test_coverage.cells.len() >= 100 {
                bail!("100 planned cases maximum");
            }
            session.test_coverage.cells.push(Cell {
                id: id.clone(),
                target,
                method: method.into(),
                account_label: account.into(),
                scenario: scenario.into(),
                versions: Vec::new(),
                findings: Vec::new(),
            });
            Ok(
                json!({"case_id":id,"state":"not-tested","network_requests":0,"authorization_granted":false}),
            )
        }
        "record" => {
            let id = args["case_id"].as_str().context("case_id required")?;
            let evidence = args["evidence_id"]
                .as_str()
                .context("evidence_id required")?;
            let cell = session
                .test_coverage
                .cells
                .iter()
                .find(|c| c.id == id)
                .context("Unknown case")?;
            let obs = session
                .observations
                .iter()
                .find(|o| o.id == evidence)
                .context("Unknown evidence")?;
            let version = Version {
                evidence_id: evidence.into(),
                request_sha256: obs
                    .request_sha256
                    .clone()
                    .context("Native request digest required")?,
                result_sha256: obs
                    .result_sha256
                    .clone()
                    .context("Native result digest required")?,
            };
            let state = checked(session, cell, &version, &crate::lab_report::report(session))?;
            if cell.versions.iter().any(|v| v.evidence_id == evidence) {
                return Ok(json!({"case_id":id,"state":state,"already_recorded":true}));
            }
            if cell.versions.len() >= 8 {
                bail!("Eight evidence versions maximum per case; no silent eviction");
            }
            session
                .test_coverage
                .cells
                .iter_mut()
                .find(|c| c.id == id)
                .unwrap()
                .versions
                .push(version);
            Ok(json!({"case_id":id,"state":state,"network_requests":0}))
        }
        "link_finding" => {
            let id = args["case_id"].as_str().context("case_id required")?;
            let index = usize::try_from(
                args["finding_index"]
                    .as_u64()
                    .context("finding_index required")?,
            )?;
            let finding = session
                .findings
                .get(index)
                .context("Unknown finding index")?;
            let digest = hash(&serde_json::to_string(finding)?);
            let cell = session
                .test_coverage
                .cells
                .iter_mut()
                .find(|c| c.id == id)
                .context("Unknown case")?;
            if !cell
                .findings
                .iter()
                .any(|f| f.index == index && f.sha256 == digest)
            {
                if cell.findings.len() >= 8 {
                    bail!("Eight finding links maximum");
                }
                cell.findings.push(FindingLink {
                    index,
                    sha256: digest,
                });
            }
            Ok(
                json!({"case_id":id,"finding_index":index,"semantic_claim_verified":false,"network_requests":0}),
            )
        }
        "group_hypotheses" => {
            let id = args["group_id"].as_str().context("group_id required")?;
            if !label(id) { bail!("Use an opaque group ID of at most 40 characters"); }
            let indices = args["finding_indices"].as_array().context("finding_indices required")?;
            let cases = args["case_ids"].as_array().context("case_ids required")?;
            if !(2..=4).contains(&indices.len()) || !(1..=8).contains(&cases.len()) {
                bail!("Require 2–4 distinct alternatives and 1–8 distinct planned cases");
            }
            if session.test_coverage.hypothesis_groups.len() >= 20 || session.test_coverage.hypothesis_groups.iter().any(|g|g.id==id) {
                bail!("Group ID already exists or 20-group limit reached; no overwrite");
            }
            let mut alternatives = Vec::<FindingLink>::new();
            for index in indices {
                let index = usize::try_from(index.as_u64().context("Invalid finding index")?)?;
                let f = session.findings.get(index).context("Unknown finding")?;
                if f.status != "hypothesis" || alternatives.iter().any(|a|a.index==index) {
                    bail!("Distinct hypothesis findings required; no promotion or duplicate alternatives");
                }
                alternatives.push(FindingLink {index, sha256:hash(&serde_json::to_string(f)?)});
            }
            let mut case_ids = Vec::<String>::new();
            for case in cases {
                let case = case.as_str().context("Invalid case ID")?;
                if case_ids.iter().any(|c|c==case) || !session.test_coverage.cells.iter().any(|c|c.id==case) {
                    bail!("Distinct existing planned cases required");
                }
                case_ids.push(case.into());
            }
            session.test_coverage.hypothesis_groups.push(HypothesisGroup {id:id.into(), alternatives, case_ids});
            Ok(json!({"group_id":id,"network_requests":0,"hypotheses_promoted":0,"semantic_discrimination_verified":false}))
        }
        "report" => Ok(report(session)),
        _ => bail!(
            "Use plan, record, link_finding, group_hypotheses or report; states are derived, never user supplied"
        ),
    }
}

pub fn report(session: &Session) -> Value {
    let labs = crate::lab_report::report(session);
    let mut rows = Vec::new();
    let mut budget = 4 * 1024 * 1024usize;
    for cell in session.test_coverage.cells.iter().take(100) {
        let current = session
            .scope
            .declared_account(&cell.target, required_level(&cell.method, &cell.scenario))
            .as_deref()
            == Some(cell.account_label.as_str());
        let mut versions = Vec::new();
        let mut captures = Vec::new();
        for version in cell.versions.iter().take(8) {
            let size = session
                .observations
                .iter()
                .find(|o| o.id == version.evidence_id)
                .and_then(|o| session.messages.get(o.message_index))
                .and_then(|m| m["content"].as_str())
                .map(str::len)
                .unwrap_or(0);
            let state = if size > budget {
                "inconclusive"
            } else {
                budget -= size;
                checked(session, cell, version, &labs).unwrap_or("inconclusive")
            };
            // Read only already revalidated complete captures; never expose body hashes/prose.
            let capture = if state == "tested" && cell.scenario == "http_response" {
                crate::audit::request_for(session, &version.evidence_id)
                    .ok()
                    .and_then(|(_, _, r)| {
                        let index = session
                            .observations
                            .iter()
                            .find(|o| o.id == version.evidence_id)?
                            .message_index;
                        Some((index, r["status"].as_u64()?, hash(r["text"].as_str()?)))
                    })
            } else {
                None
            };
            captures.push(capture);
            versions.push(json!({"evidence_id":version.evidence_id,"request_sha256":version.request_sha256,"result_sha256":version.result_sha256,"state":state}));
        }
        let retest_deltas = captures.windows(2).enumerate().map(|(i,pair)| {
            let comparable = match (&pair[0], &pair[1]) {
                (Some(a),Some(b)) => a.0 < b.0 && cell.versions[i].request_sha256 == cell.versions[i+1].request_sha256,
                _ => false,
            };
            let (status_changed, decoded_text_changed) = if comparable {
                let a=pair[0].as_ref().unwrap(); let b=pair[1].as_ref().unwrap();
                (json!(a.1!=b.1),json!(a.2!=b.2))
            } else { (Value::Null,Value::Null) };
            json!({"before_evidence_id":cell.versions[i].evidence_id,"after_evidence_id":cell.versions[i+1].evidence_id,"comparable":comparable,"status_changed":status_changed,"decoded_text_changed":decoded_text_changed,"causality_verified":false,"reason":if comparable {"Same recorded request, ordered complete captures; server/account state may differ"} else {"Requires ordered complete HTTP captures with identical recorded request; attachment order alone is not time"}})
        }).collect::<Vec<_>>();
        let state = versions
            .last()
            .map(|v| v["state"].clone())
            .unwrap_or_else(|| json!(if current { "not-tested" } else { "blocked" }));
        let findings=cell.findings.iter().take(8).map(|link| {
            let finding=session.findings.get(link.index);
            let unchanged=finding.is_some_and(|f|serde_json::to_string(f).is_ok_and(|v|hash(&v)==link.sha256));
            let citations_valid=unchanged && finding.is_some_and(|f|!f.evidence_ids.is_empty() && f.evidence_ids.iter().all(|id|versions.iter().any(|v|v["evidence_id"]==*id && v["state"]=="tested")));
            json!({"finding_index":link.index,"finding_sha256":link.sha256,"finding_unchanged":unchanged,"native_citations_revalidated":citations_valid,"semantic_claim_verified":false,"narrative":"Model-authored hypothesis stays in the local session; linking does not prove or refute it"})
        }).collect::<Vec<_>>();
        let prior_tested = versions
            .iter()
            .filter(|v| v["state"] == "tested")
            .map(|v| v["evidence_id"].clone())
            .collect::<Vec<_>>();
        let native_target_supported = if cell.scenario == "http_response" {
            reqwest::Url::parse(&cell.target).is_ok_and(|u| {
                u.scheme() == "https"
                    || matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
            })
        } else {
            crate::lab_runner::target(&cell.target).is_ok()
        };
        let recipe = if cell.method != "GET" || !native_target_supported {
            json!({"available":false,"reason":"No supported native producer for method/target/scenario"})
        } else if cell.scenario == "http_response" {
            json!({"available":true,"tool":"web_fetch","arguments":{"url":cell.target},"method":"GET","browser_session_used":false})
        } else {
            json!({"available":true,"tool":"run_local_lab","arguments":{"action":"run","url":cell.target,"scenario":cell.scenario},"run_id":"MUST_SUPPLY_NEW_ID_AFTER_NEW_USER_REQUEST","sequence":["baseline","variant","repeated_control"],"method":"GET"})
        };
        rows.push(json!({"case_id":cell.id,"target":cell.target,"method":cell.method,"account_label":cell.account_label,"scenario":cell.scenario,"state":state,"evidence_versions":versions,"retest_deltas":retest_deltas,"linked_findings":findings,"retained_tested_evidence":prior_tested,"retest":{"recipe":recipe,"current_scope_and_label_match":current,"requires_new_user_request":true,"requires_human_approval":true,"automatic_execution_allowed":false,"account_and_server_state_must_be_rechecked":true},"current_scope_and_label_match":current,"authentication_verified":false,"automatic_retry_allowed":false,"security_property_verified":false}));
    }
    let groups = session.test_coverage.hypothesis_groups.iter().take(20).map(|group| {
        let cases = group.case_ids.iter().take(8).map(|id| {
            let row=rows.iter().find(|r|r["case_id"]==*id);
            json!({"case_id":id,"state":row.map(|r|r["state"].clone()).unwrap_or(json!("inconclusive")),"retest":row.map(|r|r["retest"].clone()),"retest_deltas":row.map(|r|r["retest_deltas"].clone())})
        }).collect::<Vec<_>>();
        let alternatives = group.alternatives.iter().take(4).map(|link| {
            let f=session.findings.get(link.index);
            let unchanged=f.is_some_and(|f|serde_json::to_string(f).is_ok_and(|v|hash(&v)==link.sha256));
            let citations=if unchanged { f.map(|f|f.evidence_ids.iter().take(32).map(|id| {
                let matched=rows.iter().filter(|r|group.case_ids.iter().any(|c|r["case_id"]==*c)).any(|r|r["evidence_versions"].as_array().is_some_and(|vs|vs.iter().any(|v|v["evidence_id"]==*id && v["state"]=="tested")));
                json!({"evidence_id":id,"revalidated_in_group_cases":matched})
            }).collect::<Vec<_>>()).unwrap_or_default() } else { Vec::new() };
            json!({"finding_index":link.index,"finding_unchanged":unchanged,"citations":citations,"citations_omitted":f.map(|f|f.evidence_ids.len().saturating_sub(32)).unwrap_or(0),"semantic_outcome":"unresolved","verification_gap":if unchanged {"Evaluate the original confirms/refutes criteria against observations; citation validity does not distinguish rivals"} else {"Pinned hypothesis changed or is missing"}})
        }).collect::<Vec<_>>();
        json!({"group_id":group.id,"alternatives":alternatives,"cases":cases,"winner":null,"automatic_execution_allowed":false,"semantic_discrimination_verified":false})
    }).collect::<Vec<_>>();
    json!({"schema":1,"cases":rows,"hypothesis_groups":groups,"hypothesis_groups_omitted":session.test_coverage.hypothesis_groups.len().saturating_sub(20),"planned_cases_omitted":session.test_coverage.cells.len().saturating_sub(100),"network_requests":0,"fresh_evidence":false,"coverage_percentage":null,"limits":"Only explicitly planned cells, not an endpoint inventory or denominator. tested means the named bounded response/marker scenario produced complete observations, not safety, vulnerability, identity or causality. Latest attached evidence version determines state; failed revalidation is inconclusive. Scope labels are user declarations, never verified login identity. Historical tests remain visible after scope revocation but authorize no action. No raw bodies, headers, query values or model prose. Paths and account aliases still need human review before sharing. Checkpoint hashes detect inconsistencies, not whole-checkpoint forgery. 100 cells, 8 versions/cell, 4MiB revalidation budget; lab report has its own recent-evidence limits."})
}

pub fn definition() -> Value {
    json!({"type":"function","function":{"name":"test_coverage","description":"Plan a bounded test cell, attach native evidence, group 2-4 competing existing hypotheses with planned cases, or report same-request HTTP retest changes and derived not-tested/blocked/inconclusive/tested states. Never executes tests or grants scope. tested is scenario observation, not security proof. Account labels must match explicit user scope; no credentials. HTTP methods beyond GET can be planned but have no qualifying producer yet.","parameters":{"type":"object","properties":{"action":{"type":"string","enum":["plan","record","link_finding","group_hypotheses","report"]},"url":{"type":"string"},"method":{"type":"string"},"account_label":{"type":"string"},"scenario":{"type":"string","enum":["http_response","literal_marker","inert_html"]},"case_id":{"type":"string"},"evidence_id":{"type":"string"},"finding_index":{"type":"integer","minimum":0},"group_id":{"type":"string"},"finding_indices":{"type":"array","minItems":2,"maxItems":4,"items":{"type":"integer","minimum":0}},"case_ids":{"type":"array","minItems":1,"maxItems":8,"items":{"type":"string"}}},"required":["action"],"additionalProperties":false}}})
}

#[cfg(test)]
mod tests {
    use super::*;
    const URL: &str = "https://example.test/app";
    fn session() -> Session {
        let mut s = crate::session::new(Path::new("/fixture"), "fixture");
        s.scope
            .command("allow https://example.test/app interact account-a")
            .unwrap();
        s
    }
    use std::path::Path;
    fn plan(s: &mut Session, url: &str, method: &str, account: &str, scenario: &str) -> String {
        apply(s,&json!({"action":"plan","url":url,"method":method,"account_label":account,"scenario":scenario,"state":"tested"})).unwrap()["case_id"].as_str().unwrap().into()
    }
    fn evidence(s: &mut Session, tool: &str, args: Value, mut result: Value) -> String {
        stamp(&s.scope, tool, &args, &mut result);
        let call = format!("c{}", s.messages.len());
        s.messages.push(json!({"role":"assistant","tool_calls":[{"id":call,"function":{"name":tool,"arguments":args.to_string()}}]}));
        let mut obs = crate::reasoning::observe(s.messages.len(), tool, &result);
        obs.request_sha256 = Some(hash(&args.to_string()));
        let id = obs.id.clone();
        s.observations.push(obs);
        s.messages
            .push(json!({"role":"tool","tool_call_id":call,"content":result.to_string()}));
        id
    }
    fn record(s: &mut Session, id: &str, e: &str) -> Result<Value> {
        apply(s, &json!({"action":"record","case_id":id,"evidence_id":e}))
    }
    fn response() -> Value {
        json!({"ok":true,"status":200,"text":"PRIVATE_BODY_SENTINEL","content_type":"PRIVATE_HEADER_SENTINEL","truncated":false})
    }

    #[test]
    fn retest_deltas_compare_only_ordered_complete_identical_requests() {
        let mut s = session();
        let id = plan(&mut s, URL, "GET", "account-a", "http_response");
        let first = evidence(&mut s, "web_fetch", json!({"url":URL}), response());
        record(&mut s, &id, &first).unwrap();
        let second = evidence(&mut s, "web_fetch", json!({"url":URL}), response());
        record(&mut s, &id, &second).unwrap();
        let third = evidence(
            &mut s,
            "web_fetch",
            json!({"url":URL}),
            json!({"status":404,"text":"PRIVATE_CHANGED","truncated":false}),
        );
        record(&mut s, &id, &third).unwrap();
        let r = report(&s);
        let deltas = &r["cases"][0]["retest_deltas"];
        assert_eq!(deltas[0]["comparable"], true);
        assert_eq!(deltas[0]["decoded_text_changed"], false);
        assert_eq!(deltas[1]["status_changed"], true);
        assert_eq!(deltas[1]["decoded_text_changed"], true);
        assert_eq!(deltas[1]["causality_verified"], false);
        assert!(!r.to_string().contains("PRIVATE_"));
        // A changed request option is not a same-request retest, even if the cell matches.
        let fourth = evidence(
            &mut s,
            "web_fetch",
            json!({"url":URL,"max_bytes":1}),
            response(),
        );
        record(&mut s, &id, &fourth).unwrap();
        assert_eq!(
            report(&s)["cases"][0]["retest_deltas"][2]["comparable"],
            false
        );
        s.test_coverage.cells[0].versions.swap(0, 1);
        assert_eq!(
            report(&s)["cases"][0]["retest_deltas"][0]["comparable"],
            false
        );
        let index = s
            .observations
            .iter()
            .find(|o| o.id == third)
            .unwrap()
            .message_index;
        s.messages[index]["content"] = json!("{}");
        let r = report(&s);
        assert_eq!(r["cases"][0]["retest_deltas"][1]["comparable"], false);
        assert!(r["cases"][0]["retest_deltas"][1]["decoded_text_changed"].is_null());
    }
    fn hypothesis(s: &mut Session, evidence: &str) {
        let ids = if evidence.is_empty() {
            vec![]
        } else {
            vec![evidence]
        };
        s.findings.push(crate::reasoning::finding(&json!({"status":"hypothesis","summary":"PRIVATE_RIVAL","evidence_ids":ids,"limitation":"fixture only","rationale":"PRIVATE_REASON","next_test":"repeat with control","confirms":"different response","refutes":"same response"}),&s.observations).unwrap());
    }
    #[test]
    fn rival_groups_pin_alternatives_and_revalidate_citations_without_promotion() {
        let mut s = session();
        let id = plan(&mut s, URL, "GET", "account-a", "http_response");
        let e = evidence(&mut s, "web_fetch", json!({"url":URL}), response());
        record(&mut s, &id, &e).unwrap();
        hypothesis(&mut s, &e);
        hypothesis(&mut s, "");
        let args = json!({"action":"group_hypotheses","group_id":"rivals","finding_indices":[0,1],"case_ids":[id]});
        apply(&mut s, &args).unwrap();
        assert!(apply(&mut s, &args).is_err());
        let mut s: Session = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        s.scope.command("clear").unwrap();
        let r = report(&s);
        let g = &r["hypothesis_groups"][0];
        assert!(g["winner"].is_null());
        assert_eq!(
            g["alternatives"][0]["citations"][0]["revalidated_in_group_cases"],
            true
        );
        assert_eq!(g["alternatives"][1]["semantic_outcome"], "unresolved");
        assert_eq!(
            g["cases"][0]["retest"]["current_scope_and_label_match"],
            false
        );
        assert!(!r.to_string().contains("PRIVATE_"));
        s.findings[0].summary = "changed".into();
        let r = report(&s);
        assert_eq!(
            r["hypothesis_groups"][0]["alternatives"][0]["finding_unchanged"],
            false
        );
        assert!(r["hypothesis_groups"][0]["alternatives"][0]["citations"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(s.findings[1].status, "hypothesis");
    }
    #[test]
    fn rival_groups_reject_missing_duplicate_and_oversized_inputs_atomically() {
        let mut s = session();
        let id = plan(&mut s, URL, "GET", "account-a", "http_response");
        hypothesis(&mut s, "");
        hypothesis(&mut s, "");
        for (indices, cases) in [
            (json!([0, 0]), json!([id])),
            (json!([0, 99]), json!([id])),
            (json!([0, 1]), json!([id, id])),
            (json!([0, 1]), json!(["missing"])),
            (json!([0]), json!([id])),
        ] {
            assert!(apply(&mut s,&json!({"action":"group_hypotheses","group_id":"rivals","finding_indices":indices,"case_ids":cases})).is_err());
            assert!(s.test_coverage.hypothesis_groups.is_empty());
        }
        for n in 0..20 {
            apply(&mut s,&json!({"action":"group_hypotheses","group_id":format!("g{n}"),"finding_indices":[0,1],"case_ids":[id]})).unwrap();
        }
        assert!(apply(&mut s,&json!({"action":"group_hypotheses","group_id":"overflow","finding_indices":[0,1],"case_ids":[id]})).is_err());
    }

    #[test]
    fn cell_dimensions_deduplicate_without_promoting_plans() {
        let mut s = session();
        let id = plan(&mut s, URL, "GET", "account-a", "http_response");
        assert_eq!(id, plan(&mut s, URL, "GET", "account-a", "http_response"));
        for (u, m, a, c) in [
            (
                "https://example.test/app/other",
                "GET",
                "account-a",
                "http_response",
            ),
            (URL, "POST", "account-a", "http_response"),
            (URL, "GET", "account-b", "http_response"),
            (URL, "GET", "account-a", "literal_marker"),
        ] {
            assert_ne!(id, plan(&mut s, u, m, a, c));
        }
        let r = report(&s);
        assert_eq!(r["cases"].as_array().unwrap().len(), 5);
        assert_eq!(r["cases"][0]["state"], "not-tested");
        assert_eq!(r["cases"][3]["state"], "blocked");
        assert!(r["coverage_percentage"].is_null());
        assert_eq!(r["network_requests"], 0);
    }

    #[test]
    fn native_versions_survive_restart_scope_revocation_and_hide_bodies() {
        let mut s = session();
        let id = plan(&mut s, URL, "GET", "account-a", "http_response");
        let e = evidence(&mut s, "web_fetch", json!({"url":URL}), response());
        assert_eq!(record(&mut s, &id, &e).unwrap()["state"], "tested");
        record(&mut s, &id, &e).unwrap();
        assert_eq!(s.test_coverage.cells[0].versions.len(), 1);
        let mut s: Session = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        s.scope.command("clear").unwrap();
        let r = report(&s);
        assert_eq!(r["cases"][0]["state"], "tested");
        assert_eq!(r["cases"][0]["current_scope_and_label_match"], false);
        assert_eq!(r["cases"][0]["automatic_retry_allowed"], false);
        assert!(!r.to_string().contains("PRIVATE_"));
        assert_eq!(r["cases"][0]["authentication_verified"], false);
        let args = json!({"url":URL});
        let mut result = response();
        stamp(&s.scope, "web_fetch", &args, &mut result);
        assert!(result.get("test_coverage_context").is_none());
    }

    #[test]
    fn blocked_unknown_truncated_and_error_responses_remain_distinct() {
        let mut s = session();
        let id = plan(&mut s, URL, "GET", "account-a", "http_response");
        for (result, state) in [
            (json!({"ok":false,"status":"denied by user"}), "blocked"),
            (
                json!({"execution_status":"unknown","status":200,"text":"x","truncated":false}),
                "inconclusive",
            ),
            (
                json!({"status":200,"text":"x","truncated":true}),
                "inconclusive",
            ),
            (
                json!({"ok":false,"status":404,"text":"not found","truncated":false}),
                "tested",
            ),
        ] {
            let e = evidence(&mut s, "web_fetch", json!({"url":URL}), result);
            assert_eq!(record(&mut s, &id, &e).unwrap()["state"], state);
            assert_eq!(report(&s)["cases"][0]["state"], state);
        }
        assert_eq!(
            report(&s)["cases"][0]["evidence_versions"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }

    #[test]
    fn mismatched_missing_cached_and_modified_evidence_cannot_qualify() {
        let mut s = session();
        let id = plan(&mut s, URL, "GET", "account-a", "http_response");
        let e = evidence(&mut s, "web_fetch", json!({"url":URL}), response());
        for (method, account, scenario) in [
            ("POST", "account-a", "http_response"),
            ("GET", "account-b", "http_response"),
            ("GET", "account-a", "inert_html"),
        ] {
            let other = plan(&mut s, URL, method, account, scenario);
            assert!(record(&mut s, &other, &e).is_err());
        }
        record(&mut s, &id, &e).unwrap();
        s.messages[1]["content"] = json!("{}");
        assert_eq!(report(&s)["cases"][0]["state"], "inconclusive");
        assert!(record(&mut s, &id, &e).is_err());
        for marker in ["cached", "cached_result", "reused_local_copy"] {
            let mut r = response();
            r[marker] = json!(true);
            let e = evidence(&mut s, "web_fetch", json!({"url":URL}), r);
            assert!(record(&mut s, &id, &e).is_err());
        }
        let e = evidence(&mut s, "web_fetch", json!({"url":URL}), response());
        s.observations.last_mut().unwrap().request_sha256 = None;
        assert!(record(&mut s, &id, &e).is_err());
        let e = evidence(
            &mut s,
            "web_fetch",
            json!({"url":"https://example.test/app/other"}),
            response(),
        );
        assert!(record(&mut s, &id, &e).is_err());
    }

    #[test]
    fn scoped_account_changes_ambiguity_and_private_urls_are_rejected() {
        let mut s = session();
        let id = plan(&mut s, URL, "GET", "account-a", "http_response");
        s.scope
            .command("allow https://example.test/app interact account-b")
            .unwrap();
        let e = evidence(&mut s, "web_fetch", json!({"url":URL}), response());
        assert!(record(&mut s, &id, &e).is_err());
        s.scope
            .command("allow https://example.test interact account-a")
            .unwrap();
        assert!(s.scope.declared_account(URL, "read").is_none());
        for raw in [
            "https://user:secret@example.test/app",
            "https://example.test/app?token=SECRET",
            "https://example.test/app#SECRET",
            "file:///secret",
        ] {
            assert!(apply(&mut s,&json!({"action":"plan","url":raw,"method":"GET","account_label":"account-a","scenario":"http_response"})).is_err());
        }
        assert!(apply(&mut s,&json!({"action":"plan","url":URL,"method":"GET","account_label":"Bearer secret","scenario":"http_response"})).is_err());
        let mut result = response();
        stamp(&s.scope, "web_fetch", &json!({"url":URL}), &mut result);
        assert!(result.get("test_coverage_context").is_none());
    }

    #[test]
    fn local_lab_uses_existing_recomputed_verdict_not_claimed_success() {
        let mut s = session();
        let url = "http://127.0.0.1:8765/preview/reflected-xss/fixed";
        s.scope
            .command(&format!("allow {url} interact lab"))
            .unwrap();
        let id = plan(&mut s, url, "GET", "lab", "inert_html");
        let rows=(0..3).map(|step|json!({"step":step,"status":200,"complete":true,"body_sha256":if step==1 {"b".repeat(64)}else{"a".repeat(64)},"content_type_sha256":"c".repeat(64),"bytes_captured":10,"baseline_marker_seen":step!=1,"variant_marker_seen":step==1,"raw_probe_sequence_seen":false,"escaped_probe_sequence_seen":true})).collect::<Vec<_>>();
        let args = json!({"action":"run","run_id":"test","url":url,"scenario":"inert_html"});
        let mut r = json!({"ok":true,"run_id":"test","target":url,"state":"completed","execution_status":"observed","requests_attempted":3,"observations":rows,"recipe":{"scenario":"inert_html"},"verdict":"PRIVATE_FAKE_SUCCESS"});
        let e = evidence(&mut s, "run_local_lab", args.clone(), r.clone());
        assert_eq!(record(&mut s, &id, &e).unwrap()["state"], "tested");
        r["run_id"] = json!("test2");
        r["observations"][2]["body_sha256"] = json!("d".repeat(64));
        let mut a = args;
        a["run_id"] = json!("test2");
        let e = evidence(&mut s, "run_local_lab", a, r);
        assert_eq!(record(&mut s, &id, &e).unwrap()["state"], "inconclusive");
        assert!(!report(&s).to_string().contains("PRIVATE_FAKE"));
    }

    #[test]
    fn case_and_version_limits_do_not_silently_discard_history() {
        let mut s = session();
        let id = plan(&mut s, URL, "GET", "account-a", "http_response");
        for i in 0..9 {
            let e = evidence(&mut s, "web_fetch", json!({"url":URL}), response());
            assert_eq!(record(&mut s, &id, &e).is_ok(), i < 8);
        }
        for i in 1..100 {
            plan(
                &mut s,
                &format!("{URL}/{i}"),
                "GET",
                "account-a",
                "http_response",
            );
        }
        assert!(apply(&mut s,&json!({"action":"plan","url":"https://example.test/extra","method":"GET","account_label":"account-a","scenario":"http_response"})).is_err());
        assert_eq!(s.test_coverage.cells.len(), 100);
        assert_eq!(s.test_coverage.cells[0].versions.len(), 8);
    }
    #[test]
    fn finding_links_retest_history_and_resume_are_revalidated() {
        let mut s = session();
        let id = plan(&mut s, URL, "GET", "account-a", "http_response");
        let e = evidence(&mut s, "web_fetch", json!({"url":URL}), response());
        record(&mut s, &id, &e).unwrap();
        let f=crate::reasoning::finding(&json!({"status":"hypothesis","summary":"PRIVATE_HYPOTHESIS","evidence_ids":[e],"limitation":"bounded","rationale":"fixture","next_test":"repeat with control","confirms":"native evidence","refutes":"opposite evidence"}),&s.observations).unwrap();
        s.findings.push(f);
        apply(
            &mut s,
            &json!({"action":"link_finding","case_id":id,"finding_index":0}),
        )
        .unwrap();
        let r = report(&s);
        assert_eq!(
            r["cases"][0]["linked_findings"][0]["native_citations_revalidated"],
            true
        );
        assert!(!r.to_string().contains("PRIVATE_HYPOTHESIS"));
        assert_eq!(
            r["cases"][0]["retest"]["automatic_execution_allowed"],
            false
        );
        s.findings[0].summary = "changed".into();
        assert_eq!(
            report(&s)["cases"][0]["linked_findings"][0]["finding_unchanged"],
            false
        );
        let failed = evidence(
            &mut s,
            "web_fetch",
            json!({"url":URL}),
            json!({"execution_status":"unknown"}),
        );
        record(&mut s, &id, &failed).unwrap();
        let r = report(&s);
        assert_eq!(r["cases"][0]["state"], "inconclusive");
        assert_eq!(r["cases"][0]["retained_tested_evidence"][0], e);
        let resumed = crate::continuity::status(&s, &json!({"jobs":[]}));
        assert_eq!(resumed["research_cases"][0]["state"], "inconclusive");
        assert_eq!(resumed["actions_replayed"], 0);
    }
}
