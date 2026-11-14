//! Detect stalled observations without caching potentially changing browser/file data.
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
        None
    }
    pub fn after(&mut self, name: &str, args: &Value, result: &Value) -> bool {
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
        self.blocked >= 3 || self.repetitions >= 8
    }
    pub fn status(&self) -> Value {
        json!({"blocked_identical_calls":self.blocked,"unchanged_observations":self.repetitions,"note":"No cached tool output is presented as a fresh observation."})
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
