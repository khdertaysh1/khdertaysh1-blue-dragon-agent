//! Deterministic, bounded project overview. Never follows symlinks or reads source bodies.
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const MAX_FILES: usize = 1200;
const MAX_DEPTH: usize = 8;

fn excluded(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".ssh"
            | ".aws"
            | ".gnupg"
            | "node_modules"
            | "target"
            | "dist"
            | "vendor"
            | ".venv"
            | "__pycache__"
            | "build"
    ) || name.starts_with(".env")
        || name.ends_with(".pem")
        || name.ends_with(".key")
}

fn language(extension: &str) -> Option<&'static str> {
    match extension {
        "rs" => Some("Rust"),
        "py" => Some("Python"),
        "js" | "jsx" | "mjs" | "cjs" => Some("JavaScript"),
        "ts" | "tsx" => Some("TypeScript"),
        "java" => Some("Java"),
        "kt" | "kts" => Some("Kotlin"),
        "go" => Some("Go"),
        "php" => Some("PHP"),
        "c" | "h" => Some("C"),
        "cpp" | "cc" | "hpp" => Some("C++"),
        "cs" => Some("C#"),
        "rb" => Some("Ruby"),
        "swift" => Some("Swift"),
        "html" | "css" | "scss" => Some("Web"),
        "sh" | "bash" | "zsh" | "ps1" => Some("Shell"),
        _ => None,
    }
}

/// A heuristic project overview, NOT an AST or a complete dependency graph.
pub fn inspect(root: &Path) -> Result<Value> {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    let mut total_bytes = 0u64;
    let mut language_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut manifests = Vec::new();
    let mut entries = Vec::new();
    let mut truncated = false;

    while let Some((dir, depth)) = stack.pop() {
        let Ok(listing) = fs::read_dir(&dir) else {
            continue;
        };
        for item in listing.flatten() {
            if visited >= MAX_FILES {
                truncated = true;
                break;
            }
            let name = item.file_name();
            let name = name.to_string_lossy();
            if excluded(&name) {
                continue;
            }
            let Ok(kind) = item.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                if depth < MAX_DEPTH {
                    stack.push((item.path(), depth + 1));
                } else {
                    truncated = true;
                }
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            visited += 1;
            total_bytes = total_bytes.saturating_add(item.metadata().map(|m| m.len()).unwrap_or(0));
            let relative = item
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .to_string();
            if let Some(ext) = item.path().extension().and_then(|x| x.to_str()) {
                if let Some(lang) = language(&ext.to_ascii_lowercase()) {
                    *language_counts.entry(lang.to_string()).or_default() += 1;
                }
            }
            if matches!(
                name.as_ref(),
                "Cargo.toml"
                    | "package.json"
                    | "pyproject.toml"
                    | "requirements.txt"
                    | "go.mod"
                    | "pom.xml"
                    | "build.gradle"
                    | "composer.json"
                    | "Dockerfile"
                    | "Makefile"
            ) && manifests.len() < 30
            {
                manifests.push(relative.clone());
            }
            if entries.len() < 50 {
                entries.push(relative);
            }
        }
    }

    manifests.sort();
    entries.sort();
    Ok(json!({
        "root": root.display().to_string(),
        "files_inspected": visited,
        "size_bytes_estimate": total_bytes,
        "languages": language_counts,
        "manifests": manifests,
        "sample_files": entries,
        "truncated": truncated,
        "note": "Heuristic filesystem overview, not a full semantic code graph"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_languages_and_skips_secrets() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"test\"\n").unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.path().join(".env"), "secret=hidden\n").unwrap();
        let report = inspect(dir.path()).unwrap();
        assert_eq!(report["languages"]["Rust"], 1);
        assert_eq!(report["files_inspected"], 2);
        assert_eq!(report["manifests"][0], "Cargo.toml");
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_directory_symlinks() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        symlink("/etc", dir.path().join("outside")).unwrap();
        let report = inspect(dir.path()).unwrap();
        assert_eq!(report["files_inspected"], 0);
    }
}
