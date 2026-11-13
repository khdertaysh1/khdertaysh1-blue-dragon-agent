use anyhow::{Context, Result};
use serde_json::{json, Value};
use similar::TextDiff;
use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub enum ApprovalMode {
    Ask,
    ReadOnly,
}

pub struct ToolBox {
    root: PathBuf,
    approval: ApprovalMode,
}

const MAX_TEXT_BYTES: usize = 128 * 1024;
const MAX_OUTPUT_BYTES: usize = 40 * 1024;

impl ToolBox {
    pub fn new(root: PathBuf, approval: ApprovalMode) -> Self {
        Self { root, approval }
    }

    fn denied_name(name: &OsStr) -> bool {
        let n = name.to_string_lossy().to_ascii_lowercase();
        n == ".git"
            || n == ".ssh"
            || n == ".aws"
            || n == ".gnupg"
            || n == "node_modules"
            || n == "target"
            || n == "__pycache__"
            || n == ".venv"
            || n == "vendor"
            || n == "dist"
            || n.starts_with(".env")
            || n == "id_rsa"
            || n == "id_ed25519"
            || n.ends_with(".pem")
            || n.ends_with(".key")
            || n == "credentials"
    }

    fn validated_parts(path: &str) -> Result<PathBuf> {
        let rel = Path::new(path);
        if rel.is_absolute() {
            anyhow::bail!("Absolute paths are not allowed");
        }
        let mut out = PathBuf::new();
        for comp in rel.components() {
            match comp {
                Component::Normal(segment) => {
                    if Self::denied_name(segment) {
                        anyhow::bail!("Protected or excluded path component");
                    }
                    out.push(segment);
                }
                Component::CurDir => {}
                _ => anyhow::bail!("Unsafe path component"),
            }
        }
        Ok(out)
    }

    fn resolve(&self, path: &str, writing: bool) -> Result<PathBuf> {
        let rel = Self::validated_parts(path)?;
        let wanted = self.root.join(rel);
        if writing && !wanted.exists() {
            let parent = wanted
                .parent()
                .context("Missing parent directory")?
                .canonicalize()
                .context("Create the parent directory in the project before writing the file")?;
            if !parent.starts_with(&self.root) {
                anyhow::bail!("Path escapes workspace");
            }
            Ok(wanted)
        } else {
            let real = wanted
                .canonicalize()
                .with_context(|| format!("Cannot access {}", path))?;
            if !real.starts_with(&self.root) {
                anyhow::bail!("Path escapes workspace via symlink");
            }
            Ok(real)
        }
    }

    pub fn list_files(&self, raw: &str) -> Result<Value> {
        let base = self.resolve(raw, false)?;
        if !base.is_dir() {
            anyhow::bail!("Not a directory");
        }
        let mut output = Vec::new();
        let mut stack = vec![base];
        while let Some(dir) = stack.pop() {
            if output.len() >= 300 {
                break;
            }
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for ent in entries.flatten() {
                if output.len() >= 300 {
                    break;
                }
                let path = ent.path();
                if path.file_name().is_some_and(Self::denied_name) {
                    continue;
                }
                let kind = match ent.file_type() {
                    Ok(kind) => kind,
                    Err(_) => continue,
                };
                if kind.is_symlink() {
                    continue;
                }
                if let Ok(rel) = path.strip_prefix(&self.root) {
                    if kind.is_dir() {
                        stack.push(path.clone());
                    }
                    output.push(format!(
                        "{}{}",
                        rel.display(),
                        if kind.is_dir() { "/" } else { "" }
                    ));
                }
            }
        }
        output.sort();
        let truncated = output.len() >= 300;
        Ok(json!({"files": output, "truncated": truncated}))
    }

    pub fn read_file(&self, path: &str) -> Result<Value> {
        let path = self.resolve(path, false)?;
        if !path.is_file() {
            anyhow::bail!("Not a file");
        }
        if path.metadata()?.len() > MAX_TEXT_BYTES as u64 {
            anyhow::bail!("File exceeds 128 KiB limit");
        }
        let value = fs::read_to_string(&path).context("File is not UTF-8 text")?;
        Ok(json!({"path": path.strip_prefix(&self.root)?.display().to_string(), "content": value}))
    }

