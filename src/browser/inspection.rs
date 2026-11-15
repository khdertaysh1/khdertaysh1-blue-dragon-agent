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
            let mut changed = false;
            target
                .query_pairs_mut()
                .clear()
                .extend_pairs(pairs.iter().map(|(k, v)| {
                    let value = if k == &key && !changed {
                        changed = true;
                        payload
                    } else {
                        v.as_str()
                    };
                    (k.as_str(), value)
                }));
            variants.push((format!("query:{key}"), target.to_string()));
        }
    }
    Ok(variants)
}
pub fn probe_document_matches(expected: &str, actual: &str) -> bool {
    match (reqwest::Url::parse(expected), reqwest::Url::parse(actual)) {
        (Ok(expected), Ok(actual)) => expected == actual,
        _ => false,
    }
}
/// Summarize only allowlisted response headers; duplicates/binary/oversized
/// values are incomplete, never silently reduced to a single trusted value.
pub fn http_security_header_indicators(headers: &reqwest::header::HeaderMap) -> Value {
    let mut selected = serde_json::Map::new();
    for name in [
        "content-security-policy",
        "content-security-policy-report-only",
        "x-frame-options",
        "x-content-type-options",
    ] {
        let mut values = headers.get_all(name).iter();
        if let Some(value) = values.next() {
            let value = if values.next().is_some() {
                Value::Null
            } else {
                value
                    .to_str()
                    .ok()
                    .filter(|v| v.len() <= 2000)
                    .map(|v| Value::String(v.to_owned()))
                    .unwrap_or(Value::Null)
            };
            selected.insert(name.to_owned(), value);
        }
    }
    security_header_indicators(&Value::Object(selected))
}
pub fn security_header_indicators(headers: &Value) -> Value {
    let available = headers.is_object();
    let mut complete = headers.as_object().is_some_and(|h| h.len() <= 200);
    let mut selected = std::collections::BTreeMap::new();
    for (name, value) in headers.as_object().into_iter().flatten().take(200) {
        if name.len() > 80 {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if !matches!(
            key.as_str(),
            "content-security-policy"
                | "content-security-policy-report-only"
                | "x-frame-options"
                | "x-content-type-options"
        ) {
            continue;
        }
        if selected.contains_key(&key) {
            complete = false;
            continue;
        }
        selected.insert(key.clone(), "");
        if let Some(value) = value
            .as_str()
            .filter(|v| v.len() <= 2000 && !v.contains(['\r', '\n']))
        {
            selected.insert(key, value);
        } else {
            complete = false;
        }
    }
    let get = |name: &str| selected.get(name).copied().unwrap_or("");
    let frame = |value: &str| {
        value.split(';').any(|d| {
            d.split_whitespace()
                .next()
                .is_some_and(|n| n.eq_ignore_ascii_case("frame-ancestors"))
        })
    };
    let xframe = get("x-frame-options").trim();
    json!({"available":available,"complete":complete,"x_frame_options_known_value_seen":xframe.eq_ignore_ascii_case("deny")||xframe.eq_ignore_ascii_case("sameorigin"),"csp_header_frame_directive_seen":frame(get("content-security-policy")),"report_only_frame_directive_seen":frame(get("content-security-policy-report-only")),"nosniff_value_seen":get("x-content-type-options").trim().eq_ignore_ascii_case("nosniff")})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn http_headers_duplicates_binary_and_limits_are_incomplete() {
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut headers = HeaderMap::new();
        headers.insert("set-cookie", HeaderValue::from_static("PRIVATE_COOKIE"));
        headers.insert(
            "content-security-policy-report-only",
            HeaderValue::from_static("frame-ancestors 'none'; report-uri /PRIVATE"),
        );
        let summary = http_security_header_indicators(&headers);
        assert_eq!(summary["complete"], true);
        assert_eq!(summary["report_only_frame_directive_seen"], true);
        assert_eq!(summary["csp_header_frame_directive_seen"], false);
        assert!(!summary.to_string().contains("PRIVATE"));
        headers.append(
            "content-security-policy-report-only",
            HeaderValue::from_static("frame-ancestors 'self'"),
        );
        assert_eq!(http_security_header_indicators(&headers)["complete"], false);
        for value in [
            HeaderValue::from_bytes(&[0xff]).unwrap(),
            HeaderValue::from_str(&"x".repeat(2001)).unwrap(),
        ] {
            let mut h = HeaderMap::new();
            h.insert("x-frame-options", value);
            assert_eq!(http_security_header_indicators(&h)["complete"], false);
        }
        assert_eq!(
            http_security_header_indicators(&HeaderMap::new())["complete"],
            true
        );
    }
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
    fn probe_changes_only_one_duplicate_query_and_requires_exact_document() {
        let variants = probe_urls(
            "https://example.test/?q=one&q=two&token=secret",
            "canary",
            false,
            true,
        )
        .unwrap();
        let parsed = reqwest::Url::parse(&variants[0].1).unwrap();
        assert_eq!(
            parsed
                .query_pairs()
                .map(|(_, v)| v.into_owned())
                .collect::<Vec<_>>(),
            vec!["canary", "two", "secret"]
        );
        assert!(probe_document_matches(&variants[0].1, &variants[0].1));
        assert!(!probe_document_matches(
            "https://example.test/allowed#canary",
            "https://example.test/other#canary"
        ));
        assert!(!probe_document_matches(
            "https://example.test/allowed#canary",
            "https://example.test/allowed#changed"
        ));
    }
    #[test]
    fn policy_indicator_limits_match_extension_contract_cases() {
        let cases: Value = serde_json::from_str(include_str!(
            "../../scripts/fixtures/policy-header-cases.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let headers = case["headers"]
                .as_array()
                .map(|rows| {
                    Value::Object(
                        rows.iter()
                            .map(|row| {
                                (
                                    row["name"].as_str().unwrap().to_owned(),
                                    row["value"].clone(),
                                )
                            })
                            .collect(),
                    )
                })
                .unwrap_or(Value::Null);
            let result = security_header_indicators(&headers);
            assert_eq!(result["available"], case["available"], "{}", case["name"]);
            assert_eq!(result["complete"], case["complete"], "{}", case["name"]);
            assert!(!result.to_string().contains("PRIVATE_"));
        }
    }
    #[test]
    fn security_header_collection_excludes_credentials_and_cookies() {
        let result = security_header_indicators(
            &json!({"Content-Security-Policy":"default-src 'self'", "Set-Cookie":"secret", "Authorization":"secret", "X-Frame-Options":"DENY"}),
        );
        assert_eq!(result["available"], true);
        assert_eq!(result["complete"], true);
        assert_eq!(result["x_frame_options_known_value_seen"], true);
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
