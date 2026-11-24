//! Detect stalled observations and reuse per-turn HTTP evidence references.
//! Live browser/file tools are never cached.
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[derive(Default)]
struct Seen {
    result: String,
    unchanged: usize,
    denied: bool,
    unadvertised: bool,
    block_reason: &'static str,
    evidence: Option<String>,
    next_offset: Option<u64>,
}
#[derive(Default)]
pub struct Guard {
    seen: BTreeMap<String, Seen>,
    last: String,
    blocked: usize,
    consecutive_blocks: usize,
    repetitions: usize,
    fetches: BTreeMap<String, Value>,
    cached_reuses: usize,
    consecutive_cached: usize,
    recall_reads: usize,
    saving_file: bool,
    opened_url: Option<String>,
    network_evidence: Option<String>,
    capture_summary: Option<Value>,
    // Observed bytes, shared across full and window reads. No live read is skipped.
    read_ranges: BTreeMap<(String, usize), String>,
    redundant_reads: usize,
}
impl Guard {
    pub fn for_task(prompt: &str) -> Self {
        let text = prompt.to_lowercase();
        let saving_file = ["save", "download", "احفظ", "حفظ", "نزّل", "نزل", "تحميل"]
            .iter()
            .any(|word| text.contains(word))
            && ![
                "reload",
                "refresh the page",
                "reopen",
                "أعد تحميل الصفحة",
                "أعد فتح",
            ]
            .iter()
            .any(|word| text.contains(word));
        Self {
            saving_file,
            ..Self::default()
        }
    }

    fn key(name: &str, args: &Value) -> String {
        if name == "recall_context" && args["network_events"] == true {
            return format!(
                "network-events:{}:{}:{}",
                args["message_index"],
                args["request_id"],
                args["offset"].as_u64().unwrap_or(0)
            );
        }
        if name == "recall_context" {
            return format!(
                "recall:{}:{}:{}",
                args["message_index"],
                args["offset"].as_u64().unwrap_or(0),
                args["count"].as_u64().unwrap_or(6000)
            );
        }
        format!("{name}:{}", args)
    }
    /// A schema transition corrects only a never-dispatched unadvertised call.
    /// Actual denial, unknown execution and malformed values remain guarded.
    pub fn advertised_and_valid(&mut self, name: &str, args: &Value) {
        let key = Self::key(name, args);
        if self.seen.get(&key).is_some_and(|s| s.unadvertised) {
            self.seen.remove(&key);
        }
    }
    /// Called exclusively for the local pre-dispatch schema error, never tool data.
    pub fn rejected_unadvertised(&mut self, name: &str, args: &Value) {
        if let Some(seen) = self.seen.get_mut(&Self::key(name, args)) {
            seen.unadvertised = true;
        }
    }
    pub fn link_evidence(&mut self, name: &str, args: &Value, id: &str, result: &Value) {
        if name == "browser"
            && args["action"] == "network"
            && self.opened_url.is_some()
            && result["ok"] == true
        {
            self.network_evidence = Some(id.to_owned());
            self.capture_summary = self
                .opened_url
                .as_deref()
                .and_then(|url| crate::network_history::file_selection(result, url, id));
        }
        if let Some(seen) = self.seen.get_mut(&Self::key(name, args)) {
            seen.evidence = Some(id.to_owned());
        }
    }
    pub fn file_capture(&self) -> Option<&Value> {
        self.capture_summary.as_ref()
    }

    pub fn is_file_task(&self) -> bool {
        self.saving_file
    }

