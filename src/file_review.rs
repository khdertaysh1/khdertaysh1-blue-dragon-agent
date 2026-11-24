//! Verify quotations against native read results before displaying model prose.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;

pub(crate) const REPORT_INSTRUCTION: &str = r#"Return only one JSON object, without Markdown fences: {"summary":"short interpretation","observations":[{"evidence_id":"E000001","quote":"literal excerpt from that window","note":"what this excerpt shows"}],"limits":"what remains unverified"}. Supply 1–3 observations. State which part of each conclusion the quote supports. When control flow, caller context or runtime behavior is missing, mark the dependent conclusion unverified in the summary as well as the limits. For a conditional, identify which branch returns, throws, or falls through. If the branch boundary is outside the window, qualify the conclusion. A quoted statement alone does not establish downstream behavior. Every quote must be an exact nonempty substring of the cited read_file_window content (not a search result). Keep the summary and limits under 320 characters each, notes under 220 and quotes under 140. Do not claim full-file or runtime verification unless supported."#;

pub(crate) fn requests_tail(prompt: &str) -> bool {
    let Some((_, task)) = prompt
        .split_once("\nUser task: ")
        .or_else(|| prompt.split_once("state the limits of interpretation. User task: "))
    else {
        return false;
    };
    let task = task
        .strip_suffix(REPORT_INSTRUCTION)
        .unwrap_or(task)
        .to_lowercase();
    [
        "last function",
        "final function",
        "last declaration",
        "final declaration",
        "آخر دالة",
        "اخر دالة",
        "آخر سطر",
        "اخر سطر",
    ]
    .iter()
    .any(|phrase| task.contains(*phrase))
}

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
    let (coverage, next) = window_progress(bytes, actions);
    let next = next.map_or_else(|| "none".to_owned(), |offset| offset.to_string());
    format!("Byte-window coverage: {coverage}/{bytes} unique bytes. Search matches are excluded. First unread byte: {next}.")
}

pub(crate) fn window_progress(bytes: u64, actions: &[Value]) -> (u64, Option<u64>) {
    let mut spans = Vec::new();
    for action in actions {
        let w = &action["verified_window"];
        if let (Some(start), Some(end)) = (w["start"].as_u64(), w["end"].as_u64()) {
            if start < end && end <= bytes {
                spans.push((start, end));
            }
        }
    }
    spans.sort_unstable();
    let mut covered = 0;
    let mut end = 0;
    let mut prefix = 0;
    for (start, stop) in spans {
        covered += stop.saturating_sub(start.max(end));
        end = end.max(stop);
        if start <= prefix {
            prefix = prefix.max(stop);
        }
    }
    (covered, (prefix < bytes).then_some(prefix))
}

pub(crate) fn render(
    answer: &str,
    meta: &Value,
    actions: &[Value],
    session: &crate::session::Session,
    tail_requested: bool,
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
    let mut cites_tail = false;
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
        cites_tail |= stop > meta["bytes"].as_u64().unwrap_or(0).saturating_sub(2048);
        out.push_str(&format!(
            "- {} [{start}..{stop} bytes] {:?}\n  Interpretation (model): {}\n",
            observation.evidence_id, observation.quote, observation.note
        ));
    }
    ensure!(
        !tail_requested || cites_tail,
        "A review of the file ending requires a quotation from its final window"
    );
    out.push_str(&format!("Limits (model): {}", report.limits));
    Ok(out)
}

#[cfg(test)]
mod progress_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tail_goal_is_read_from_the_task_not_snapshot_metadata() {
        assert!(requests_tail("Review.\nUser task: اشرح آخر دالة"));
        assert!(requests_tail(
            "Review.\nUser task: Read the last declaration."
        ));
        assert!(!requests_tail(
            "last function.js\nUser task: explain imports"
        ));
    }

    #[test]
    fn resume_skips_overlaps_and_stops_at_the_first_gap() {
        let span = |a, b| json!({"verified_window":{"start":a,"end":b}});
        let spans = [
            span(12, 20),
            span(0, 4),
            span(2, 8),
            span(0, 4),
            span(4, 99),
        ];
        assert_eq!(window_progress(20, &spans), (16, Some(8)));
        assert_eq!(window_progress(8, &spans), (8, None));
        assert_eq!(window_progress(0, &[]), (0, None));
    }
}
