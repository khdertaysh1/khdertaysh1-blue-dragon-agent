//! Compact, read-only views over recorded browser traffic. Redirect events stay separate.
use anyhow::{Context, Result};
use serde_json::{json, Value};

pub fn page(record: &Value, id: Option<&str>, offset: usize, limit: usize) -> Result<Value> {
    let rows = record["entries"]
        .as_array()
        .context("No network events in this observation")?;
    let selected: Vec<_> = rows
        .iter()
        .filter(|row| id.is_none() || row["request_id"].as_str() == id)
        .collect();
    let mut responses = std::collections::BTreeMap::<(&str, &str), Vec<&Value>>::new();
    for row in rows
        .iter()
        .filter(|r| r["event"] == "response" && r["url_truncated"] != true)
    {
        if let (Some(id), Some(url)) = (row["request_id"].as_str(), row["url"].as_str()) {
            if row["url_truncated"] == false || url.chars().count() < 2000 {
                responses.entry((id, url)).or_default().push(row);
            }
        }
    }
    let mut events = Vec::new();
    for row in selected.iter().skip(offset).take(limit) {
        let mut event = serde_json::Map::new();
        for key in [
            "event",
            "request_id",
            "type",
            "method",
            "mime_type",
            "security_header_indicators",
            "status",
            "url",
            "url_truncated",
        ] {
            if let Some(value) = row.get(key) {
                event.insert(key.into(), value.clone());
            }
        }
        if event.get("url_truncated").is_none()
            && row["url"]
                .as_str()
                .is_some_and(|u| u.chars().count() >= 2000)
        {
            event.insert("url_completeness_unknown".into(), json!(true));
        }
        if row["event"] == "request"
            && row["url_truncated"] != true
            && !event.contains_key("url_completeness_unknown")
        {
            if let (Some(id), Some(url)) = (row["request_id"].as_str(), row["url"].as_str()) {
                let statuses: Vec<_> = responses
                    .get(&(id, url))
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r.get("status").filter(|v| v.is_number()))
                    .collect();
                event.insert(
                    "recorded_response_statuses_for_id_and_url".into(),
                    json!(statuses),
                );
            }
        }
        events.push(event);
    }
    let end = offset.saturating_add(events.len());
    Ok(
        json!({"events":events,"coverage":{"raw_headers_examined":false,"request_body_examined":false,"response_body_examined":false,"ring_omission_scope":record["ring_omission_scope"]},"retained_events":rows.len(),"matched_events":selected.len(),"offset":offset,"returned_events":events.len(),"next_offset":(end < selected.len()).then_some(end),"capture_entries_omitted":record["entries_omitted"],"ring_entries_omitted":record["ring_entries_omitted"],"historical":true,"reran_tool":false}),
    )
}
/// Pair only a retained request with its own response. A new request starts
/// a new hop even if a redirect reuses both its ID and URL.
pub fn request_rows(record: &Value, offset: usize, limit: usize) -> Result<Value> {
    let entries = record["entries"].as_array().context("No recorded events")?;
    let mut rows: Vec<Value> = Vec::new();
    let mut latest = std::collections::BTreeMap::new();
    for entry in entries {
        let event = entry["event"].as_str().unwrap_or("");
        if !matches!(event, "request" | "response" | "redirect" | "error") {
            continue;
        }
        let id = entry["request_id"].as_str().filter(|id| !id.is_empty());
        let url = entry["url"].as_str();
        let complete_url = url.is_some_and(|u| {
            !u.is_empty()
                && (entry["url_truncated"] == false
                    || (entry["url_truncated"].is_null() && u.chars().count() < 2000))
        });
        let paired = id
            .and_then(|id| latest.get(id).copied())
            .filter(|index: &usize| {
                let row = &rows[*index];
                event != "request"
                    && (row["outcome"] == "response_unobserved"
                        || (event == "error" && row["outcome"] == "response_observed"))
                    && ((complete_url && row["url_complete"] == true && row["url"] == entry["url"])
                        || (event == "error" && url.is_none()))
            });
        let index = paired.unwrap_or_else(|| {
            let index = rows.len();
            rows.push(
                json!({"request_id":id,"url":url,"url_complete":complete_url,
                "method":entry["method"],"request_observed":event=="request",
                "outcome":"response_unobserved"}),
            );
            if let Some(id) = id {
                latest.insert(id, index);
            }
            index
        });
        if event != "request" {
            let row = &mut rows[index];
            row["outcome"] = json!(match event {
                "redirect" => "redirect_observed",
                "error" => "failure_observed",
                _ => "response_observed",
            });
            for key in ["status", "mime_type", "error"] {
                if !entry[key].is_null() {
                    row[key] = entry[key].clone();
                }
            }
        }
    }
    let total = rows.len();
    let selected = rows
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let end = offset.saturating_add(selected.len());
    Ok(
        json!({"requests":selected,"retained_request_rows":total,"offset":offset,
        "next_offset":(end<total).then_some(end),"historical":true,
        "capture_entries_omitted":record["entries_omitted"],
        "ring_entries_omitted":record["ring_entries_omitted"],"ring_omission_scope":record["ring_omission_scope"]}),
    )
}

