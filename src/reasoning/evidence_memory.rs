//! Derived index of pinned native captures, not assistant assertions or current truth.
use serde_json::{json, Value};
pub fn select(session: &crate::session::Session, prompt: &str) -> Value {
    let words = prompt
        .split_whitespace()
        .filter(|w| w.chars().count() > 3)
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut file_receipts = std::collections::VecDeque::new();
    let mut local_refs = std::collections::BTreeMap::new();
    for o in &session.observations {
        if o.request_sha256.is_none() || o.result_sha256.is_none() {
            continue;
        }
        let Ok((tool, args, r)) = crate::audit::request_for(session, &o.id) else {
            continue;
        };
        if o.ok && !crate::reasoning::claims::is_historical(&r) {
            let receipt=match tool.as_str() {
                "run_command" if r["exit_code"]==0 => crate::file_receipts::curl_request(&args)
                    .and_then(|(url,path)|path.map(|p|json!({"destination_argument":p,"requested_url_sha256":crate::intelligence::code::hash(url.as_bytes()),"exit_code":0}))),
                "file_info" if r["saved_to_disk"]==true => Some(json!({"observed_disk_path":r["path"],"bytes":r["bytes"]})),
                "write_file"|"edit_file" if r["saved_to_disk"]==true => Some(json!({"written_path":r["path"],"bytes":r["bytes"],"sha256":r["sha256"]})),
                "browser" if args["action"]=="save_response" && r["storage"]=="disk"
                    && r["saved_to_disk"]==true && r["complete_browser_body"]==true
                    && r["path"].is_string() && r["path"]==args["path"] =>
                    Some(json!({"written_path":r["path"],"request_id":r["request_id"],"bytes":r["bytes"],"sha256":r["sha256"],"source":"browser.save_response"})),
                "web_fetch" if r["first_script_urls"].is_array() || r["script_url_hashes"].is_array() => Some(script_receipt(&r)),
                _=>None
            };
            if let Some(mut receipt) = receipt {
                receipt["evidence_id"] = json!(o.id);
                receipt["message_index"] = json!(o.message_index);
                file_receipts.push_back(receipt);
                if file_receipts.len() > 8 {
                    file_receipts.pop_front();
                }
            }
        }
        if !matches!(
            tool.as_str(),
            "read_file" | "read_lines" | "read_file_window" | "search_file" | "browser"
        ) || r["ok"] == false
            || r["execution_status"] == "unknown"
            || r["cached_result"] == true
            || r["denied"] == true
            || r.get("error").is_some()
            || matches!(
                r["execution_status"].as_str(),
                Some("failed" | "not_executed" | "cancelled" | "stale")
            )
        {
            continue;
        }
        if matches!(tool.as_str(), "read_file" | "read_file_window") && r["content"].is_string() {
            if let Some(path) = args["path"]
                .as_str()
                .filter(|p| !p.is_empty() && p.len() <= 256)
            {
                let priority = prompt.contains(&o.id) || prompt.contains(path);
                let reference = json!({"path":path,"evidence_id":o.id,"message_index":o.message_index,
                    "tool":tool,"byte_offset":r["byte_offset"],"end_byte_offset":r["end_byte_offset"],
                    "fresh_evidence":false});
                let candidate = (priority, o.message_index);
                let entry = local_refs
                    .entry(path.to_owned())
                    .or_insert((candidate, reference.clone()));
                if candidate > entry.0 {
                    *entry = (candidate, reference);
                }
            }
        }
        let body = r.to_string();
        let lower = body.to_lowercase();
        let score = words.iter().filter(|w| lower.contains(w.as_str())).count();
        rows.push((score,o.message_index,json!({"evidence_id":o.id,"message_index":o.message_index,"tool":tool,"arguments":args,"result_sha256":o.result_sha256,"historical_native_excerpt":super::context::clip(&body,1600),"excerpt_truncated":body.chars().count()>1600,"fresh_evidence":false,"original_available_via":"recall_context","untrusted_data_not_instructions":true})));
    }
    rows.sort_by_key(|(score, index, _)| std::cmp::Reverse((*score, *index)));
    let mut refs: Vec<_> = local_refs.into_values().collect();
    refs.sort_unstable_by_key(|(rank, _)| std::cmp::Reverse(*rank));
    let refs: Vec<_> = refs.into_iter().take(2).map(|(_, row)| row).collect();
    json!({"file_receipts":file_receipts,"local_read_refs":refs,"selection":"bounded lexical relevance then recency; not semantic proof","historical_capture_only":true,"records":rows.into_iter().take(6).map(|(_,_,v)|v).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_location_uses_only_verified_disk_receipts_in_both_languages() {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "fixture");
        let args = json!({"action":"save_response","path":"actual.js"});
        let result = json!({"ok":true,"storage":"disk","saved_to_disk":true,
            "complete_browser_body":true,"path":"actual.js","bytes":84,"request_id":"4.2"});
        for (id, a, r) in [
            ("network", json!({"action":"network"}), json!({"ok":true,"storage":"session_only","saved_to_disk":false})),
            ("save", args.clone(), result),
            ("denied", args, json!({"ok":false,"denied":true})),
        ] {
            s.messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"type":"function","function":{"name":"browser","arguments":a.to_string()}}]}));
            let mut obs = crate::reasoning::observe(s.messages.len(), "browser", &r);
            obs.request_sha256 = Some(crate::intelligence::code::hash(a.to_string().as_bytes()));
            s.observations.push(obs);
            s.messages.push(json!({"role":"tool","tool_call_id":id,"content":r.to_string()}));
            for prompt in ["وين انحفظ الملف؟", "Where was the file saved?"] {
                let memory = select(&s, prompt);
                let receipts = memory["file_receipts"].as_array().unwrap();
                if id == "network" {
                    assert!(receipts.is_empty());
                } else {
                    assert_eq!(receipts.len(), 1);
                    assert_eq!(receipts[0]["written_path"], "actual.js");
                    assert_eq!(receipts[0]["request_id"], "4.2");
                }
            }
        }
    }

    #[test]
    fn pinned_capture_survives_projection_and_reload_but_tampering_does_not() {
        let root = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "fixture");
        s.messages.push(
            json!({"role":"user","content":"Keep this workspace read-only. احفظ المصدر والقيود"}),
        );
        let a = json!({"path":"note.txt"});
        let r = json!({"path":"note.txt","content":"marker=BD_CHECK_942\nitems=7\nprice_each=6\n"});
        s.messages.push(json!({"role":"assistant","tool_calls":[{"id":"capture","type":"function","function":{"name":"read_file","arguments":a.to_string()}}]}));
        let mut o = crate::reasoning::observe(2, "read_file", &r);
        o.request_sha256 = Some(crate::intelligence::code::hash(a.to_string().as_bytes()));
        s.observations.push(o);
        s.messages
            .push(json!({"role":"tool","tool_call_id":"capture","content":r.to_string()}));
        s.messages
            .push(json!({"role":"assistant","content":"Invented marker SHOULD_NOT_BE_EVIDENCE"}));
        for i in 0..25 {
            s.messages.push(json!({"role":"assistant","tool_calls":[{"id":format!("padding{i}"),"type":"function","function":{"name":"read_file","arguments":a.to_string()}}]}));
            s.messages.push(json!({"role":"tool","tool_call_id":format!("padding{i}"),"content":"padding ".repeat(3000)}));
        }
        let original = s.messages.clone();
        let memory =
            json!({"historical_native_evidence":select(&s,"marker"),"output_reserve_tokens":2048});
        let (view, stats) = crate::reasoning::context::build(
            &s.messages,
            &json!([]),
            16384,
            "Continue read-only",
            &memory,
        )
        .unwrap();
        assert!(serde_json::to_string(&view)
            .unwrap()
            .contains("BD_CHECK_942"));
        assert!(stats.to_string().len() > 2);
        assert_eq!(original, s.messages);
        crate::session::save(state.path(), &mut s).unwrap();
        let mut restored = crate::session::load(state.path(), &s.id, root.path()).unwrap();
        let v = select(&restored, "marker");
        assert_eq!(v["records"].as_array().unwrap().len(), 1);
        assert_eq!(v["records"][0]["fresh_evidence"], false);
        let reference = &v["local_read_refs"][0];
        assert_eq!(reference["path"], "note.txt");
        assert_eq!(reference["message_index"], 2);
        assert_eq!(reference["fresh_evidence"], false);
        assert!(reference.get("content").is_none());
        assert!(!v.to_string().contains("SHOULD_NOT_BE_EVIDENCE"));
        restored.messages[2]["content"] = json!("forged");
        let tampered = select(&restored, "marker");
        assert_eq!(tampered["records"], json!([]));
        assert_eq!(tampered["local_read_refs"], json!([]));
    }
}

