//! Independent Blue Free routing. Retry only explicit pre-stream failures.
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct ProviderFailure {
    pub status: u16,
    pub model_specific: bool,
    pub retry_after: u64,
}
impl std::fmt::Display for ProviderFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Provider HTTP {} ({}); retry-after {} seconds",
            self.status,
            if self.model_specific {
                "model-specific"
            } else {
                "shared or unknown scope; no rotation"
            },
            self.retry_after
        )
    }
}
impl std::error::Error for ProviderFailure {}

impl ProviderFailure {
    pub fn from_response(status: u16, retry_after: Option<u64>, body: &str) -> Self {
        let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let error = &parsed["error"];
        let source = error["limit_source"]
            .as_str()
            .or_else(|| error["metadata"]["limit_source"].as_str())
            .unwrap_or("");
        let code = error["provider_error_code"]
            .as_str()
            .or_else(|| error["code"].as_str())
            .or_else(|| error["metadata"]["provider_error_code"].as_str())
            .unwrap_or("");
        let shared = [
            "ip",
            "account",
            "gateway",
            "global",
            "anonymous",
            "organization",
        ]
        .iter()
        .any(|part| source.to_lowercase().contains(part));
        let model_specific = !shared
            && match status {
                429 => {
                    matches!(code, "model_rate_limit_exceeded" | "model_quota_exceeded")
                        || matches!(source, "model" | "upstream_model" | "model_rate_limit")
                }
                404 => matches!(code, "model_not_found" | "model_unavailable"),
                502 | 503 => true,
                _ => false,
            };
        let retry_after = retry_after
            .or_else(|| error["retry_after_seconds"].as_u64())
            .unwrap_or(if status == 429 { 300 } else { 60 })
            .clamp(1, 86400);
        Self {
            status,
            model_specific,
            retry_after,
        }
    }
}

pub struct Rotation {
    pub enabled: bool,
    cooldowns: HashMap<String, Instant>,
    tried: HashSet<String>,
    remaining: usize,
    pub history: Vec<Value>,
}
impl Rotation {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            cooldowns: HashMap::new(),
            tried: HashSet::new(),
            remaining: 3,
            history: Vec::new(),
        }
    }
    pub fn begin_turn(&mut self) {
        self.tried.clear();
        self.remaining = 3;
        self.history.clear();
    }
    pub fn next(
        &mut self,
        current: &str,
        failure: &ProviderFailure,
        candidates: &[String],
    ) -> Option<String> {
        if !self.enabled || !failure.model_specific || self.remaining == 0 {
            return None;
        }
        self.tried.insert(current.into());
        self.cooldowns.insert(
            current.into(),
            Instant::now() + Duration::from_secs(failure.retry_after),
        );
        let next = candidates
            .iter()
            .find(|id| {
                id.as_str() != crate::models::KILO_MODEL
                    && crate::models::kilo_id(id)
                    && !self.tried.contains(id.as_str())
                    && self
                        .cooldowns
                        .get(id.as_str())
                        .is_none_or(|until| *until <= Instant::now())
            })?
            .clone();
        self.remaining -= 1;
        self.tried.insert(next.clone());
        self.history.push(json!({"from":current,"to":next,"http_status":failure.status,"cooldown_seconds":failure.retry_after}));
        Some(next)
    }
    pub fn status(&self, active: &str) -> Value {
        json!({"mode":if self.enabled {"Blue Free automatic"} else {"pinned / disabled"},"active_model":active,
            "remaining_switches_this_turn":self.remaining,"max_switches_per_turn":3,"history":self.history,
            "cooldowns":self.cooldowns.iter().filter_map(|(id,until)| until.checked_duration_since(Instant::now()).map(|d|json!({"model":id,"seconds":d.as_secs()}))).collect::<Vec<_>>(),
            "rules":"Same Kilo zero-priced tool-capable pool only. No paid/auth/IP/global-limit fallback; no retry after stream interruption; completed tools are never replayed."})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotates_only_explicit_model_limits_and_honors_budget() {
        let model = ProviderFailure::from_response(
            429,
            Some(20),
            r#"{"error":{"limit_source":"upstream_model","code":"model_rate_limit_exceeded"}}"#,
        );
        let ip = ProviderFailure::from_response(
            429,
            None,
            r#"{"error":{"limit_source":"anonymous_ip","code":"model_rate_limit_exceeded"}}"#,
        );
        let unknown = ProviderFailure::from_response(429, None, "unknown");
        let mut r = Rotation::new(true);
        let pool = [
            "a/one:free",
            "a/two:free",
            "a/three:free",
            "a/four:free",
            "paid/model",
        ]
        .map(str::to_owned);
        assert!(r.next("kilo-auto/free", &ip, &pool).is_none());
        assert!(r.next("kilo-auto/free", &unknown, &pool).is_none());
        let one = r.next("kilo-auto/free", &model, &pool).unwrap();
        let two = r.next(&one, &model, &pool).unwrap();
        let three = r.next(&two, &model, &pool).unwrap();
        assert!(r.next(&three, &model, &pool).is_none());
        assert_eq!(r.history.len(), 3);
        r.begin_turn();
        assert_eq!(r.next(&three, &model, &pool).unwrap(), "a/four:free");
    }
    #[test]
    fn pinned_payment_auth_and_shared_limits_never_rotate() {
        let pool = vec!["x/code:free".into()];
        let retry = ProviderFailure::from_response(503, None, "");
        assert!(Rotation::new(false)
            .next("x/pinned:free", &retry, &pool)
            .is_none());
        for status in [401, 402, 403, 400, 500] {
            assert!(Rotation::new(true)
                .next(
                    "kilo-auto/free",
                    &ProviderFailure::from_response(status, None, ""),
                    &pool
                )
                .is_none());
        }
    }
}
