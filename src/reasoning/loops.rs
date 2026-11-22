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
}
#[derive(Default)]
pub struct Guard {
    seen: BTreeMap<String, Seen>,
    last: String,
    blocked: usize,
    repetitions: usize,
    fetches: BTreeMap<String, Value>,
    cached_reuses: usize,
    consecutive_cached: usize,
    recall_reads: usize,
}
impl Guard {
    fn key(name: &str, args: &Value) -> String {
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
    pub fn before(&mut self, name: &str, args: &Value) -> Option<Value> {
        if matches!(name, "job_status" | "job_output") {
            return None;
        }
        let key = Self::key(name, args);
        if name == "recall_context" && (self.recall_reads >= 8 || self.seen.contains_key(&key)) {
            self.blocked += 1;
            return Some(
                json!({"ok":false,"repetition_blocked":true,"execution_status":"not_executed","error":"This history page was already recalled, or the eight-page turn budget was reached. Use the existing excerpt, narrow the investigation, or request another user turn; original history is preserved."}),
            );
        }
        if self
            .seen
            .get(&key)
            .is_some_and(|s| s.denied || (self.last == key && s.unchanged >= 3))
        {
            self.blocked += 1;
            let uncertain_or_denied = self.seen.get(&key).is_some_and(|s| s.denied);
            return Some(json!({"ok":false,"execution_status":"not_executed",
            "repetition_blocked":true,"repeated_tool":name,
            "category":if uncertain_or_denied {"prior_denied_or_uncertain"} else {"repeated_unchanged"},
            "error":if uncertain_or_denied {
                "An earlier identical action was denied or its outcome is uncertain. Consult its original result; do not re-request approval."
            } else {
                "Repeated identical arguments returned unchanged results. Inspect the previous result for this tool."
            }}));
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
        self.consecutive_cached = 0;
        if name == "recall_context" && result["ok"] == true {
            self.recall_reads += 1;
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
        old.result = text;
        self.last = key;
        old.unchanged >= 2
    }
    pub fn stalled(&self) -> bool {
        self.blocked >= 3 || self.repetitions >= 8 || self.consecutive_cached >= 3
    }
    pub fn status(&self) -> Value {
        json!({"blocked_identical_calls":self.blocked,"unchanged_observations":self.repetitions,"cached_fetch_reuses":self.cached_reuses,"note":"No cached tool output is presented as a fresh observation."})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
