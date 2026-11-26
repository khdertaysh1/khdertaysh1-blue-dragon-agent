//! Local Markdown guidance. Automatic consultation is bounded to workspace reads.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_SKILL_BYTES: u64 = 16 * 1024;

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn directory(sessions_dir: &Path) -> Result<PathBuf> {
    let home = sessions_dir
        .parent()
        .context("Missing Blue Dragon data directory")?;
    Ok(home.join("skills"))
}

pub fn list(sessions_dir: &Path) -> Result<Value> {
    let dir = directory(sessions_dir)?;
    if !dir.exists() {
        return Ok(json!({"skills": [BUILTIN_NAME], "directory": dir.display().to_string()}));
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().and_then(|s| s.to_str()) == Some("md") && valid_name(name) {
            names.push(name.to_string());
        }
    }
    names.push(BUILTIN_NAME.to_string());
    names.sort();
    names.dedup();
    names.truncate(100);
    Ok(json!({"skills": names, "directory": dir.display().to_string()}))
}

pub fn load(sessions_dir: &Path, name: &str) -> Result<String> {
    if !valid_name(name) {
        anyhow::bail!("Skill names accept only ASCII letters, numbers, '-' and '_'");
    }
    let dir = directory(sessions_dir)?;
    let path = dir.join(format!("{name}.md"));
    if name == BUILTIN_NAME
        && path
            .symlink_metadata()
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(include_str!("../skills/symbol-trace.md").into());
    }
    if !path.is_file() || path.symlink_metadata()?.file_type().is_symlink() {
        anyhow::bail!("Skill not found or not a regular file");
    }
    if path.metadata()?.len() > MAX_SKILL_BYTES {
        anyhow::bail!("Skill exceeds 16 KiB");
    }
    fs::read_to_string(&path).with_context(|| format!("Unable to read skill {}", path.display()))
}

const BUILTIN_NAME: &str = "symbol-trace";
pub const MANUAL_PREFIX: &str = "For my next requests, use the following locally installed and explicitly selected skill named '";
pub const SELECT: &str = "select_workspace_skill";
pub const READ_TOOLS: &[&str] = &[
    "list_files",
    "file_info",
    "read_file",
    "read_file_window",
    "read_lines",
    "search_file",
    "search_files",
    "file_outline",
    "code_search",
    "project_map",
];

pub struct Guide {
    pub name: String,
    description: String,
    patterns: Vec<String>,
    pub body: String,
}

pub fn catalog(dir: &Path) -> Result<Vec<Guide>> {
    let mut guides = Vec::new();
    if directory(dir)?
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Ok(guides);
    }
    for name in list(dir)?["skills"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let Ok(text) = load(dir, name) else { continue };
        let text = text.replace("\r\n", "\n");
        let Some((header, body)) = text
            .strip_prefix("---\n")
            .and_then(|s| s.split_once("\n---\n"))
        else {
            continue;
        };
        if !header
            .lines()
            .any(|s| s.trim() == "auto-scope: workspace-read")
        {
            continue;
        }
        let Some(description) = header
            .lines()
            .find_map(|s| s.strip_prefix("description:"))
            .map(str::trim)
        else {
            continue;
        };
        if description.is_empty()
            || description.chars().count() > 240
            || body.trim().is_empty()
            || body.len() > 4096
        {
            continue;
        }
        guides.push(Guide {
            name: name.to_string(),
            description: description.to_string(),
            patterns: header
                .lines()
                .filter_map(|s| s.strip_prefix("match:"))
                .map(str::trim)
                .filter(|s| s.len() <= 256 && !s.is_empty())
                .take(8)
                .map(str::to_owned)
                .collect(),
            body: body.trim().to_owned(),
        });
        if guides.len() == 12 {
            break;
        }
    }
    Ok(guides)
}

fn words(text: &str) -> Vec<String> {
    text.trim()
        .trim_end_matches(['?', '؟', '.', '!'])
        .split_whitespace()
        .map(|s| s.trim_matches('`').to_lowercase())
        .collect()
}

// An exact task template is a high-confidence fast path. Other paraphrases
// use the model's optional catalog selection in the ordinary first request.
pub fn exact<'a>(guides: &'a [Guide], prompt: &str) -> Option<&'a Guide> {
    let request = words(prompt);
    let mut found = guides.iter().filter(|g| {
        g.patterns.iter().any(|pattern| {
            let tokens = words(pattern);
            tokens.len() == request.len()
                && tokens.iter().zip(&request).all(|(a, b)| {
                    if a == "{symbol}" {
                        b.chars()
                            .next()
                            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
                            && b.chars()
                                .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                    } else {
                        a == b
                    }
                })
        })
    });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

pub fn workspace_reads(definitions: &Value) -> Value {
    let names = READ_TOOLS.iter().map(|s| s.to_string()).collect();
    crate::tool_selection::filter(definitions, Some(&names))
}

pub fn selector(guides: &[Guide]) -> Value {
    let entries: Vec<_> = guides
        .iter()
        .map(|g| json!([g.name, g.description]))
        .collect();
    let names: Vec<_> = guides.iter().map(|g| &g.name).collect();
    json!({"type":"function","function":{"name":SELECT,
        "description":format!("Optional. Match the meaning of the latest user request in its language to one installed guide. Select only if the entire task calls for reading existing local workspace files and a single guide clearly fits. For a general explanation, an ambiguous match, or a task requiring edits, tests, commands or a website, proceed normally. Call alone. Available names and descriptions (metadata): {}",json!(entries)),
        "parameters":{"type":"object","properties":{"name":{"type":"string","enum":names}},"required":["name"],"additionalProperties":false}}})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_in_guides_and_overlapping_templates() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions = tmp.path().join("sessions");
        let dir = directory(&sessions).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plain.md"), "match: Find {symbol}").unwrap();
        fs::write(dir.join("other.md"),"---\r\nauto-scope: workspace-read\r\ndescription: Trace local symbols\r\nmatch: Where is {symbol} defined and called\r\n---\r\nUse local evidence.").unwrap();
        let guides = catalog(&sessions).unwrap();
        assert_eq!(guides.len(), 2);
        assert!(exact(&guides, "Where is foo defined and called?").is_none());
        assert!(exact(&guides, "Where is foo defined and called? Then edit it.").is_none());
        assert!(exact(&guides, "شو يعني export *؟").is_none());
        assert_eq!(
            exact(&guides, "وين مكتوبة `foo` ومين بيناديها؟")
                .unwrap()
                .name,
            BUILTIN_NAME
        );
    }

    #[test]
    fn lists_and_reads_only_local_markdown() {
        let home = tempfile::tempdir().unwrap();
        let sessions = home.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let dir = directory(&sessions).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("review.md"), "# Review\nRead the tests.").unwrap();
        fs::write(dir.join("script.sh"), "echo unsafe").unwrap();
        let available = list(&sessions).unwrap();
        assert_eq!(available["skills"][0], "review");
        assert!(load(&sessions, "review")
            .unwrap()
            .contains("Read the tests"));
        assert!(load(&sessions, "../review").is_err());
    }
}