    pub fn search_files(&self, needle: &str) -> Result<Value> {
        if needle.is_empty() || needle.len() > 128 {
            anyhow::bail!("Search must be 1..128 characters");
        }
        let listing = self.list_files(".")?;
        let mut hits = Vec::new();
        for name in listing["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if name.ends_with('/') {
                continue;
            }
            let full = match self.resolve(name, false) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if full.metadata().map(|x| x.len()).unwrap_or(u64::MAX) > MAX_TEXT_BYTES as u64 {
                continue;
            }
            if let Ok(body) = fs::read_to_string(full) {
                for (line_no, line) in body.lines().enumerate() {
                    if line.contains(needle) {
                        hits.push(json!({"path":name,"line":line_no+1,"preview":line.chars().take(240).collect::<String>()}));
                        if hits.len() >= 80 {
                            break;
                        }
                    }
                }
            }
            if hits.len() >= 80 {
                break;
            }
        }
        let truncated = hits.len() >= 80;
        Ok(json!({"matches": hits, "truncated": truncated}))
    }

    pub fn git_diff(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["-C", &self.root.to_string_lossy(), "diff", "--", "."])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .context("Git not available")?;
        if !output.status.success() {
            return Ok("Not a Git working tree, or git diff failed.".to_string());
        }
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text.chars().take(MAX_OUTPUT_BYTES).collect())
    }

    fn approve(&self, action: &str, detail: &str) -> Result<bool> {
        if matches!(self.approval, ApprovalMode::ReadOnly) {
            return Ok(false);
        }
        println!("\n┌─ APPROVAL REQUIRED ──────────────────────────────────────");
        println!("│ {action}");
        for line in detail.lines().take(140) {
            println!("│ {line}");
        }
        if detail.lines().count() > 140 {
            println!("│ ... diff truncated; use /diff to review staged edits");
        }
        println!("└─────────────────────────────────────────────────────────");
        print!("Approve this ONE action? [y/N] ");
        std::io::stdout().flush()?;
        let mut answer = String::new();
        if std::io::stdin().read_line(&mut answer)? == 0 {
            return Ok(false);
        }
        Ok(answer.trim().eq_ignore_ascii_case("y") || answer.trim().eq_ignore_ascii_case("yes"))
    }

    pub fn write_file(&self, relative: &str, new_content: &str) -> Result<Value> {
        if new_content.len() > MAX_TEXT_BYTES {
            anyhow::bail!("Write exceeds 128 KiB limit");
        }
        let path = self.resolve(relative, true)?;
        if path.is_dir() {
            anyhow::bail!("Cannot overwrite directory");
        }
        let old_content = if path.exists() {
            fs::read_to_string(&path).context("Cannot overwrite binary/non-text file")?
        } else {
            String::new()
        };
        if old_content.len() > MAX_TEXT_BYTES {
            anyhow::bail!("Existing file exceeds 128 KiB limit");
        }
        let diff = TextDiff::from_lines(old_content.as_str(), new_content)
            .unified_diff()
            .context_radius(3)
            .header(&format!("a/{relative}"), &format!("b/{relative}"))
            .to_string();
        if old_content == new_content {
            return Ok(json!({"ok":true,"status":"unchanged"}));
        }
        if !self.approve(&format!("Write {relative}"), &diff)? {
            return Ok(json!({"ok":false,"status":"denied by user"}));
        }
        let parent = path.parent().context("Missing file parent")?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(new_content.as_bytes())?;
        temp.as_file_mut().sync_all()?;
        // Recheck the parent before replacing; no untrusted filesystem mutations during approval.
        let safe_parent = parent.canonicalize()?;
        if !safe_parent.starts_with(&self.root) {
            anyhow::bail!("Workspace escape detected");
        }
        temp.persist(&path).map_err(|error| error.error)?;
        Ok(json!({"ok":true,"status":"written","path":relative,"bytes":new_content.len()}))
    }

    pub fn run_command(&self, program: &str, args: &[String]) -> Result<Value> {
        if program.is_empty()
            || program.len() > 200
            || args.len() > 30
            || args.iter().any(|x| x.len() > 1000)
        {
            anyhow::bail!("Command exceeds limits");
        }
        let command_preview = format!("{} {}", program, args.join(" "));
        if !self.approve("Execute workspace command", &command_preview)? {
            return Ok(json!({"ok":false,"status":"denied by user"}));
        }
        let mut out = tempfile::tempfile()?;
        let mut err = tempfile::tempfile()?;
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out.try_clone()?))
            .stderr(Stdio::from(err.try_clone()?))
            .env_clear();
        for var in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "SYSTEMROOT",
            "WINDIR",
            "TEMP",
            "TMP",
            "TMPDIR",
            "CARGO_HOME",
            "RUSTUP_HOME",
            "LANG",
            "LC_ALL",
            "VIRTUAL_ENV",
        ] {
            if let Some(value) = std::env::var_os(var) {
                command.env(var, value);
            }
        }
        let mut child = command.spawn().context("Could not start command")?;
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().ok();
                child.wait().ok();
                anyhow::bail!("Command timed out after 30 seconds");
            }
            thread::sleep(Duration::from_millis(40));
        };
        out.seek(SeekFrom::Start(0))?;
        err.seek(SeekFrom::Start(0))?;
        let mut stdout = String::new();
        let mut stderr = String::new();
        out.take(MAX_OUTPUT_BYTES as u64)
            .read_to_string(&mut stdout)
            .ok();
        err.take(MAX_OUTPUT_BYTES as u64)
            .read_to_string(&mut stderr)
            .ok();
        Ok(
            json!({"ok":status.success(), "exit_code":status.code(), "stdout":stdout,"stderr":stderr}),
        )
    }
}