    pub fn before(&mut self, name: &str, args: &Value) -> Option<Value> {
        if matches!(name, "job_status" | "job_output") {
            return None;
        }
        let key = Self::key(name, args);
        if name == "recall_context" && (self.recall_reads >= 8 || self.seen.contains_key(&key)) {
            self.blocked += 1;
            self.consecutive_blocks += 1;
            return Some(
                json!({"ok":false,"repetition_blocked":true,"execution_status":"not_executed","message_index":args["message_index"],"requested_offset":args["offset"].as_u64().unwrap_or(0),"previous_next_offset":self.seen.get(&key).and_then(|s|s.next_offset),"category":if self.seen.contains_key(&key) {"duplicate_recall"} else {"recall_budget_exhausted"},"error":if self.seen.contains_key(&key) {"This page was requested earlier. Use the original result."} else {"Eight history pages already read in this turn. Report available evidence."}}),
            );
        }
        if self
            .seen
            .get(&key)
            .is_some_and(|s| s.denied || (self.last == key && s.unchanged >= 3))
        {
            self.blocked += 1;
            self.consecutive_blocks += 1;
            let original = &self.seen[&key];
            let uncertain_or_denied = original.denied;
            return Some(json!({"ok":false,"execution_status":"not_executed",
            "repetition_blocked":true,"repeated_tool":name,
            "original_evidence_id":self.seen.get(&key).and_then(|s|s.evidence.as_deref()),
            "category":if uncertain_or_denied {self.seen[&key].block_reason} else {"repeated_unchanged"},
            "error":if uncertain_or_denied {
                match original.block_reason {
                    "prior_user_declined"=>"User declined the earlier action. Stop here.",
                    "prior_execution_unknown"=>"The earlier action may have executed. Use its evidence and any later page observation; do not replay it.",
                    "prior_invalid_arguments"=>"The previous arguments were invalid. Consult the original schema diagnostic.",
                    _=>"The previous action was blocked. Inspect the original evidence."
                }
            } else {
                "Repeated identical arguments returned unchanged results. Inspect the previous result for this tool."
            }}));
        }
        if name == "browser"
            && self.saving_file
            && matches!(args["action"].as_str(), Some("open" | "navigate"))
            && self.opened_url.is_some()
            && self.opened_url.as_deref() == args["url"].as_str()
        {
            if let Some(id) = self.network_evidence.as_deref() {
                self.blocked += 1;
                self.consecutive_blocks += 1;
                return Some(
                    json!({"ok":false,"execution_status":"not_executed","repetition_blocked":true,
                    "category":"network_already_captured","original_evidence_id":id,
                    "error":"This document already has a network capture. Continue from its recorded request or page the existing capture. Use browser action save_response for the requested file. If no suitable response is present, report the missing evidence."}),
                );
            }
        }
        if name == "web_fetch" {
            if let Some(reference) = self.fetches.get(&key) {
                self.cached_reuses += 1;
                self.consecutive_cached += 1;
                return Some(reference.clone());
            }
        }
        None
    }
    /// Reuse only within one turn. Never label the old response as a fresh check.
    pub fn record_fetch(&mut self, args: &Value, result: &Value, message_index: usize) {
        if result["cached_result"] == true || result["repetition_blocked"] == true {
            return;
        }
        if result["status"].as_u64().is_none() {
            return;
        }
        self.fetches.insert(Self::key("web_fetch", args), json!({
            "ok":result["ok"], "cached_result":true, "fresh_evidence":false,
            "execution_status":"not_executed", "status":result["status"],
            "original_evidence_id":result["evidence_id"], "original_message_index":message_index,
            "note":"This URL was already fetched during this turn. No network request or approval was repeated. Use recall_context with original_message_index to inspect its exact response. A new user turn permits a fresh fetch."
        }));
    }
    pub fn after(&mut self, name: &str, args: &Value, result: &Value) -> bool {
        if matches!(name, "job_status" | "job_output") {
            return false;
        }
        if name == "browser" && self.saving_file {
            match args["action"].as_str() {
                Some("open" | "navigate") => {
                    self.opened_url = args["url"]
                        .as_str()
                        .filter(|_| result["ok"] == true)
                        .map(str::to_owned);
                    self.network_evidence = None;
                    self.capture_summary = None;
                }
                Some("snapshot") if result["ok"] == true && result["url"].is_string() => {
                    if self.opened_url.as_deref() != result["url"].as_str() {
                        self.opened_url = result["url"].as_str().map(str::to_owned);
                        self.network_evidence = None;
                        self.capture_summary = None;
                    }
                }
                Some(
                    "snapshot" | "network" | "source" | "status" | "console" | "screenshot"
                    | "wait_for" | "save_response",
                ) => {}
                _ => {
                    self.opened_url = None;
                    self.network_evidence = None;
                    self.capture_summary = None;
                }
            }
        }
        let mut redundant = false;
        if matches!(name, "read_file" | "read_file_window") {
            if let (Some(path), Some(content)) = (args["path"].as_str(), result["content"].as_str())
            {
                let offset = result["byte_offset"].as_u64().unwrap_or(0) as usize;
                let key = (path.to_owned(), offset);
                redundant = self
                    .read_ranges
                    .range((path.to_owned(), 0)..=key.clone())
                    .any(|((_, start), prior)| {
                        prior
                            .as_bytes()
                            .get(offset - start..(offset - start).saturating_add(content.len()))
                            == Some(content.as_bytes())
                    });
                if !redundant {
                    // A new local observation breaks the streak, even after blocked calls
                    // to other tools. Individual denied requests remain guarded in seen.
                    if !content.is_empty() {
                        self.consecutive_blocks = 0;
                    }
                    let end = offset.saturating_add(content.len());
                    self.read_ranges.retain(|(p, s), v| {
                        p != path
                            || (!matches!(name, "read_file")
                                && (*s >= end || s.saturating_add(v.len()) <= offset))
                    });
                    self.read_ranges.insert(key, content.to_owned());
                }
            }
        }
        self.redundant_reads = if redundant {
            self.redundant_reads + 1
        } else {
            0
        };
        self.consecutive_cached = 0;
        if name == "recall_context" && result["ok"] == true {
            self.recall_reads += 1;
            if !self.seen.contains_key(&Self::key(name, args)) {
                self.consecutive_blocks = 0;
            }
        }
        let key = Self::key(name, args);
        let text = result.to_string();
        let old = self.seen.entry(key.clone()).or_default();
        if old.result == text {
            old.unchanged += 1;
            self.repetitions += 1;
        } else {
            old.unchanged = 1;
            self.repetitions = 0;
        }
        let status = result["status"].as_str().unwrap_or("");
        let error = result["error"].as_str().unwrap_or("").to_lowercase();
        old.unadvertised = false;
        old.denied = result["category"] == "invalid_arguments"
            || result["denied"] == true
            || status.contains("denied")
            || result["execution_status"] == "unknown"
            || error.contains("scope denied")
            || error.contains("outcome unknown")
            || error.contains("outcome may be unknown")
            || error.contains("timed out")
            || error.contains("cancelled");
        old.next_offset = result["next_offset"].as_u64();
        old.block_reason = if result["approval_source"] == "user_choice" {
            "prior_user_declined"
        } else if result["execution_status"] == "unknown" {
            "prior_execution_unknown"
        } else if result["category"] == "invalid_arguments" {
            "prior_invalid_arguments"
        } else {
            "prior_action_blocked"
        };
        old.result = text;
        self.last = key;
        redundant || old.unchanged >= 2
    }
    pub fn repeated_file_reads(&self) -> bool {
        self.redundant_reads >= 3
    }
    pub fn stalled(&self) -> bool {
        self.repeated_file_reads()
            || self.consecutive_blocks >= 3
            || self.repetitions >= 8
            || self.consecutive_cached >= 3
    }
    pub fn status(&self) -> Value {
        json!({"blocked_identical_calls":self.blocked,"unchanged_observations":self.repetitions,"cached_fetch_reuses":self.cached_reuses,"note":"No cached tool output is presented as a fresh observation."})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_task_preserves_capture_across_read_only_actions() {
        let mut guard = Guard::for_task("Open the page and save a file");
        let open = json!({"action":"open","url":"https://example.test/"});
        let network = json!({"action":"network"});
        let ok = json!({"ok":true});
        guard.after("browser", &open, &ok);
        guard.after("browser", &network, &ok);
        guard.link_evidence("browser", &network, "E000047", &ok);
        guard.after("list_files", &json!({}), &ok);
        let blocked = guard.before("browser", &open).unwrap();
        assert_eq!(blocked["original_evidence_id"], "E000047");
        assert!(guard
            .before("browser", &json!({"action":"save_response"}))
            .is_none());
        guard.after("browser", &json!({"action":"click"}), &ok);
        assert!(guard.before("browser", &open).is_none());
    }

    #[test]
    fn new_local_windows_break_block_streaks_without_unlocking_declined_calls() {
        let mut guard = Guard::default();
        let denied = json!({"path":"locked.txt","content":"x"});
        guard.after("write_file", &denied, &json!({"denied":true}));
        for offset in [0, 5, 10, 15] {
            assert!(guard.before("write_file", &denied).is_some());
            let args = json!({"path":"demo.txt","byte_offset":offset,"byte_count":5});
            let window = json!({"byte_offset":offset,"content":"hello"});
            assert!(!guard.after("read_file_window", &args, &window));
            assert!(!guard.stalled());
        }
        for _ in 0..3 {
            assert!(guard.before("write_file", &denied).is_some());
        }
        assert!(guard.stalled());
    }
    #[test]
    fn distinct_saved_pages_are_progress_but_identical_recall_still_stops() {
        let mut guard = Guard::default();
        for offset in [0, 8, 16] {
            let args = json!({"message_index":9,"network_events":true,"offset":offset});
            assert!(guard.before("recall_context", &args).is_none());
            guard.after(
                "recall_context",
                &args,
                &json!({"ok":true,"next_offset":offset+8}),
            );
            assert!(guard.before("recall_context", &args).is_some());
            assert!(!guard.stalled());
        }
        let args = json!({"message_index":9,"network_events":true,"offset":16});
        assert!(guard.before("recall_context", &args).is_some());
        assert!(guard.before("recall_context", &args).is_some());
        assert!(guard.stalled());
    }
    #[test]
    fn retry_reports_original_evidence_and_cause() {
        for (value, expected) in [
            (
                json!({"denied":true,"approval_source":"user_choice"}),
                "prior_user_declined",
            ),
            (
                json!({"execution_status":"unknown"}),
                "prior_execution_unknown",
            ),
        ] {
            let mut g = Guard::default();
            let args = json!({"action":"click"});
            g.after("browser", &args, &value);
            g.link_evidence("browser", &args, "E9", &value);
            let blocked = g.before("browser", &args).unwrap();
            assert_eq!(blocked["category"], expected);
            assert_eq!(blocked["original_evidence_id"], "E9");
        }
    }
    #[test]
    fn schema_transition_only_clears_unadvertised_never_dispatched_calls() {
        let args = json!({"action":"status"});
        let mut g = Guard::default();
        g.after("browser", &args, &json!({"category":"invalid_arguments","execution_status":"not_executed","field":"$","expected_type":"advertised tool","actual_type":"unknown tool"}));
        // A tool-authored lookalike does not grant schema recovery.
        g.advertised_and_valid("browser", &args);
        assert!(g.before("browser", &args).is_some());
        g.rejected_unadvertised("browser", &args);
        g.advertised_and_valid("other", &args);
        assert!(g.before("browser", &args).is_some());
        g.advertised_and_valid("browser", &args);
        assert!(g.before("browser", &args).is_none());
        for result in [
            json!({"denied":true}),
            json!({"execution_status":"unknown"}),
            json!({"category":"invalid_arguments","field":"$.action","execution_status":"not_executed"}),
        ] {
            let mut g = Guard::default();
            g.after("browser", &args, &json!({"category":"invalid_arguments","execution_status":"not_executed","field":"$","expected_type":"advertised tool","actual_type":"unknown tool"}));
            g.rejected_unadvertised("browser", &args);
            g.advertised_and_valid("browser", &args);
            g.after("browser", &args, &result);
            g.advertised_and_valid("browser", &args);
            assert!(g.before("browser", &args).is_some());
        }
    }
    #[test]
    fn recall_blocks_same_page_even_when_interleaved_and_bounds_distinct_pages() {
        let mut guard = super::Guard::default();
        let args = serde_json::json!({"message_index":3,"offset":0,"count":6000});
        assert!(guard.before("recall_context", &args).is_none());
        guard.after(
            "recall_context",
            &args,
            &serde_json::json!({"ok":true,"excerpt":"old source"}),
        );
        guard.after(
            "file_info",
            &serde_json::json!({"path":"a"}),
            &serde_json::json!({"ok":true}),
        );
        assert!(guard
            .before(
                "recall_context",
                &serde_json::json!({"message_index":3,"ignored":"different"})
            )
            .is_some());
        for index in 4..11 {
            let args = serde_json::json!({"message_index":index});
            guard.after("recall_context", &args, &serde_json::json!({"ok":true}));
        }
        assert!(guard
            .before("recall_context", &serde_json::json!({"message_index":100}))
            .is_some());
    }
    #[test]
    fn live_job_polling_is_not_cached_or_classified_as_a_stall() {
        let mut guard = Guard::default();
        let args = json!({"job_id":"test"});
        for _ in 0..12 {
            assert!(guard.before("job_status", &args).is_none());
            assert!(!guard.after("job_status", &args, &json!({"ok":true,"state":"running"})));
        }
        assert!(!guard.stalled());
    }
    #[test]
    fn blocks_denied_actions_but_rechecks_live_reads_until_unchanged() {
        let mut g = Guard::default();
        let args = json!({"path":"x"});
        g.after(
            "write_file",
            &args,
            &json!({"ok":false,"status":"denied by user"}),
        );
        assert!(g.before("write_file", &args).is_some());
        for _ in 0..3 {
            assert!(g.before("read_file", &args).is_none());
            g.after("read_file", &args, &json!({"content":"same"}));
        }
        assert!(g.before("read_file", &args).is_some());
        assert!(g
            .before("read_file", &json!({"path":"different"}))
            .is_none());
    }
}

#[cfg(test)]
mod fetch_tests {
    use super::*;
    #[test]
    fn productive_turn_is_not_stalled_by_interleaved_cached_fetches() {
        let mut guard = Guard::default();
        let args = json!({"url":"https://example.com/app.js"});
        guard.record_fetch(
            &args,
            &json!({"ok":true,"status":200,"evidence_id":"E1"}),
            8,
        );
        for i in 0..10 {
            assert!(guard.before("web_fetch", &args).is_some());
            guard.after(
                "read_file",
                &json!({"path":format!("file{i}")}),
                &json!({"ok":true,"content":i}),
            );
            assert!(!guard.stalled());
        }
        for _ in 0..3 {
            guard.before("web_fetch", &args);
        }
        assert!(guard.stalled(), "a pure cached-result loop still stops");
    }
    #[test]
    fn fetch_reference_survives_interleaved_tools_without_replaying_body() {
        let mut guard = Guard::default();
        let args = json!({"url":"https://example.com/app.js"});
        guard.record_fetch(
            &args,
            &json!({"ok":true,"status":200,"text":"body","evidence_id":"E1"}),
            8,
        );
        guard.after("read_file", &json!({"path":"other"}), &json!({"ok":true}));
        let result = guard.before("web_fetch", &args).unwrap();
        assert_eq!(result["original_message_index"], 8);
        assert_eq!(result["original_evidence_id"], "E1");
        assert_eq!(result["fresh_evidence"], false);
        assert!(result.get("text").is_none());
        assert!(Guard::default().before("web_fetch", &args).is_none());
    }
}
