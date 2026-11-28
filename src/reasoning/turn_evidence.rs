//! Validate explicit requests for new evidence against this session's native records.
use crate::session::Session;
use anyhow::{ensure, Result};

pub(crate) fn archived_reply(text: &str) -> bool {
    text.trim_start()
        .starts_with("Historical assistant statement (not verified fact), message ")
}

// Also validate citations volunteered in an operational answer when the user
// didn't explicitly ask for IDs. Quoted evidence from the prompt is input only.
pub(crate) fn operational_prompt(prompt: &str) -> bool {
    let lower = prompt.trim_start().to_lowercase();
    let lower = lower.strip_prefix("please ").unwrap_or(&lower);
    matches!(
        lower.split_whitespace().next().unwrap_or(""),
        "run"
            | "execute"
            | "read"
            | "save"
            | "inspect"
            | "open"
            | "check"
            | "list"
            | "search"
            | "write"
            | "edit"
            | "شغل"
            | "شغّل"
            | "نفذ"
            | "نفّذ"
            | "اقرأ"
            | "احفظ"
            | "افحص"
            | "افتح"
            | "اعرض"
            | "ابحث"
            | "اكتب"
            | "عدّل"
    )
}

pub(crate) fn references_history(prompt: &str) -> bool {
    requests_previous_input(prompt) || evidence_ids(prompt).next().is_some()
}

fn requests_previous_input(prompt: &str) -> bool {
    let text = prompt.to_lowercase();
    [
        "previous task",
        "previous session",
        "earlier result",
        "saved evidence",
        "المهمة السابقة",
        "الطلب السابق",
        "الجلسة السابقة",
        "الدليل السابق",
        "النتيجة القديمة",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

pub(crate) fn validate_volunteered_ids(
    session: &Session,
    start: usize,
    answer: &str,
) -> Result<()> {
    let prompt = session.messages[start]["content"]
        .as_str()
        .unwrap_or_default();
    for id in evidence_ids(answer) {
        if evidence_ids(prompt).any(|input| input == id)
            || (requests_previous_input(prompt)
                && session
                    .observations
                    .iter()
                    .any(|o| o.id == id && o.message_index < start)
                && crate::audit::paired_request_for(session, id).is_ok())
        {
            continue;
        }
        ensure!(
            session
                .observations
                .iter()
                .any(|o| o.id == id && o.message_index > start)
                && crate::audit::paired_request_for(session, id).is_ok(),
            "{id} has no paired observation in this request and session"
        );
    }
    Ok(())
}

enum Operation {
    Network,
    FileRead,
    Command,
    Any,
}

pub(crate) struct Requirement {
    operation: Operation,
    require_id: bool,
}

impl Requirement {
    pub(crate) fn from_prompt(prompt: &str) -> Option<Self> {
        let text = prompt.to_lowercase().replace('ّ', "");
        let network = text.contains("browser network")
            || text.contains("سجل الشبكة")
            || text.contains("network buffer")
            || text.contains("network event buffer");
        let first = text.split_whitespace().next().unwrap_or_default();
        let inspect_network = network
            && matches!(
                first,
                "inspect" | "check" | "capture" | "افحص" | "تحقق" | "افتح"
            );
        let explicit = [
            "new tool evidence id",
            "fresh evidence id",
            "new evidence id",
            "new read evidence id",
            "دليل جديد",
            "رقم الدليل الجديد",
        ]
        .iter()
        .any(|p| text.contains(p));
        // A question about the meaning of a phrase isn't an execution contract.
        let explanatory = [
            "what is ",
            "what does ",
            "explain ",
            "شو يعني",
            "اشرح",
            "ما معنى",
        ];
        (!explanatory.iter().any(|p| text.starts_with(p)) && (inspect_network || explicit))
            .then_some(Self {
                operation: if inspect_network {
                    Operation::Network
                } else if first == "read"
                    || first == "اقرأ"
                    || text.contains("new read evidence id")
                {
                    Operation::FileRead
                } else if matches!(first, "run" | "execute" | "شغل" | "نفذ") {
                    Operation::Command
                } else {
                    Operation::Any
                },
                require_id: explicit || text.contains("evidence id") || text.contains("رقم الدليل"),
            })
    }

    pub(crate) fn validate(&self, session: &Session, start: usize, answer: &str) -> Result<()> {
        let cited: std::collections::BTreeSet<_> = evidence_ids(answer).collect();
        validate_volunteered_ids(session, start, answer)?;
        let mut failure = None;
        for observation in session
            .observations
            .iter()
            .rev()
            .take_while(|o| o.message_index > start)
        {
            let Ok((tool, args, result)) =
                crate::audit::paired_request_for(session, &observation.id)
            else {
                continue;
            };
            let relevant = match self.operation {
                Operation::Network => tool == "browser" && args["action"] == "network",
                Operation::FileRead => matches!(
                    tool.as_str(),
                    "read_file" | "read_file_window" | "read_lines"
                ),
                Operation::Command => tool == "run_command",
                Operation::Any => !matches!(
                    tool.as_str(),
                    "recall_context" | "discover_tools" | "update_plan"
                ),
            };
            if !relevant || super::claims::is_historical(&result) {
                continue;
            }
            if !observation.ok || result.get("error").is_some() {
                failure.get_or_insert_with(|| {
                    format!(
                        "{}: {}",
                        observation.id,
                        result["error"]
                            .as_str()
                            .or_else(|| result["execution_status"].as_str())
                            .unwrap_or("unsuccessful observation")
                    )
                });
                continue;
            }
            if !self.require_id || cited.contains(observation.id.as_str()) {
                return Ok(());
            }
        }
        anyhow::bail!(
            "{}",
            failure.unwrap_or_else(|| {
                "Missing matching current-turn evidence for the requested operation.".into()
            })
        )
    }
}

fn evidence_ids(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|s| {
            s.strip_prefix('E')
                .is_some_and(|n| n.len() >= 6 && n.bytes().all(|b| b.is_ascii_digit()))
        })
}
