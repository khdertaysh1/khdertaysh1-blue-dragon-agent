//! Detect stalled observations and reuse per-turn HTTP evidence references.
//! Live browser/file tools are never cached.
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[derive(Default)]
struct Seen {
    result: String,
    unchanged: usize,
    denied: bool,
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
}
impl Guard {
    fn key(name: &str, args: &Value) -> String {
        format!("{name}:{}", args)
    }
    pub fn before(&mut self, name: &str, args: &Value) -> Option<Value> {
        let key = Self::key(name, args);
        if self
            .seen
            .get(&key)
            .is_some_and(|s| s.denied || (self.last == key && s.unchanged >= 3))
        {
            self.blocked += 1;
            return Some(
                json!({"ok":false,"execution_status":"not_executed","repetition_blocked":true,"error":"This identical call was denied/uncertain or repeatedly returned unchanged results. Do not re-request the same approval. Inspect different evidence or change inputs/strategy. A new user turn resets this guard."}),
            );
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
        self.consecutive_cached = 0;
        let key = Self::key(name, args);
        let text = result.to_string();
        let old = self.seen.entry(key.clone()).or_default();
        if old.result == text {
            old.unchanged += 1;
            self.repetitions += 1;
        } else {
            old.unchanged = 1;
        }
        let status = result["status"].as_str().unwrap_or("");
        let error = result["error"].as_str().unwrap_or("").to_lowercase();
        old.denied = result["denied"] == true
            || status.contains("denied")
            || result["execution_status"] == "unknown"
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
