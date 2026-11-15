//! Redacted selected error fields only; no raw response/header/transcript logging.
use serde_json::{json, Value};

pub fn scrub(value: &str, key: Option<&str>, messages: &[Value]) -> String {
    let mut text = value.to_string();
    if let Some(key) = key.filter(|k| !k.is_empty()) {
        text = text.replace(key, "[credential redacted]");
    }
    // Error messages sometimes echo the rejected private content/arguments.
    fn strings(value: &Value, text: &mut String, depth: usize) {
        if depth > 16 {
            return;
        }
        match value {
            Value::String(s) => {
                if s.len() >= 8 && text.contains(s.as_str()) {
                    *text = text.replace(s, "[request text redacted]");
                }
                for token in s.split_whitespace().filter(|s| s.len() >= 16) {
                    if text.contains(token) {
                        *text = text.replace(token, "[request token redacted]");
                    }
                }
            }
            Value::Array(values) => {
                for v in values {
                    strings(v, text, depth + 1);
                }
            }
            Value::Object(values) => {
                for v in values.values() {
                    strings(v, text, depth + 1);
                }
            }
            _ => {}
        }
    }
    for message in messages {
        strings(message, &mut text, 0);
    }
    // Strip common credential assignments, including credentials unrelated to the
    // configured key. This is not a guarantee against arbitrary prose secrets.
    for marker in [
        "bearer ",
        "authorization:",
        "x-api-key:",
        "api_key=",
        "api-key=",
        "token=",
        "password=",
        "sk-",
    ] {
        loop {
            let lower = text.to_ascii_lowercase();
            let Some(at) = lower.find(marker) else {
                break;
            };
            let tail = &text[at + marker.len()..];
            let end = tail
                .find(['\n', '\r', ',', ';'])
                .map(|n| at + marker.len() + n)
                .unwrap_or(text.len());
            text.replace_range(at..end, "[sensitive field redacted]");
        }
    }
    crate::reasoning::context::clip(
        &text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>(),
        512,
    )
}
pub fn retry_after(value: &str, now: chrono::DateTime<chrono::Utc>) -> Option<u64> {
    value.trim().parse().ok().or_else(|| {
        chrono::DateTime::parse_from_rfc2822(value)
            .ok()
            .map(|date| {
                let delay = date.with_timezone(&chrono::Utc) - now;
                let seconds = delay.num_seconds().max(0) as u64;
                seconds + u64::from(delay > chrono::Duration::seconds(seconds as i64))
            })
    })
}
pub fn metadata(failure: &crate::rotation::ProviderFailure) -> Value {
    json!({"http_status":failure.status,"rate_limit_dimension":if failure.code.as_deref()==Some("tpm_rate_limit_exceeded") {"tokens_per_minute"} else {"unknown"},"provider_error_code":failure.code,"reason_redacted":failure.reason,"request_id":failure.request_id,"cooldown_seconds":failure.retry_after,"cooldown_source":failure.retry_after_source,"context_exceeded":failure.context_exceeded,"model_specific_evidence":failure.model_specific,"raw_response_retained":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redact_credentials_and_echoed_private_content_without_raw_headers() {
        let private = "private conversation content must never be logged";
        let got = scrub(
            &format!("Invalid arguments: {private}; key=actual-key; Bearer OTHER_SECRET"),
            Some("actual-key"),
            &[json!({"content":private})],
        );
        assert!(got.contains("Invalid arguments"));
        assert!(!got.contains(private));
        assert!(!got.contains("actual-key"));
        assert!(!got.contains("OTHER_SECRET"));
    }
    #[test]
    fn header_date_and_long_delay_are_preserved() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-15T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(retry_after("864001", now), Some(864001));
        assert_eq!(retry_after("0", now), Some(0));
        assert_eq!(retry_after("Sun, 15 Nov 2026 00:02:00 GMT", now), Some(120));
        assert_eq!(retry_after("invalid", now), None);
        assert_eq!(
            retry_after(
                "Sun, 15 Nov 2026 00:02:00 GMT",
                now + chrono::Duration::milliseconds(900)
            ),
            Some(120)
        );
    }
}
