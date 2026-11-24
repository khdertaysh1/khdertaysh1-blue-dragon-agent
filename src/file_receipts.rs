//! Verify named local deliverables using pinned tool results and workspace reads.
use crate::{audit, session::Session, tools::ToolBox};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn words(prompt: &str) -> impl Iterator<Item = &str> {
    prompt.split_whitespace().map(|w| {
        w.trim_matches(['`', '"', '\'', ',', '،', ':', ';', '(', ')'])
            .trim_end_matches('.')
    })
}

fn files<'a>(prompt: &'a str, ext: &str) -> Vec<&'a str> {
    let mut paths = Vec::new();
    for word in words(prompt) {
        let path = word
            .strip_prefix('و')
            .filter(|p| p.starts_with(|c: char| c.is_ascii_alphanumeric()))
            .unwrap_or(word);
        if path.ends_with(ext) && !path.contains("://") && !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

pub fn manifest(
    prompt: &str,
    tools: &ToolBox,
    session: &Session,
    start: usize,
) -> Result<Option<String>> {
    let reports = files(prompt, ".tsv");
    let inputs = files(prompt, ".js");
    if reports.len() != 1 || inputs.is_empty() || !prompt.to_ascii_lowercase().contains("sha-256") {
        return Ok(None);
    }
    let path = reports[0];
    // Conditional "already exists" and "missing input" exits do not require a new receipt.
    let writes = session
        .observations
        .iter()
        .filter(|o| {
            o.message_index > start
                && o.request_sha256.is_some()
                && o.result_sha256.is_some()
                && o.ok
                && o.tool == "write_file"
        })
        .filter_map(|o| {
            audit::paired_request_for(session, &o.id)
                .ok()
                .map(|(_, a, r)| (o.message_index, a, r))
        });
    let Some((written, _, _)) = writes
        .rev()
        .find(|(_, a, r)| a["path"] == path && r["saved_to_disk"] == true)
    else {
        return Ok(None);
    };
    let data = tools.read_file(path)?;
    let observed = session
        .observations
        .iter()
        .filter(|o| {
            o.message_index > written
                && o.request_sha256.is_some()
                && o.result_sha256.is_some()
                && o.ok
                && o.tool == "read_file"
        })
        .filter_map(|o| audit::paired_request_for(session, &o.id).ok())
        .any(|(_, a, r)| {
            a["path"] == path && r["sha256"] == data["sha256"] && r["content"] == data["content"]
        });
    ensure!(
        observed,
        "{path}: missing matching full report read after save"
    );
    let mut rows = std::collections::BTreeSet::new();
    for line in data["content"]
        .as_str()
        .context("Missing report text")?
        .lines()
    {
        let cells: Vec<_> = line.split('\t').collect();
        ensure!(cells.len() == 3, "{path}: expected three TSV columns");
        let name = cells[0];
        ensure!(
            inputs.contains(&name) && rows.insert(name),
            "{path}: unexpected or duplicate input row"
        );
        let local = tools.local_file_path(name)?;
        ensure!(local.is_file(), "{name}: expected a regular file");
        let mut file = std::fs::File::open(local)?;
        ensure!(
            file.metadata()?.is_file(),
            "{name}: expected a regular file"
        );
        let mut digest = Sha256::new();
        let bytes = std::io::copy(&mut file, &mut digest)?;
        ensure!(
            cells[1].parse::<u64>().ok() == Some(bytes),
            "{name}: size mismatch"
        );
        ensure!(
            cells[2].eq_ignore_ascii_case(&format!("{:x}", digest.finalize())),
            "{name}: SHA-256 mismatch"
        );
    }
    ensure!(rows.len() == inputs.len(), "{path}: missing file row");
    Ok(Some(format!(
        "{path} · {} / {} MATCH (bytes + SHA-256)",
        rows.len(),
        inputs.len()
    )))
}

// Accept a single, literal curl URL and optional destination. Shell programs,
// redirects, configs, output templates and inferred URLs cannot attest equality.
pub(crate) fn curl_request(args: &Value) -> Option<(&str, Option<&str>)> {
    if args["program"] != "curl" {
        return None;
    }
    let mut it = args["args"].as_array()?.iter();
    let (mut url, mut dest) = (None, None);
    while let Some(v) = it.next() {
        let s = v.as_str()?;
        match s {
            "-o" | "--output" if dest.is_none() => dest = Some(it.next()?.as_str()?),
            "-q" | "--silent" | "--show-error" | "--fail" | "--compressed" | "--" => {}
            _ if s.starts_with('-')
                && s.len() > 1
                && s[1..].bytes().all(|c| b"fsS".contains(&c)) => {}
            _ if url.is_none() && (s.starts_with("https://") || s.starts_with("http://")) => {
                url = Some(s)
            }
            _ => return None,
        }
    }
    Some((url?, dest))
}

pub fn download_targets(prompt: &str) -> Vec<&str> {
    let names = files(prompt, ".js");
    let lower = prompt.to_lowercase();
    if names.is_empty()
        || !(lower.contains("رابط") || lower.contains("url") || lower.contains("link"))
        || !["احفظ", "نزّل", "نزل", "download", "save as"]
            .iter()
            .any(|w| lower.contains(w))
        || ![
            "ناقص",
            "بدون تخمين",
            "incomplete",
            "truncated",
            "do not guess",
        ]
        .iter()
        .any(|w| lower.contains(w))
    {
        return vec![];
    }
    names
}

pub fn page_links(page: &reqwest::Url, html: &str) -> Vec<String> {
    let scan = crate::source_discovery::html(html);
    if scan["limited"] == true || scan["base_element_seen"] == true {
        return vec![];
    }
    let mut refs = Vec::new();
    for reference in scan["references"].as_array().into_iter().flatten() {
        let Some(raw) = reference["specifier"].as_str() else {
            break;
        };
        let Ok(url) = page.join(raw) else { break };
        let href = url.to_string();
        if !refs.contains(&href) {
            refs.push(href);
        }
    }
    refs
}

pub fn download(
    prompt: &str,
    session: &Session,
    pending: Option<&Value>,
    tools: &ToolBox,
) -> Result<Option<String>> {
    let names = download_targets(prompt);
    if names.is_empty() {
        return Ok(None);
    }
    let proposed = pending
        .and_then(curl_request)
        .and_then(|(u, d)| d.map(|dest| (u, dest)))
        .filter(|(_, dest)| names.contains(dest));
    if pending.is_some() && proposed.is_none() {
        return Ok(None);
    }
    let origin = |p: &str| {
        words(p).find_map(|w| {
            reqwest::Url::parse(w)
                .ok()
                .filter(|u| matches!(u.scheme(), "http" | "https"))
        })
    };
    let mut page = origin(prompt);
    let mut since = session.user_turns.last().copied().unwrap_or(0);
    if page.is_none() {
        for &at in session.user_turns.iter().rev().skip(1) {
            let Some(old) = session.messages.get(at).and_then(|m| m["content"].as_str()) else {
                continue;
            };
            if !names.iter().all(|p| old.contains(p)) {
                continue;
            }
            page = origin(old);
            if page.is_some() {
                since = at;
                break;
            }
        }
    }
    let page = page.context("Original page URL unavailable for exact download comparison")?;
    let mut refs = Vec::new();
    let mut actual = std::collections::BTreeMap::new();
    let first_effect = session
        .observations
        .iter()
        .filter(|o| {
            o.message_index > since
                && matches!(
                    o.tool.as_str(),
                    "write_file" | "edit_file" | "run_command" | "start_job" | "browser"
                )
        })
        .map(|o| o.message_index)
        .min()
        .unwrap_or(usize::MAX);
    let mut present = std::collections::BTreeSet::new();
    for o in session.observations.iter().filter(|o| {
        o.ok && o.request_sha256.is_some()
            && o.result_sha256.is_some()
            && o.message_index > since
            && matches!(o.tool.as_str(), "run_command" | "web_fetch" | "file_info")
    }) {
        let Ok((tool, args, r)) = audit::paired_request_for(session, &o.id) else {
            continue;
        };
        if tool == "file_info"
            && o.message_index < first_effect
            && r["saved_to_disk"] == true
            && o.message_index > session.user_turns.last().copied().unwrap_or(0)
            && !crate::reasoning::claims::is_historical(&r)
        {
            if let Some(p) = args["path"].as_str() {
                present.insert(p.to_owned());
            }
        }
        if crate::reasoning::claims::is_historical(&r) || r["ok"] != true {
            continue;
        }
        let body = if tool == "web_fetch"
            && args["url"]
                .as_str()
                .and_then(|v| v.parse::<reqwest::Url>().ok())
                .as_ref()
                == Some(&page)
            && r["status"]
                .as_u64()
                .is_some_and(|v| (200..300).contains(&v))
        {
            refs.clear();
            actual.clear();
            if let Some(hashes) = r["script_url_hashes"].as_array() {
                refs.extend(hashes.iter().filter_map(Value::as_str).map(str::to_owned));
                None
            } else if r["truncated"] == false {
                r["text"].as_str()
            } else {
                None
            }
        } else if tool == "run_command" && r["exit_code"] == 0 {
            match curl_request(&args) {
                Some((u, Some(dest))) => {
                    actual.insert(dest.to_owned(), u.to_owned());
                    None
                }
                Some((u, None))
                    if u.parse::<reqwest::Url>().ok().as_ref() == Some(&page)
                        && r["stdout"]
                            .as_str()
                            .is_some_and(|s| s.len() < 40 * 1024 && !s.contains("[truncated")) =>
                {
                    r["stdout"].as_str()
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some(body) = body {
            refs.clear();
            actual.clear();
            refs = page_links(&page, body)
                .iter()
                .map(|u| crate::intelligence::code::hash(u.as_bytes()))
                .collect();
        }
    }
    let mut rows = Vec::new();
    let skip_existing = prompt.contains("موجود") || prompt.to_lowercase().contains("exist");
    for (i, name) in names.iter().enumerate() {
        let used = if let Some((u, d)) = proposed {
            if *name != d {
                continue;
            }
            u
        } else if let Some(u) = actual.get(*name) {
            u.as_str()
        } else if skip_existing && present.contains(*name) {
            let info = tools.file_info(name)?;
            ensure!(
                info["saved_to_disk"] == true,
                "{name}: expected a disk file"
            );
            rows.push(format!(
                "{name} · {} bytes (already present)",
                info["bytes"]
            ));
            continue;
        } else {
            anyhow::bail!("{name}: literal download URL unavailable");
        };
        let expected = refs
            .get(i)
            .with_context(|| format!("{name}: complete original script URL unavailable"))?;
        ensure!(
            &crate::intelligence::code::hash(used.as_bytes()) == expected,
            "{name}: download URL differs from original script URL"
        );
        if pending.is_none() {
            let info = tools.file_info(name)?;
            ensure!(info["saved_to_disk"] == true, "{name}: disk file missing");
            rows.push(format!(
                "{name} · {} bytes (source URL matched)",
                info["bytes"]
            ));
        }
    }
    Ok((!rows.is_empty()).then(|| rows.join("\n")))
}
