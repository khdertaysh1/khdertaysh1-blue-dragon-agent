//! Bounded initial evidence collection, not a vulnerability verdict.
use serde_json::{json, Value};
pub fn bare_url(prompt: &str) -> Option<&str> {
    let value = prompt.trim();
    (!value.chars().any(char::is_whitespace) && super::validate_url(value).is_ok()).then_some(value)
}
pub fn indicators(source: &str) -> Vec<Value> {
    let mut rows = Vec::new();
    for (index, line) in source.lines().enumerate() {
        for (kind, token) in [
            ("input_source", "location.search"),
            ("input_source", "location.hash"),
            ("input_source", "URLSearchParams"),
            ("input_source", "document.referrer"),
            ("input_source", "postMessage"),
            ("html_sink", "innerHTML"),
            ("html_sink", "outerHTML"),
            ("html_sink", "insertAdjacentHTML"),
            ("html_sink", "document.write"),
            ("code_sink", "eval("),
            ("code_sink", "new Function("),
        ] {
            if line.contains(token) {
                let at = line.find(token).unwrap_or(0);
                let start = line[..at]
                    .char_indices()
                    .rev()
                    .nth(80)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                rows.push(json!({"kind":kind,"token":token,"line":index+1,"excerpt":line[start..].chars().take(220).collect::<String>(),"verdict":"indicator_only_not_a_confirmed_vulnerability"}));
                if rows.len() >= 40 {
                    return rows;
                }
            }
        }
    }
    rows
}
pub fn probe_urls(
    url: &str,
    payload: &str,
    fragment: bool,
    query: bool,
) -> anyhow::Result<Vec<(String, String)>> {
    let original = reqwest::Url::parse(url)?;
    let mut variants = Vec::new();
    if fragment {
        let mut target = original.clone();
        target.set_fragment(Some(payload));
        variants.push(("fragment".into(), target.to_string()));
    }
    if query {
        let pairs = original
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>();
        if let Some(key) = pairs
            .iter()
            .find(|(key, _)| {
                matches!(
                    key.as_str(),
                    "q" | "query" | "search" | "s" | "term" | "keyword"
                )
            })
            .map(|p| p.0.clone())
        {
            let mut target = original.clone();
            target.query_pairs_mut().clear().extend_pairs(
                pairs
                    .iter()
                    .map(|(k, v)| (k.as_str(), if k == &key { payload } else { v.as_str() })),
            );
            variants.push((format!("query:{key}"), target.to_string()));
        }
    }
    Ok(variants)
}
pub fn security_headers(headers: &Value) -> Value {
    let mut selected = serde_json::Map::new();
    if let Some(headers) = headers.as_object() {
        for (name, value) in headers {
            let key = name.to_ascii_lowercase();
            if matches!(
                key.as_str(),
                "content-security-policy"
                    | "content-security-policy-report-only"
                    | "strict-transport-security"
                    | "x-content-type-options"
                    | "x-frame-options"
                    | "referrer-policy"
                    | "permissions-policy"
            ) {
                selected.insert(
                    key,
                    json!(value
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(2000)
                        .collect::<String>()),
                );
            }
        }
    }
    Value::Object(selected)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probes_only_fragment_and_existing_search_parameter_preserving_other_parameters() {
        let variants = probe_urls(
            "https://example.com/search?q=hello&token=kept",
            "<svg>",
            true,
            true,
        )
        .unwrap();
        assert_eq!(variants.len(), 2);
        let changed = reqwest::Url::parse(&variants[1].1).unwrap();
        assert!(changed
            .query_pairs()
            .any(|(k, v)| k == "token" && v == "kept"));
        assert!(changed.query_pairs().any(|(k, v)| k == "q" && v == "<svg>"));
        assert_eq!(
            probe_urls("https://example.com/?action=delete", "x", false, true)
                .unwrap()
                .len(),
            0
        );
    }
    #[test]
    fn security_header_collection_excludes_credentials_and_cookies() {
        let result = security_headers(
            &json!({"Content-Security-Policy":"default-src 'self'", "Set-Cookie":"secret", "Authorization":"secret", "X-Frame-Options":"DENY"}),
        );
        assert_eq!(result.as_object().unwrap().len(), 2);
        assert!(!result.to_string().contains("secret"));
    }
    #[test]
    fn url_intent_and_source_hints_are_bounded_not_verdicts() {
        assert_eq!(
            bare_url(" https://example.com/?a=1 "),
            Some("https://example.com/?a=1")
        );
        for s in [
            "hello",
            "open https://example.com",
            "file:///tmp/a",
            "https://u:p@example.com",
        ] {
            assert!(bare_url(s).is_none());
        }
        let hints = indicators("const q=location.hash;\nel.innerHTML=q;");
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[1]["line"], 2);
        assert_eq!(indicators(&"eval(x)\n".repeat(1000)).len(), 40);
    }
}