pub fn definitions() -> Value {
    json!([
        {"type":"function","function":{"name":"list_files","description":"List project files under a workspace-relative directory. No hidden secrets.","parameters":{"type":"object","properties":{"path":{"type":"string","description":"Directory relative to workspace; use . for root"}},"required":["path"],"additionalProperties":false}}},
        {"type":"function","function":{"name":"read_file","description":"Read one UTF-8 project file, up to 128KiB, excluding secret paths.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}}},
        {"type":"function","function":{"name":"search_files","description":"Find exact string occurrences across project text files.","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}}},
        {"type":"function","function":{"name":"git_diff","description":"Show unstaged Git diff. Read-only.","parameters":{"type":"object","properties":{},"additionalProperties":false}}},
        {"type":"function","function":{"name":"write_file","description":"Replace or create a UTF-8 file relative to workspace. User approval is mandatory and a diff is shown.","parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false}}},
        {"type":"function","function":{"name":"run_command","description":"Execute a program with arguments in workspace (no shell expansion); requires user approval. 30-second timeout.","parameters":{"type":"object","properties":{"program":{"type":"string"},"args":{"type":"array","items":{"type":"string"}}},"required":["program","args"],"additionalProperties":false}}}
    ])
}

pub fn invoke(boxes: &ToolBox, name: &str, args: &Value) -> Value {
    let arg = |key: &str| -> Result<&str> {
        args.get(key)
            .and_then(Value::as_str)
            .context(format!("Missing or invalid {key}"))
    };
    let result = match name {
        "list_files" => arg("path").and_then(|x| boxes.list_files(x)),
        "read_file" => arg("path").and_then(|x| boxes.read_file(x)),
        "search_files" => arg("query").and_then(|x| boxes.search_files(x)),
        "git_diff" => boxes.git_diff().map(|x| json!({"diff":x})),
        "write_file" => {
            arg("path").and_then(|p| arg("content").and_then(|c| boxes.write_file(p, c)))
        }
        "run_command" => (|| -> Result<Value> {
            let program = arg("program")?;
            let arr = args
                .get("args")
                .and_then(Value::as_array)
                .context("Missing args array")?;
            let values: Result<Vec<String>> = arr
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .context("Invalid command argument")
                })
                .collect();
            boxes.run_command(program, &values?)
        })(),
        _ => Err(anyhow::anyhow!("Unknown tool: {name}")),
    };
    match result {
        Ok(value) => value,
        Err(error) => json!({"ok":false,"error":format!("{error:#}")}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, ToolBox) {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("hello.rs"),
            "fn main() { println!(\"hello\"); }\n",
        )
        .unwrap();
        let tools = ToolBox::new(tmp.path().canonicalize().unwrap(), ApprovalMode::ReadOnly);
        (tmp, tools)
    }
    #[test]
    fn reads_and_searches_real_files() {
        let (_tmp, t) = setup();
        assert!(t.read_file("hello.rs").unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("hello"));
        assert_eq!(
            t.search_files("println").unwrap()["matches"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn blocks_path_traversal_and_secret_files() {
        let (_tmp, t) = setup();
        assert!(t.read_file("../hello.rs").is_err());
        assert!(t.read_file(".env").is_err());
        assert!(t.read_file("/etc/passwd").is_err());
    }
    #[test]
    fn denies_writes_and_commands_without_approval() {
        let (tmp, t) = setup();
        assert_eq!(t.write_file("hello.rs", "changed").unwrap()["ok"], false);
        assert_eq!(
            fs::read_to_string(tmp.path().join("hello.rs")).unwrap(),
            "fn main() { println!(\"hello\"); }\n"
        );
        assert_eq!(
            t.run_command("echo", &["hello".to_string()]).unwrap()["ok"],
            false
        );
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let (tmp, t) = setup();
        symlink("/etc/passwd", tmp.path().join("leak.txt")).unwrap();
        assert!(t.read_file("leak.txt").is_err());
    }
}
