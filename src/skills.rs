//! Explicitly activated local Markdown skills. Never runs bundled scripts.
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
    let home = sessions_dir.parent().context("Missing Blue Dragon data directory")?;
    Ok(home.join("skills"))
}

pub fn list(sessions_dir: &Path) -> Result<Value> {
    let dir = directory(sessions_dir)?;
    if !dir.exists() {
        return Ok(json!({"skills": [], "directory": dir.display().to_string()}));
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
    names.sort();
    names.truncate(100);
    Ok(json!({"skills": names, "directory": dir.display().to_string()}))
}

pub fn load(sessions_dir: &Path, name: &str) -> Result<String> {
    if !valid_name(name) {
        anyhow::bail!("Skill names accept only ASCII letters, numbers, '-' and '_'");
    }
    let dir = directory(sessions_dir)?;
    let path = dir.join(format!("{name}.md"));
    if !path.is_file() || path.symlink_metadata()?.file_type().is_symlink() {
        anyhow::bail!("Skill not found or not a regular file");
    }
    if path.metadata()?.len() > MAX_SKILL_BYTES {
        anyhow::bail!("Skill exceeds 16 KiB");
    }
    fs::read_to_string(&path).with_context(|| format!("Unable to read skill {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(load(&sessions, "review").unwrap().contains("Read the tests"));
        assert!(load(&sessions, "../review").is_err());
    }
}
