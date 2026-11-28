//! Disposable detached Git worktrees for an explicitly approved local coding run.
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) struct Tree {
    repo: PathBuf,
    _temporary: tempfile::TempDir,
    pub root: PathBuf,
}

fn git(root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(["--no-optional-locks", "-c", "core.hooksPath=/dev/null"])
        .stdin(Stdio::null());
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}

pub(crate) fn revision(root: &Path) -> Result<String> {
    let prefix = git(root).args(["rev-parse", "--show-prefix"]).output()?;
    ensure!(
        prefix.status.success() && prefix.stdout.iter().all(u8::is_ascii_whitespace),
        "Repair requires a Git workspace opened at its repository root"
    );
    let head = git(root)
        .args(["rev-parse", "--verify", "HEAD^{commit}"])
        .output()?;
    let hash = std::str::from_utf8(&head.stdout)?.trim();
    ensure!(
        head.status.success()
            && matches!(hash.len(), 40 | 64)
            && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "Missing Git commit for isolated repair"
    );
    Ok(hash.into())
}

impl Tree {
    pub fn new(repo: &Path, commit: &str) -> Result<Self> {
        let temporary = tempfile::Builder::new().prefix("blue-swarm-").tempdir()?;
        let root = temporary.path().canonicalize()?.join("checkout");
        let run = git(repo)
            .args(["worktree", "add", "--quiet", "--detach"])
            .arg(&root)
            .arg(commit)
            .output()
            .context("Start git worktree add")?;
        ensure!(
            run.status.success(),
            "Could not create isolated worktree: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        Ok(Self {
            repo: repo.into(),
            _temporary: temporary,
            root,
        })
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        // Remove only the exact disposable checkout allocated by this instance.
        let _ = git(&self.repo)
            .args(["worktree", "remove", "--force"])
            .arg(&self.root)
            .output();
    }
}
