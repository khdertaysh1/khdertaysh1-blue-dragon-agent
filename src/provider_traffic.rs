//! Opt-in bounded metadata only. Never accept request/response bodies here.
use serde_json::{json, Value};
use std::io::Write;
use std::sync::Mutex;
static AUDIT_LOCK: Mutex<()> = Mutex::new(());
pub fn positive_env(name: &str, ceiling: u64) -> Option<u64> {
    std::env::var(name)
        .ok()?
        .parse::<u64>()
        .ok()
        .filter(|v| *v > 0 && *v <= ceiling)
}
pub fn validate_env() -> anyhow::Result<()> {
    for (name, max) in [
        ("BLUE_DRAGON_RPM", 10000),
        ("BLUE_DRAGON_TPM", 100_000_000),
        ("BLUE_DRAGON_INPUT_BUDGET", 1_000_000),
        ("BLUE_DRAGON_MAX_OUTPUT_TOKENS", 16384),
    ] {
        if std::env::var_os(name).is_some() && positive_env(name, max).is_none() {
            anyhow::bail!(
                "Invalid {name}: expected an integer in 1..={max}; no provider request sent"
            );
        }
    }
    Ok(())
}
pub fn is_kilo(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .is_some_and(|u| matches!(u.host_str(), Some("ai-gateway.kilo.ai" | "api.kilo.ai")))
}
pub fn output_limit(url: &str, delegated: bool) -> Option<u64> {
    positive_env("BLUE_DRAGON_MAX_OUTPUT_TOKENS", 16384).or_else(|| {
        (is_kilo(url) || delegated || positive_env("BLUE_DRAGON_TPM", 100_000_000).is_some())
            .then_some(4960)
    })
}
pub fn input_budget(url: &str) -> Option<usize> {
    positive_env("BLUE_DRAGON_INPUT_BUDGET", 1_000_000)
        .map(|v| v as usize)
        .or_else(|| is_kilo(url).then_some(8192))
}
pub fn record(mut event: Value) {
    let Ok(folder) = std::env::var("BLUE_DRAGON_PROVIDER_AUDIT_DIR") else {
        return;
    };
    let _guard = AUDIT_LOCK.lock().unwrap();
    event["audit_schema"] = json!(1);
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&folder)?;
        let path =
            std::path::Path::new(&folder).join(format!("requests-{}.jsonl", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        if file.metadata()?.len() > 16_000_000 {
            return Err(std::io::Error::other("audit file limit reached"));
        }
        writeln!(file, "{event}")
    };
    if write().is_err() {
        eprintln!("Provider audit incomplete: metadata could not be recorded");
    }
}
pub fn headers(headers: &reqwest::header::HeaderMap) -> Value {
    let mut values = serde_json::Map::new();
    for name in [
        "x-ratelimit-limit-requests",
        "x-ratelimit-remaining-requests",
        "x-ratelimit-limit-tokens",
        "x-ratelimit-remaining-tokens",
        "anthropic-ratelimit-requests-limit",
        "anthropic-ratelimit-requests-remaining",
        "anthropic-ratelimit-input-tokens-limit",
        "anthropic-ratelimit-input-tokens-remaining",
        "anthropic-ratelimit-output-tokens-limit",
        "anthropic-ratelimit-output-tokens-remaining",
    ] {
        if let Some(n) = headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
        {
            values.insert(name.into(), json!(n));
        }
    }
    // Only numeric durations; never log arbitrary upstream header text.
    for name in [
        "retry-after",
        "x-ratelimit-reset-requests",
        "x-ratelimit-reset-tokens",
    ] {
        if let Some(v) = headers.get(name).and_then(|v| v.to_str().ok()) {
            if let Some(seconds) = crate::provider_diagnostics::retry_after(v, chrono::Utc::now()) {
                values.insert(name.into(), json!({"seconds":seconds}));
            }
        }
    }
    Value::Object(values)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headers_never_accept_credentials_or_arbitrary_text() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("authorization", "Bearer PRIVATE".parse().unwrap());
        h.insert("x-ratelimit-limit-tokens", "30000".parse().unwrap());
        h.insert("x-ratelimit-remaining-tokens", "SECRET".parse().unwrap());
        let got = headers(&h);
        assert_eq!(got["x-ratelimit-limit-tokens"], 30000);
        assert!(!got.to_string().contains("PRIVATE"));
        assert!(!got.to_string().contains("SECRET"));
    }
}
