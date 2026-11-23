//! Verify quotations against native read results before displaying model prose.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;

pub(crate) const REPORT_INSTRUCTION: &str = r#"Return only one JSON object, without Markdown fences: {"summary":"short interpretation","observations":[{"evidence_id":"E000001","quote":"literal excerpt from that window","note":"what this excerpt shows"}],"limits":"what remains unverified"}. Supply 1–3 observations. State which part of each conclusion the quote supports. When control flow, caller context or runtime behavior is missing, mark the dependent conclusion unverified in the summary as well as the limits. A quoted statement alone does not establish downstream behavior. Every quote must be an exact nonempty substring of the cited read_file_window content (not a search result). Keep the summary and limits under 320 characters each, notes under 220 and quotes under 140. Do not claim full-file or runtime verification unless supported."#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    summary: String,
    observations: Vec<Observation>,
    limits: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    evidence_id: String,
    quote: String,
    note: String,
}

fn check_text(field: &str, text: &str, max: usize) -> Result<()> {
    ensure!(!text.trim().is_empty(), "{field} must not be empty");
    ensure!(
        text.chars().count() <= max,
        "{field} exceeds {max} characters"
    );
    ensure!(
        !text
            .chars()
            .any(|ch| ch.is_control() && ch != '\n' && ch != '\t'),
        "{field} contains unsupported control characters"
    );
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub(crate) struct Checkpoint {
    pub prompt: String,
    pub file: Value,
    pub windows: Vec<Value>,
}

pub(crate) fn saved_windows(
    checkpoint: &Checkpoint,
    session: &crate::session::Session,
) -> (Vec<Value>, Vec<Value>) {
    let mut windows = Vec::new();
    let mut excerpts = Vec::new();
    for action in &checkpoint.windows {
        let window = &action["verified_window"];
        let Some(id) = window["id"].as_str() else {
            continue;
        };
        let Ok((tool, _, result)) = crate::audit::request_for(session, id) else {
            continue;
        };
        if tool == "read_file_window"
            && result["source_sha256"] == checkpoint.file["sha256"]
            && result["path"] == checkpoint.file["path"]
            && result["byte_offset"] == window["start"]
            && result["end_byte_offset"] == window["end"]
            && result["content"].is_string()
        {
            windows.push(action.clone());
            excerpts.push(serde_json::json!({
                "evidence_id": id, "start": window["start"], "end": window["end"],
                "content": result["content"]
            }));
        }
    }
    // The native record retains all verified windows. Only pass a bounded
    // selection to each continuation request.
    let first = excerpts.len().saturating_sub(4);
    excerpts.drain(..first);
    (windows, excerpts)
}

pub(crate) fn coverage_line(meta: &Value, actions: &[Value]) -> String {
    let bytes = meta["bytes"].as_u64().unwrap_or(0);
    let coverage = unique_window_bytes(actions);
    format!("Byte-window coverage: {coverage}/{bytes} unique bytes. Search matches are excluded.")
}

fn unique_window_bytes(actions: &[Value]) -> u64 {
    let mut spans = Vec::new();
    for action in actions {
        if let Some(w) = action.get("verified_window") {
            if let (Some(start), Some(end)) = (w["start"].as_u64(), w["end"].as_u64()) {
                spans.push((start, end));
            }
        }
    }
    spans.sort_unstable();
    let mut end = 0;
    let mut coverage = 0;
    for (start, stop) in spans {
        coverage += stop.saturating_sub(start.max(end));
        end = end.max(stop);
    }
    coverage
}

pub(crate) fn render(
    answer: &str,
    meta: &Value,
    actions: &[Value],
    session: &crate::session::Session,
) -> Result<String> {
    let report: Report = serde_json::from_str(answer.trim())
        .context("Model did not return the required structured file report")?;
    check_text("summary", &report.summary, 320)?;
    check_text("limits", &report.limits, 320)?;
    ensure!(
        (1..=3).contains(&report.observations.len()),
        "observations must contain 1 to 3 entries"
    );
    let mut out = format!(
        "File: {} · {} bytes\nSHA-256: {}\n\nSummary (model): {}\n",
        meta["path"],
        meta["bytes"],
        meta["sha256"].as_str().unwrap_or(""),
        report.summary
    );
    out.push_str(&coverage_line(meta, actions));
    out.push('\n');
    for (index, observation) in report.observations.into_iter().enumerate() {
        let field = format!("observations[{index}]");
        check_text(&format!("{field}.quote"), &observation.quote, 140)?;
        check_text(&format!("{field}.note"), &observation.note, 220)?;
        let w = actions
            .iter()
            .find_map(|a| {
                a.get("verified_window")
                    .filter(|w| w["id"] == observation.evidence_id)
            })
            .with_context(|| {
                format!("{field}.evidence_id did not cite a byte window from this review")
            })?;
        let (tool, _, result) = crate::audit::request_for(session, &observation.evidence_id)?;
        ensure!(
            tool == "read_file_window"
                && result["source_sha256"] == meta["sha256"]
                && result["path"] == meta["path"]
                && result["byte_offset"] == w["start"]
                && result["end_byte_offset"] == w["end"],
            "Observation did not cite the verified file"
        );
        let offset = result["content"]
            .as_str()
            .and_then(|c| c.find(&observation.quote))
            .context("Quoted text was not present in the cited byte window")?
            as u64;
        let start = w["start"].as_u64().context("Invalid window offset")? + offset;
        let stop = start + observation.quote.len() as u64;
        out.push_str(&format!(
            "- {} [{start}..{stop} bytes] {:?}\n  Interpretation (model): {}\n",
            observation.evidence_id, observation.quote, observation.note
        ));
    }
    out.push_str(&format!("Limits (model): {}", report.limits));
    Ok(out)
}