pub fn inventory(record: &Value, offset: usize) -> Result<Value> {
    use std::collections::{BTreeMap, BTreeSet};
    let rows = record["entries"]
        .as_array()
        .context("No captured network entries")?;
    #[derive(Default)]
    struct Resource<'a> {
        kinds: BTreeSet<&'a str>,
        ids: BTreeSet<&'a str>,
        responses: Vec<Value>,
    }
    let mut resources: BTreeMap<&str, Resource<'_>> = BTreeMap::new();
    let mut incomplete = 0;
    for row in rows {
        let Some(url) = row["url"].as_str().filter(|s| !s.is_empty()) else {
            incomplete += 1;
            continue;
        };
        if row["url_truncated"] == true
            || (row.get("url_truncated").is_none() && url.chars().count() >= 2000)
        {
            incomplete += 1;
            continue;
        }
        let item = resources.entry(url).or_default();
        if let Some(kind) = row["type"].as_str() {
            item.kinds.insert(kind);
        }
        if let Some(id) = row["request_id"].as_str() {
            item.ids.insert(id);
            if row["event"] == "response" {
                item.responses.push(
                    json!({"request_id":id,"status":row["status"],"mime_type":row["mime_type"]}),
                );
            }
        }
    }
    let total = resources.len();
    let items:Vec<_>=resources.into_iter().skip(offset).take(8).map(|(url,r)|
        json!({"url":url,"observed_types":r.kinds,"request_ids":r.ids,"observed_responses":r.responses,"body_examined":false})
    ).collect();
    let end = offset.saturating_add(items.len());
    Ok(
        json!({"resources":items,"unique_complete_urls_in_capture":total,"retained_events":rows.len(),"incomplete_url_events":incomplete,"capture_entries_omitted":record["entries_omitted"],"ring_entries_omitted":record["ring_entries_omitted"],"next_offset":(end<total).then_some(end),"historical":true,"reran_tool":false}),
    )
}

