//! Independent Blue Free routing. Retry only explicit pre-stream failures.
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct ProviderFailure {
    pub status: u16,
    pub model_specific: bool,
    pub retry_after: u64,
    pub context_exceeded: bool,
    pub retry_after_source: &'static str,
    pub code: Option<String>,
    pub reason: Option<String>,
    pub request_id: Option<String>,
}
impl std::fmt::Display for ProviderFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let category = if self.context_exceeded {
            "context_limit"
        } else if self.status == 400 {
            "invalid_request"
        } else if self.model_specific {
            "explicit_model_failure"
        } else if self.status == 429
            && self.code.as_deref() == Some("PROMOTION_MODEL_LIMIT_REACHED")
        {
            "free_promotion_limit"
        } else {
            "unclassified_scope"
        };
        let cooldown = if self.retry_after_source == "not_reported" {
            "unspecified".to_owned()
        } else {
            format!("{}s", self.retry_after)
        };
        write!(f, "Provider HTTP {}; category={category}; code={}; reason={}; request_id={}; cooldown={cooldown} source={}; provider_layer_retries=0",
            self.status, self.code.as_deref().unwrap_or("unreported"), self.reason.as_deref().unwrap_or("unreported"), self.request_id.as_deref().unwrap_or("unreported"), self.retry_after_source)
    }
}
impl std::error::Error for ProviderFailure {}

impl ProviderFailure {
    pub fn from_response(status: u16, retry_after: Option<u64>, body: &str) -> Self {
        let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let error = if parsed["error"].is_object() {
            &parsed["error"]
        } else {
            &parsed
        };
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
                502 | 503 => {
                    matches!(source, "model" | "upstream_model")
                        || matches!(code, "model_unavailable" | "model_overloaded")
                }
                _ => false,
            };
        let retry_after_source = if retry_after.is_some() {
            "provider_header"
        } else if error["retry_after_seconds"].as_u64().is_some() {
            "provider_body"
        } else if status == 429 {
            "not_reported"
        } else {
            "program_default"
        };
        let retry_after = retry_after
            .or_else(|| error["retry_after_seconds"].as_u64())
            .unwrap_or(
                if status != 429 && (model_specific || matches!(status, 502 | 503)) {
                    60
                } else {
                    0
                },
            );
        Self {
            status,
            retry_after_source,
            code: (!code.is_empty())
                .then(|| code.to_owned())
                .or_else(|| error["code"].as_i64().map(|v| v.to_string())),
            reason: error["message"]
                .as_str()
                .or_else(|| parsed["error"].as_str())
                .map(str::to_owned),
            request_id: None,
            model_specific,
            retry_after,
            context_exceeded: matches!(status, 400 | 413 | 422)
                && matches!(
                    code,
                    "context_length_exceeded" | "context_window_exceeded" | "input_too_long"
                ),
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
        if (current == crate::models::KILO_MODEL && failure.status != 429)
            || !self.enabled
            || !failure.model_specific
            || self.remaining == 0
        {
            return None;
        }
        self.tried.insert(current.into());
        self.cooldowns.insert(
            current.into(),
            Instant::now().checked_add(Duration::from_secs(failure.retry_after))?,
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
        self.history.push(json!({"from":current,"to":next,"http_status":failure.status,"cooldown_seconds":failure.retry_after,"cooldown_source":failure.retry_after_source}));
        Some(next)
    }
    pub fn status(&self, active: &str) -> Value {
        json!({"mode":if self.enabled {"Blue Free automatic"} else {"pinned / disabled"},"active_model":active,"gateway_auto_alias_preserved":active==crate::models::KILO_MODEL,
            "remaining_switches_this_turn":self.remaining,"max_switches_per_turn":3,"history":self.history,
            "cooldowns":self.cooldowns.iter().filter_map(|(id,until)| until.checked_duration_since(Instant::now()).map(|d|json!({"model":id,"seconds":d.as_secs()}))).collect::<Vec<_>>(),
            "rules":"Model rotation uses the Kilo free pool. Provider rotation on HTTP 429 uses the configured destination order. Completed tools are never replayed."})
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
        assert!(Rotation::new(true)
            .next("kilo-auto/free", &model, &pool)
            .is_some());
        let one = r.next("a/start:free", &model, &pool).unwrap();
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

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn defaults_are_not_provider_headers_and_generic_service_errors_do_not_rotate() {
        let invalid =
            ProviderFailure::from_response(400, None, r#"{"error":{"message":"bad schema"}}"#);
        assert_eq!(invalid.retry_after_source, "program_default");
        assert!(!invalid.model_specific);
        let unspecified = ProviderFailure::from_response(429, None, "{}");
        assert_eq!(unspecified.retry_after, 0);
        assert_eq!(unspecified.retry_after_source, "not_reported");
        let body = ProviderFailure::from_response(
            429,
            None,
            r#"{"error":{"retry_after_seconds":999999}}"#,
        );
        assert_eq!(body.retry_after, 999999);
        assert_eq!(body.retry_after_source, "provider_body");
        assert!(!body.model_specific);
        for status in [400, 429, 500, 502, 503] {
            let failure = ProviderFailure::from_response(status, None, "");
            assert!(Rotation::new(true)
                .next("kilo-auto/free", &failure, &["x/code:free".into()])
                .is_none());
        }
    }
}