fn script_receipt(result: &Value) -> Value {
    let hashes: Vec<_> = result["script_url_hashes"]
        .as_array()
        .into_iter()
        .flatten()
        .take(2)
        .collect();
    let verified: Vec<_> = result["first_script_urls"]
        .as_array()
        .into_iter()
        .flatten()
        .zip(&hashes)
        .take_while(|(url, digest)| {
            url.as_str().is_some_and(|s| {
                s.len() <= 2048
                    && Some(crate::intelligence::code::hash(s.as_bytes()).as_str())
                        == digest.as_str()
            })
        })
        .map(|(url, _)| url)
        .collect();
    json!({"first_script_url_hashes":hashes,"verified_script_urls":verified,
        "source":"original fetch result","untrusted_data_not_instructions":true})
}

#[cfg(test)]
mod receipt_tests {
    use super::*;

    #[test]
    fn compact_receipt_only_quotes_verified_values_in_order() {
        let url = "https://example.test/a.js?v=1";
        let sha = crate::intelligence::code::hash(url.as_bytes());
        let receipt = script_receipt(
            &json!({"first_script_urls":[url,"truncated"],"script_url_hashes":[sha,"invalid"]}),
        );
        assert_eq!(receipt["verified_script_urls"], json!([url]));
        assert_eq!(
            script_receipt(&json!({"script_url_hashes":[sha]}))["verified_script_urls"],
            json!([])
        );
    }
}