/// Bounded selection metadata from this capture; the normal browser save still
/// checks tab ownership, response identity, content and write approval.
pub(crate) fn file_selection(record: &Value, page: &str, id: &str) -> Option<Value> {
    let origin = reqwest::Url::parse(page).ok()?.origin();
    let entries = record["entries"].as_array()?;
    let mut candidates = Vec::new();
    for row in entries {
        let (Some(url), Some(request), Some(status)) = (
            row["url"].as_str(),
            row["request_id"].as_str(),
            row["status"].as_u64(),
        ) else {
            continue;
        };
        if row["event"] != "response"
            || !(200..300).contains(&status)
            || row["url_truncated"] == true
            || url.len() >= 2000
            || request.is_empty()
            || request.len() > 128
        {
            continue;
        }
        let Ok(parsed) = reqwest::Url::parse(url) else {
            continue;
        };
        if parsed.origin() != origin || !matches!(parsed.scheme(), "http" | "https") {
            continue;
        }
        if row["mime_type"]
            .as_str()
            .is_some_and(|s| s.contains("javascript"))
            || row["type"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("script"))
            || parsed.path().ends_with(".js")
            || parsed.path().ends_with(".mjs")
        {
            candidates.push(json!({"request_id":request,"url":url,"status":status}));
            if candidates.len() == 2 {
                break;
            }
        }
    }
    Some(
        json!({"network_evidence":id,"observed_script_responses":candidates,
        "body_saved_by_network":false,
        "next_step":"If an entry matches the requested file, use browser save_response with its request_id and exact URL. Otherwise report the selection gap or consult the existing capture."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_selection_skips_fonts_errors_and_other_origins() {
        let entries = [("a.woff2", 200), ("bad.js", 404), ("https://cdn.test/x.js", 200), ("app.js", 200)]
            .map(|(path, status)| json!({"event":"response","request_id":path,"status":status,
                "url":reqwest::Url::parse("https://example.test/").unwrap().join(path).unwrap().as_str()}));
        let view =
            file_selection(&json!({"entries":entries}), "https://example.test/", "E7").unwrap();
        assert_eq!(
            view["observed_script_responses"].as_array().unwrap().len(),
            1
        );
        assert_eq!(view["observed_script_responses"][0]["request_id"], "app.js");
        assert_eq!(view["body_saved_by_network"], false);
    }
    #[test]
    fn request_rows_keep_redirect_hops_failures_and_unpaired_responses() {
        let record = json!({"entries":[
            {"event":"request","request_id":"r","url":"a","method":"GET"},
            {"event":"redirect","request_id":"r","url":"a","status":302},
            {"event":"request","request_id":"r","url":"b","method":"GET"},
            {"event":"response","request_id":"r","url":"b","status":200},
            {"event":"error","request_id":"r","error":"connection reset"},
            {"event":"response","request_id":"other","url":"b","status":404},
            {"event":"request","request_id":"pending","url":"c","method":"POST"}],
            "entries_omitted":3,"ring_entries_omitted":7});
        let first = request_rows(&record, 0, 2).unwrap();
        assert_eq!(first["retained_request_rows"], 4);
        assert_eq!(first["requests"][0]["outcome"], "redirect_observed");
        assert_eq!(first["requests"][1]["status"], 200);
        assert_eq!(first["requests"][1]["outcome"], "failure_observed");
        assert_eq!(first["next_offset"], 2);
        let next = request_rows(&record, 2, 2).unwrap();
        assert_eq!(next["requests"][0]["request_observed"], false);
        assert_eq!(next["requests"][1]["outcome"], "response_unobserved");
        assert_eq!(next["ring_entries_omitted"], 7);
        assert!(next["next_offset"].is_null());
    }

    #[test]
    fn historical_network_exposes_saved_indicators_without_inferring_body_contents() {
        let record = json!({"entries":[{"event":"response","mime_type":"text/css","security_header_indicators":{"available":true},"headers":{"Cookie":"private"},"body":"private"}]});
        let out = page(&record, None, 0, 8).unwrap();
        assert_eq!(
            out["events"][0]["security_header_indicators"]["available"],
            true
        );
        assert_eq!(out["events"][0]["mime_type"], "text/css");
        assert_eq!(out["coverage"]["response_body_examined"], false);
        assert!(!out.to_string().contains("private"));
    }
    #[test]
    fn inventory_excludes_partial_urls_and_attaches_status_to_recorded_url() {
        let record = json!({"entries":[{"url":"https://unit.test/q?x=1","event":"request","type":"Script","request_id":"r"},{"url":"https://unit.test/q?x=2","event":"response","request_id":"r","status":200},{"url":"partial","url_truncated":true}],"ring_entries_omitted":154,"entries_omitted":0});
        let out = inventory(&record, 0).unwrap();
        assert_eq!(out["unique_complete_urls_in_capture"], 2);
        assert_eq!(out["ring_entries_omitted"], 154);
        assert_eq!(out["incomplete_url_events"], 1);
        assert_eq!(out["resources"][0]["observed_responses"], json!([]));
        assert_eq!(out["resources"][1]["observed_responses"][0]["status"], 200);
        assert_eq!(out["resources"][1]["body_examined"], false);
    }
    #[test]
    fn large_capture_filters_before_paging_and_keeps_redirects() {
        let mut rows: Vec<_> = (0..299)
            .map(|i| json!({"event":"request","request_id":i.to_string(),"url":"a".repeat(390)}))
            .collect();
        rows.push(json!({"event":"response","request_id":"298","status":200}));
        let data = json!({"entries":rows,"ring_entries_omitted":5});
        assert!(data.to_string().len() > 93460);
        let first = page(&data, Some("298"), 0, 1).unwrap();
        assert_eq!(first["next_offset"], 1);
        assert_eq!(first["retained_events"], 300);
        assert_eq!(first["matched_events"], 2);
        assert_eq!(first["returned_events"], 1);
        assert!(first["events"][0].get("status").is_none());
        let second = page(&data, Some("298"), 1, 1).unwrap();
        assert_eq!(second["events"][0]["status"], 200);
        assert_eq!(second["ring_entries_omitted"], 5);
    }
}
