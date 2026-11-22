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
    let mut events = Vec::new();
    for row in selected.iter().skip(offset).take(limit) {
        let mut event = serde_json::Map::new();
        for key in [
            "event",
            "request_id",
            "type",
            "method",
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
        events.push(event);
    }
    let end = offset.saturating_add(events.len());
    Ok(
        json!({"events":events,"retained_events":rows.len(),"matched_events":selected.len(),"offset":offset,"returned_events":events.len(),"next_offset":(end < selected.len()).then_some(end),"capture_entries_omitted":record["entries_omitted"],"ring_entries_omitted":record["ring_entries_omitted"],"historical":true,"reran_tool":false}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
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
