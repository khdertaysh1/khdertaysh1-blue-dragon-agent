//! User-derived fresh local-source requirements, independent of model declarations.
use crate::{session::Session, tools::ToolBox};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub path: Option<String>,
    pub after_message: usize,
    pub requested_at: String,
    #[serde(default)]
    pub bounded_only: bool,
    #[serde(default)]
    pub expected_window: Option<(u64, u64)>,
    #[serde(default)]
    pub explicit_window: bool,
}
fn success(result: &Value) -> bool {
    result["ok"] != false
        && result["denied"] != true
        && result.get("error").is_none()
        && result["cached_result"] != true
        && result["fresh_evidence"] != false
        && result
            .get("execution_status")
            .is_none_or(|s| matches!(s.as_str(), Some("completed" | "succeeded" | "success")))
}
// Recognize a deliberately narrow half-open byte-range syntax. Other explicit
// range forms are unresolved, never silently downgraded to any nonempty read.
fn explicit_range(prompt: &str) -> Option<Result<(u64, u64), ()>> {
    let normalized: String = prompt
        .chars()
        .map(|c| match c {
            '\u{0660}'..='\u{0669}' => char::from(b'0' + (c as u32 - 0x660) as u8),
            '\u{06f0}'..='\u{06f9}' => char::from(b'0' + (c as u32 - 0x6f0) as u8),
            _ => c,
        })
        .collect();
    let lower = normalized.to_lowercase();
    // "first N bytes" names a prefix without spelling out two offsets.
    let words: Vec<_> = lower.split_whitespace()
        .map(|word| word.trim_matches(|c: char| matches!(c, '.' | ',' | ';' | '،' | '؛' | ':' | '`')))
        .collect();
    let mut prefixes = words.windows(3).filter(|w|
        matches!(w[0], "first" | "أول" | "اول") && matches!(w[2], "bytes" | "byte" | "بايت"));
    if let Some(prefix) = prefixes.next() {
        // A second range or offset must not be silently discarded.
        if prefixes.next().is_some() || lower.contains("..")
            || words.iter().any(|w| matches!(*w, "offset" | "offsets" | "إزاحة" | "ازاحة"))
            || words.iter().filter(|w| matches!(**w, "byte" | "bytes" | "بايت" | "البايت")).count() != 1
        {
            return Some(Err(()));
        }
        return Some(prefix[1].parse::<u64>().ok().filter(|n| *n > 0)
            .map(|n| (0, n)).ok_or(()));
    }
    let marker = ["bytes ", "byte range ", "بايت ", "البايت "]
        .iter()
        .filter_map(|m| lower.find(m).map(|i| i + m.len()))
        .min();
    let range_like = lower.as_bytes().windows(3).any(|w| {
        (w[0].is_ascii_digit() && &w[1..] == b"..") || (&w[..2] == b".." && w[2].is_ascii_digit())
    }) || ((lower.contains("byte")
        || lower.contains("بايت")
        || lower.contains("offset")
        || lower.contains("إزاحة")
        || lower.contains("ازاحة"))
        && lower.chars().any(|c| c.is_ascii_digit()));
    if !range_like {
        return None;
    }
    let Some(start) = marker else {
        return Some(Err(()));
    };
    if lower.matches("..").count() != 1 {
        return Some(Err(()));
    }
    let input = lower[start..].trim_start();
    let n = input.chars().take_while(char::is_ascii_digit).count();
    let first = input[..n].parse::<u64>();
    let Some(rest) = input[n..].strip_prefix("..") else {
        return Some(Err(()));
    };
    let n = rest.chars().take_while(char::is_ascii_digit).count();
    let second = rest[..n].parse::<u64>();
    if lower.contains("inclusive")
        || lower.contains("شامل")
        || rest[n..].chars().any(|c| c.is_ascii_digit())
    {
        return Some(Err(()));
    }
    if rest[n..].chars().next().is_some_and(|c| {
        !c.is_whitespace() && !matches!(c, '.' | ',' | ';' | '،' | ')' | ']' | '`')
    }) {
        return Some(Err(()));
    }
    Some(match (first, second) {
        (Ok(a), Ok(b)) if a < b => Ok((a, b)),
        _ => Err(()),
    })
}
pub fn detect(prompt: &str, session: &Session) -> Vec<Request> {
    if prompt.len() > 25_000 {
        return vec![];
    }
    let text = prompt.to_lowercase();
    if [
        "do not read",
        "don't read",
        "do not recheck",
        "don't recheck",
        "لا تقرأ",
        "لا تعد قراءة",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        return vec![];
    }
    let range = explicit_range(prompt);
    let explicit_window = range.is_some();
    let bounded_only = explicit_window
        || text.contains("window")
        || text.contains("bounded")
        || text.contains("نافذة")
        || text.contains("نوافذ");
    let read = text.starts_with("read ")
        || text.contains("recheck")
        || text.contains("read again")
        || text.contains("fresh read")
        || text.contains("أعد قراءة")
        || text.contains("اقرأ");
    if !read {
        return vec![];
    }
    let suffixes = [
        "js", "ts", "tsx", "jsx", "rs", "py", "json", "txt", "md", "html", "css", "c", "cpp", "go",
        "java", "yaml", "yml",
    ];
    let mut paths = prompt
        .split_whitespace()
        .filter_map(|word| {
            let word = word.trim_matches(|c: char| {
                matches!(
                    c,
                    '`' | '\'' | '"' | '(' | ')' | '[' | ']' | ',' | ';' | ':' | '،'
                )
            });
            let word = word.trim_end_matches('.');
            if word.contains("://") || word.len() > 1024 {
                return None;
            }
            let extension = std::path::Path::new(word).extension()?.to_str()?;
            suffixes.contains(&extension).then(|| word.to_owned())
        })
        .collect::<Vec<_>>();
    let historical_network = [
        "network_events",
        "saved network",
        "network capture",
        "سجل الشبكة المحفوظ",
    ]
    .iter()
    .any(|marker| text.contains(marker));
    if historical_network && paths.is_empty() && !text.contains("byte") && !text.contains("بايت")
    {
        return vec![];
    }
    let mut expected_window = range.as_ref().and_then(|r| r.as_ref().ok().copied());
    if range.as_ref().is_some_and(Result::is_err) || (explicit_window && paths.len() > 1) {
        return vec![Request {
            path: None,
            after_message: session.messages.len(),
            requested_at: chrono::Utc::now().to_rfc3339(),
            bounded_only,
            expected_window,
            explicit_window,
        }];
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty()
        && (explicit_window
            || ["source", "window", "file", "المصدر", "الملف"]
                .iter()
                .any(|s| text.contains(s)))
    {
        // This resolves a follow-up target, not evidence of a new read.
        for observation in session.observations.iter().rev() {
            if observation.tool != "read_file_window"
                || !observation.ok
                || observation.request_sha256.is_none()
                || observation.result_sha256.is_none()
            {
                continue;
            }
            if let Ok((_, args, result)) = crate::audit::request_for(session, &observation.id) {
                if success(&result) {
                    if let Some(path) = args["path"].as_str() {
                        if !explicit_window {
                            expected_window = result["byte_offset"]
                                .as_u64()
                                .zip(result["end_byte_offset"].as_u64())
                                .filter(|(a, b)| a < b);
                        }
                        paths.push(path.to_owned());
                        break;
                    }
                }
            }
        }
        if paths.is_empty() {
            return vec![Request {
                path: None,
                after_message: session.messages.len(),
                requested_at: chrono::Utc::now().to_rfc3339(),
                bounded_only,
                expected_window,
                explicit_window,
            }];
        }
    }
    if paths.len() > 4 {
        return vec![Request {
            path: None,
            after_message: session.messages.len(),
            requested_at: chrono::Utc::now().to_rfc3339(),
            bounded_only,
            expected_window,
            explicit_window,
        }];
    }
    paths
        .into_iter()
        .map(|path| Request {
            path: Some(path),
            after_message: session.messages.len(),
            requested_at: chrono::Utc::now().to_rfc3339(),
            bounded_only,
            expected_window,
            explicit_window,
        })
        .collect()
}
pub fn status(requests: &[Request], session: &Session, tools: &ToolBox) -> Value {
    let rows = requests.iter().map(|request| {
        let Some(path) = &request.path else { return json!({"request":request,"state":"blocked","reason":"fresh source target unresolved"}); };
        let mut state = "missing";
        let mut proof = Value::Null;
        for observation in session.observations.iter().filter(|o|o.message_index>request.after_message && (o.tool=="read_file_window" || (!request.bounded_only && o.tool=="read_file"))) {
            if observation.request_sha256.is_none() || observation.result_sha256.is_none() {continue;}
            let Ok((_, args, result))=crate::audit::request_for(session,&observation.id) else {continue;};
            let Some(actual_path)=args["path"].as_str() else {continue;};
            let lexical = |value: &str| std::path::Path::new(value).components().filter(|c| *c != std::path::Component::CurDir).collect::<std::path::PathBuf>();
            if lexical(actual_path)!=lexical(path) && tools.local_file_path(actual_path).ok().zip(tools.local_file_path(path).ok()).is_none_or(|(a,b)|a!=b) {continue;}
            if !observation.ok || !success(&result) {
                if proof.is_null() {state=if result["denied"]==true {"blocked"} else if result["execution_status"]=="unknown" {"unknown"} else {"failed"};}
                continue;
            }
            if observation.tool == "read_file" {
                let Ok(current)=tools.read_file(actual_path) else {state="stale";proof=Value::Null;continue;};
                if current["sha256"] != result["sha256"] || current["content"] != result["content"] {state="stale";proof=Value::Null;continue;}
                state="verified";
                proof=json!({"evidence_id":observation.id,"message_index":observation.message_index,"arguments":args,"source_sha256":result["sha256"],"verified_at":chrono::Utc::now().to_rfc3339()});
                continue;
            }
            let offset=args["byte_offset"].as_u64().unwrap_or(0);
            let count=args["byte_count"].as_u64().unwrap_or(4096);
            // A follow-up recheck must cover its prior captured span; a new
            // request without a declared range proves only the actual read window.
            let Some(end)=result["end_byte_offset"].as_u64() else {continue;};
            let Some(size)=result["file_bytes"].as_u64() else {continue;};
            if end<=offset || result["byte_offset"]!=offset || request.expected_window.is_some_and(|(a,b)|offset>a || end<if request.explicit_window {b} else {b.min(size)}) {state="inconclusive";continue;}
            let Ok(current)=tools.read_file_window(path,offset,count) else {state="stale";proof=Value::Null;continue;};
            if current["window_sha256"]!=result["window_sha256"] || current["content"]!=result["content"] || current["end_byte_offset"]!=result["end_byte_offset"] || current["file_bytes"]!=result["file_bytes"] {state="stale";proof=Value::Null;continue;}
            state="verified";
            proof=json!({"evidence_id":observation.id,"message_index":observation.message_index,"arguments":args,"window_sha256":result["window_sha256"],"byte_offset":offset,"end_byte_offset":end,"verified_at":chrono::Utc::now().to_rfc3339()});
        }
        json!({"request":request,"state":state,"proof":proof})
    }).collect::<Vec<_>>();
    json!({"required":!rows.is_empty(),"all_observed":!rows.is_empty() && rows.iter().all(|r|r["state"]=="verified"),"checks":rows,"semantic_correctness_proven":false,"scope":"Fresh local source capture only; uncovered ranges and semantic relevance are not established. Not whole-file analysis, DOM verification or runtime exploit proof. Original native captures and paired arguments are checked; current bytes are re-read within original tool permissions. Filesystem races remain possible."})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_bytes_resolves_to_named_file_and_prefix() {
        let session = crate::session::new(std::path::Path::new("."), "fixture");
        for prompt in [
            "استخدم file_info على aghanim-trial-01.js ثم اقرأ أول 512 بايت منه باستخدام read_file_window.",
            "اقرأ أول ٥١٢ بايت من aghanim-trial-01.js",
            "Read the first 512 bytes of aghanim-trial-01.js",
        ] {
            let requests = detect(prompt, &session);
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].path.as_deref(), Some("aghanim-trial-01.js"));
            assert_eq!(requests[0].expected_window, Some((0, 512)));
            assert!(requests[0].explicit_window);
        }
        assert_eq!(explicit_range("first 512 bytes and bytes 800..900"), Some(Err(())));
    }

    #[test]
    fn rejects_unsuccessful_matching_content() {
        for state in [
            "failed",
            "denied",
            "unknown",
            "stale",
            "not_executed",
            "cancelled",
        ] {
            assert!(!success(
                &json!({"execution_status":state,"content":"matching"})
            ));
        }
        for value in [
            json!({"ok":false}),
            json!({"denied":true}),
            json!({"cached_result":true}),
            json!({"fresh_evidence":false}),
        ] {
            assert!(!success(&value));
        }
    }
    #[test]
    fn detects_explicit_source_requests_without_expanding_negation() {
        let session = crate::session::new(std::path::Path::new("."), "fixture");
        assert_eq!(
            detect("Read app.js again.", &session)[0].path.as_deref(),
            Some("app.js")
        );
        assert_eq!(
            detect("أعد قراءة `app.js`", &session)[0].path.as_deref(),
            Some("app.js")
        );
        assert!(detect("Do not read app.js again.", &session).is_empty());
        assert!(detect("Read saved network evidence at message_index=30 with network_events=true and offset=8.", &session).is_empty());
        assert!(!detect(
            "Read app.js bytes 0..8 and review saved network evidence",
            &session
        )
        .is_empty());
        assert!(detect("Explain our previous results.", &session).is_empty());
        assert_eq!(
            detect("Read bounded windows of ../outside.js again.", &session)[0]
                .path
                .as_deref(),
            Some("../outside.js")
        );
        assert!(detect("Recheck the current source window.", &session)[0]
            .path
            .is_none());
    }
    #[test]
    fn native_pairs_hashes_ranges_and_success_are_required() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("app.js"), "const value = 42;\n").unwrap();
        let tools = ToolBox::new(
            dir.path().canonicalize().unwrap(),
            crate::tools::ApprovalMode::ReadOnly,
        );
        let mut session = crate::session::new(dir.path(), "fixture");
        let request = Request {
            path: Some("app.js".into()),
            after_message: 0,
            requested_at: "fixture".into(),
            bounded_only: true,
            expected_window: None,
            explicit_window: false,
        };
        let args = json!({"path":"app.js","byte_offset":0,"byte_count":4096});
        let result = tools.read_file_window("app.js", 0, 4096).unwrap();
        session.messages.push(json!({"role":"assistant","tool_calls":[{"id":"capture","function":{"name":"read_file_window","arguments":args.to_string()}}]}));
        let mut observation = crate::reasoning::observe(1, "read_file_window", &result);
        observation.request_sha256 =
            Some(crate::intelligence::code::hash(args.to_string().as_bytes()));
        session.observations.push(observation);
        session
            .messages
            .push(json!({"role":"tool","tool_call_id":"capture","content":result.to_string()}));
        assert_eq!(
            status(std::slice::from_ref(&request), &session, &tools)["all_observed"],
            true
        );
        let mut beyond_eof = request.clone();
        beyond_eof.expected_window = Some((0, 1000));
        beyond_eof.explicit_window = true;
        assert_eq!(
            status(&[beyond_eof], &session, &tools)["checks"][0]["state"],
            "inconclusive"
        );
        let original_observation = session.observations[0].clone();
        for mutation in [
            json!({"execution_status":"denied"}),
            json!({"execution_status":"unknown"}),
            json!({"execution_status":"failed"}),
            json!({"cached_result":true}),
            json!({"end_byte_offset":0}),
            json!({"window_sha256":"forged"}),
        ] {
            let mut changed = result.clone();
            for (k, v) in mutation.as_object().unwrap() {
                changed[k] = v.clone();
            }
            session.messages[1]["content"] = json!(changed.to_string());
            // Even internally consistent serialized records must pass native status and disk checks.
            session.observations[0].result_sha256 = Some(crate::intelligence::code::hash(
                changed.to_string().as_bytes(),
            ));
            assert_ne!(
                status(std::slice::from_ref(&request), &session, &tools)["all_observed"],
                true
            );
        }
        session.messages[1]["content"] = json!(result.to_string());
        session.observations[0] = original_observation;
        session.observations[0].request_sha256 = None;
        assert_ne!(status(&[request], &session, &tools)["all_observed"], true);
    }
    #[test]
    fn explicit_ranges_survive_serialization_and_never_become_any_window() {
        let session = crate::session::new(std::path::Path::new("."), "fixture");
        for prompt in [
            "Read bounded window of app.js bytes 1000..1100 again",
            "أعد قراءة app.js بايت ١٠٠٠..١١٠٠",
        ] {
            let requests = detect(prompt, &session);
            assert_eq!(requests[0].expected_window, Some((1000, 1100)));
            assert!(requests[0].bounded_only && requests[0].explicit_window);
            let restored: Vec<Request> =
                serde_json::from_str(&serde_json::to_string(&requests).unwrap()).unwrap();
            assert_eq!(restored[0].expected_window, Some((1000, 1100)));
            assert!(restored[0].explicit_window);
        }
        for prompt in [
            "Read app.js bytes 1000-1100 again",
            "Read app.js bytes 1100..1000 again",
            "Read app.js bytes 0..0 again",
            "Read app.js bytes 0..100 and 200..300 again",
            "Read app.js offset 1000 length 100 again",
            "أعد قراءة app.js بايت ١٠٠٠ إلى ١١٠٠",
        ] {
            let requests = detect(prompt, &session);
            assert!(requests[0].path.is_none(), "{prompt}");
        }
    }
    #[test]
    fn nameless_explicit_range_remains_unresolved_without_prior_capture() {
        let session = crate::session::new(std::path::Path::new("."), "fixture");
        let requests = detect("Read bytes 1000..1100 again.", &session);
        assert_eq!(requests.len(), 1);
        assert!(requests[0].path.is_none());
        assert_eq!(requests[0].expected_window, Some((1000, 1100)));
    }
}
