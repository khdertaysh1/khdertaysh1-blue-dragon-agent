//! Validate literal code in file-evidence reports before showing the final answer.
use crate::session::Session;

pub(crate) fn requested(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    [
        "quote",
        "verbatim",
        "exact condition",
        "exact statement",
        "exact changed line",
        "exact saved line",
        "prediction from",
        "اقتب",
        "حرفي",
        "توقع",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

pub(crate) fn source_error(
    session: &Session,
    start: usize,
    prompt: &str,
    answer: &str,
) -> Option<String> {
    if !requested(prompt) {
        return None;
    }
    let referenced = |text: &str, id: &str| {
        text.split(|c: char| !c.is_ascii_alphanumeric())
            .any(|word| word == id)
    };
    let lower = prompt.to_lowercase();
    let saved_line = [
        "changed line",
        "saved line",
        "line from a final file read",
        "line from disk",
    ]
    .iter()
    .any(|term| lower.contains(term));
    if saved_line {
        let mut pending = std::collections::BTreeMap::new();
        for o in session
            .observations
            .iter()
            .filter(|o| o.ok && o.message_index > start)
        {
            if !matches!(
                o.tool.as_str(),
                "edit_file" | "write_file" | "multi_edit_file" | "read_file" | "read_file_window"
            ) {
                continue;
            }
            let Ok((_, _, result)) = crate::audit::request_for(session, &o.id) else {
                continue;
            };
            if crate::reasoning::claims::is_historical(&result) {
                continue;
            }
            let Some(path) = result["path"].as_str() else {
                continue;
            };
            if result["saved_to_disk"] == true {
                pending.insert(path.to_owned(), o.id.as_str());
            } else if matches!(o.tool.as_str(), "read_file" | "read_file_window")
                && result["content"].is_string()
            {
                pending.remove(path);
            }
        }
        if !pending.is_empty() {
            return Some(format!(
                "missing post-edit read for {}",
                pending.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    let reads: Vec<_> = session
        .observations
        .iter()
        .filter(|o| {
            o.ok && matches!(o.tool.as_str(), "read_file" | "read_file_window")
                && (referenced(prompt, &o.id)
                    || (o.message_index > start
                        && (referenced(answer, &o.id) || prompt.contains(".js") || saved_line)))
        })
        .collect();
    if reads.is_empty() {
        return None;
    }
    let mut sources = Vec::new();
    for o in reads {
        if let Ok((_, _, result)) = crate::audit::request_for(session, &o.id) {
            if let Some(body) = result["content"].as_str() {
                sources.push((o.id.as_str(), body.to_owned()));
            }
        }
    }
    let ids = sources
        .iter()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>()
        .join(", ");
    let matches = |s: &str| sources.iter().any(|(_, body)| body.contains(s));
    let mut fence = None;
    let mut block = Vec::new();
    for line in answer.lines() {
        if let Some(lang) = line.trim_start().strip_prefix("```") {
            if let Some(check) = fence.take() {
                if check && !block.is_empty() && !matches(&block.join("\n")) {
                    return Some(ids);
                }
                block.clear();
            } else {
                fence = Some(matches!(
                    lang.trim(),
                    "" | "js" | "javascript" | "ts" | "typescript"
                ));
            }
        } else if fence == Some(true) {
            block.push(line);
        } else if fence.is_none() {
            for snippet in line.split('`').skip(1).step_by(2) {
                if (snippet.contains('(') && snippet.contains(')')
                    || snippet.contains("===")
                    || (saved_line && snippet.contains(" = ")))
                    && !matches(snippet)
                {
                    return Some(ids);
                }
            }
        }
    }
    if fence == Some(true) {
        return Some(ids);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cited_capture_controls_code_quote_even_in_a_later_browser_turn() {
        let root = tempfile::tempdir().unwrap();
        let mut s = crate::session::new(root.path(), "fixture");
        let args = json!({"path":"entry.js"});
        let body = json!({"path":"entry.js","content":"if(hash!=='#pricing'){window.location.replace('#pricing');}"});
        s.messages.push(json!({"role":"assistant","tool_calls":[{"id":"disk","type":"function","function":{"name":"read_file","arguments":args.to_string()}}]}));
        let mut obs = crate::reasoning::observe(s.messages.len(), "read_file", &body);
        let prompt = format!("Check the prediction from {}", obs.id);
        obs.request_sha256 = Some(crate::intelligence::code::hash(args.to_string().as_bytes()));
        s.observations.push(obs);
        s.messages
            .push(json!({"role":"tool","tool_call_id":"disk","content":body.to_string()}));
        assert!(source_error(
            &s,
            10,
            &prompt,
            "Code: `window.location.replace('#pricing');`"
        )
        .is_none());
        assert!(source_error(
            &s,
            10,
            &prompt,
            "```javascript\nhistory.replaceState(null, '', '#pricing');\n```"
        )
        .is_some());
        assert!(source_error(&s, 10, "Write an example", "`history.replaceState()`").is_none());
    }
}
