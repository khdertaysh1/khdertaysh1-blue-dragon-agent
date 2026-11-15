//! Bounded child proposals and parent-only, journaled multi-file edits.
use crate::{intelligence::code::hash, tools::ToolBox};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use similar::TextDiff;
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const FILE_BYTES: usize = 4096;
const TOTAL_BYTES: usize = 16384;

pub(crate) struct WriteLock {
    path: PathBuf,
    retain: bool,
}
impl WriteLock {
    pub(crate) fn acquire(root: &Path) -> Result<Self> {
        let path = root.join(".blue-dragon-edits.lock");
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path)
            .context("Workspace edit lock exists or is unavailable; inspect prior outcome, never automatically remove a stale lock")?;
        file.write_all(b"Native edit in progress. If interrupted, reconcile files and journals before manually removing this lock.\n")?;
        file.sync_all()?;
        Ok(Self {
            path,
            retain: false,
        })
    }
}
impl Drop for WriteLock {
    fn drop(&mut self) {
        if !self.retain {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn safe_path(tools: &ToolBox, raw: &str) -> Result<PathBuf> {
    if raw.is_empty()
        || raw.len() > 512
        || raw.chars().any(char::is_control)
        || Path::new(raw)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("Proposal path must be a plain relative file path");
    }
    let full = tools.local_file_path(raw)?;
    let mut part = tools.workspace_root().to_path_buf();
    for component in Path::new(raw).components() {
        part.push(component);
        if fs::symlink_metadata(&part)?.file_type().is_symlink() {
            bail!("Proposal symlinks are unsupported");
        }
    }
    if full != part || !fs::metadata(&full)?.is_file() {
        bail!("Proposal requires an existing regular file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if fs::metadata(&full)?.nlink() != 1 {
            bail!("Proposal hard links are unsupported");
        }
    }
    Ok(full)
}
fn text(path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > FILE_BYTES {
        bail!("Proposal file exceeds 4096 bytes");
    }
    let value = String::from_utf8(bytes)?;
    validate_text(&value)?;
    Ok(value)
}
fn validate_text(value: &str) -> Result<()> {
    if value
        .as_bytes()
        .iter()
        .enumerate()
        .any(|(i, b)| *b == b'\r' && value.as_bytes().get(i + 1) != Some(&b'\n'))
    {
        bail!("Bare carriage return is unsupported; use LF or CRLF line endings");
    }
    if value.len() > FILE_BYTES
        || value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        bail!("Proposal requires bounded UTF-8 text without terminal control characters");
    }
    Ok(())
}
fn files(value: &Value) -> Result<&Vec<Value>> {
    value["files"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 4)
        .context("Provide 1..4 files")
}

pub(crate) fn propose(
    tools: &ToolBox,
    args: &Value,
    allowed: impl Fn(&Path) -> bool,
) -> Result<Value> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    let mut total = 0;
    for file in files(args)? {
        let raw = file["path"].as_str().context("path required")?;
        let full = safe_path(tools, raw)?;
        if !allowed(&full) {
            bail!("Path outside delegated proposal scope");
        }
        if !seen.insert(full.clone()) {
            bail!("Duplicate proposal path");
        }
        let content = file["content"].as_str().context("content required")?;
        validate_text(content)?;
        let old = text(&full)?;
        let before = hash(old.as_bytes());
        if file["expected_sha256"] != before {
            bail!("Proposal base hash conflict");
        }
        if old == content {
            bail!("Proposal contains an unchanged file");
        }
        total += old.len() + content.len();
        if total > TOTAL_BYTES {
            bail!("Proposal exceeds 16KiB combined old/new text");
        }
        result.push(json!({"path":raw,"expected_sha256":before,"content":content,"after_sha256":hash(content.as_bytes())}));
    }
    Ok(
        json!({"ok":true,"kind":"delegate_edit_proposal_v1","files":result,"workspace_writes":0,"requires_parent_approval":true,"semantic_claim_verified":false}),
    )
}

struct Staged {
    path: PathBuf,
    raw: String,
    old: String,
    new: String,
}
fn stage(tools: &ToolBox, proposal: &Value, reverse: bool) -> Result<Vec<Staged>> {
    let mut seen = BTreeSet::new();
    let mut staged = Vec::new();
    let mut total = 0;
    for file in files(proposal)? {
        let raw = file["path"].as_str().context("path required")?;
        let path = safe_path(tools, raw)?;
        if !seen.insert(path.clone()) {
            bail!("Duplicate proposal path");
        }
        let old = text(&path)?;
        let (key, content) = if reverse {
            ("after_sha256", "before_content")
        } else {
            ("expected_sha256", "content")
        };
        if file[key] != hash(old.as_bytes()) {
            bail!("File version conflict: {raw}; nothing written");
        }
        let new = file[content]
            .as_str()
            .context("Missing proposal/rollback content")?
            .to_string();
        validate_text(&new)?;
        let target_key = if reverse {
            "expected_sha256"
        } else {
            "after_sha256"
        };
        if file[target_key] != hash(new.as_bytes()) {
            bail!("Proposal content digest mismatch");
        }
        total += old.len() + new.len();
        if total > TOTAL_BYTES {
            bail!("Proposal exceeds combined text budget");
        }
        staged.push(Staged {
            path,
            raw: raw.into(),
            old,
            new,
        });
    }
    Ok(staged)
}
fn diff(staged: &[Staged]) -> Result<String> {
    let mut value =
        String::from("Untrusted child proposal. Review every file below. No tests are implied.\nDisplay encoding: backslashes are doubled; CR in CRLF is shown as \\r. File bytes are unchanged.\n");
    for file in staged {
        let old_display = file.old.replace('\\', "\\\\").replace('\r', "\\r");
        let new_display = file.new.replace('\\', "\\\\").replace('\r', "\\r");
        value += &TextDiff::from_lines(&old_display, &new_display)
            .unified_diff()
            .context_radius(3)
            .header(&format!("a/{}", file.raw), &format!("b/{}", file.raw))
            .to_string();
    }
    if value.len() > 7000 || value.lines().count() > 120 {
        bail!("Full diff exceeds approval limits; request a smaller proposal");
    }
    Ok(value)
}
fn journal_path(directory: &Path, identity: &str) -> PathBuf {
    directory.join(format!("edit-{}.journal", hash(identity.as_bytes())))
}
fn save(path: &Path, value: &Value) -> Result<()> {
    let mut temp =
        tempfile::NamedTempFile::new_in(path.parent().context("Missing journal parent")?)?;
    temp.write_all(value.to_string().as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
fn load(path: &Path) -> Result<Value> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > 65536 {
        bail!("Invalid edit journal");
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        bail!("Edit journal too large");
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn replace(tools: &ToolBox, file: &Staged, expected: &str, new: &str) -> Result<()> {
    let current = safe_path(tools, &file.raw)?;
    if current != file.path || text(&current)? != expected {
        bail!("File changed during transaction: {}", file.raw);
    }
    let mut temp = tempfile::NamedTempFile::new_in(current.parent().unwrap())?;
    temp.write_all(new.as_bytes())?;
    temp.as_file()
        .set_permissions(fs::metadata(&current)?.permissions())?;
    temp.as_file().sync_all()?;
    if safe_path(tools, &file.raw)? != current || text(&current)? != expected {
        bail!("File changed before replacement");
    }
    temp.persist(current).map_err(|e| e.error)?;
    Ok(())
}

pub(crate) fn parent_action(
    tools: &ToolBox,
    directory: &Path,
    identity: &str,
    proposal: &Value,
    action: &str,
) -> Result<Value> {
    let journal = journal_path(directory, identity);
    if action == "edit_status" {
        let status = if journal.exists() {
            load(&journal)?["state"]
                .as_str()
                .unwrap_or("unknown")
                .to_string()
        } else {
            "not_started".into()
        };
        return Ok(json!({"state":status,"actions_replayed":0,"current_files_verified":false}));
    }
    let reverse = action == "rollback_edits";
    let mut record = if reverse {
        let value = load(&journal)?;
        if value["identity"] != identity
            || value["workspace"] != tools.workspace_root().to_string_lossy().as_ref()
            || value["state"] != "committed"
        {
            bail!("Rollback requires a matching committed journal; uncertain actions require manual reconciliation");
        }
        if files(&value)?.len() != files(proposal)?.len() {
            bail!("Journal proposal file count changed");
        }
        for (saved, pinned) in files(&value)?.iter().zip(files(proposal)?) {
            for key in ["path", "expected_sha256", "content", "after_sha256"] {
                if saved[key] != pinned[key] {
                    bail!("Journal does not match pinned proposal");
                }
            }
        }
        value
    } else {
        if action == "apply_edits" && journal.exists() {
            bail!("Proposal already attempted; no automatic retry or replay");
        }
        json!({"identity":identity,"workspace":tools.workspace_root(),"files":proposal["files"]})
    };
    let staged = stage(tools, &record, reverse)?;
    let detail = diff(&staged)?;
    if action == "review_edits" {
        return Ok(
            json!({"ok":true,"diff":detail,"workspace_writes":0,"current_base_verified":true,"approval_granted":false}),
        );
    }
    if !tools.approve(
        if reverse {
            "Rollback delegated edits"
        } else {
            "Apply delegated edits"
        },
        &detail,
    )? {
        return Ok(json!({"ok":false,"denied":true,"workspace_writes":0}));
    }
    let mut lock = WriteLock::acquire(tools.workspace_root())?;
    // Re-read every file after human approval before any replacement.
    let staged = stage(tools, &record, reverse)?;
    if tools.is_cancelled() {
        return Ok(json!({"ok":false,"cancelled":true,"workspace_writes":0}));
    }
    if !reverse {
        for (file, snapshot) in record["files"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(&staged)
        {
            file["before_content"] = json!(snapshot.old);
        }
        fs::create_dir_all(directory)?;
        // Exclusive intent claim persists before changing any project file.
        let claim = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&journal)?;
        claim.sync_all()?;
    }
    record["state"] = json!("unknown");
    record["operation"] = json!(action);
    save(&journal, &record)?;
    for (index, file) in staged.iter().enumerate() {
        let attempt = if tools.is_cancelled() {
            Err(anyhow::anyhow!("Cancelled during transaction"))
        } else {
            replace(tools, file, &file.old, &file.new)
        };
        if let Err(error) = attempt {
            let mut recovered = true;
            for previous in staged[..index].iter().rev() {
                if replace(tools, previous, &previous.new, &previous.old).is_err() {
                    recovered = false;
                }
            }
            // A failing rename or outside writer can leave the failed/unvisited
            // file uncertain too. Never claim complete rollback from prior files alone.
            recovered &= staged.iter().all(|entry| {
                safe_path(tools, &entry.raw).ok().as_ref() == Some(&entry.path)
                    && text(&entry.path).is_ok_and(|current| current == entry.old)
            });
            record["state"] = json!(if recovered {
                if reverse {
                    "committed"
                } else {
                    "failed_rolled_back"
                }
            } else {
                "unknown"
            });
            if save(&journal, &record).is_err() {
                recovered = false;
                record["state"] = json!("unknown");
            }
            lock.retain = !recovered;
            return Ok(
                json!({"ok":false,"state":record["state"],"execution_status":if recovered {"failed_rolled_back"} else {"unknown"},"error":error.to_string(),"rollback_complete":recovered,"automatic_retry":false}),
            );
        }
        #[cfg(test)]
        AFTER_WRITE.with(|hook| {
            if let Some(callback) = hook.borrow_mut().as_mut() {
                callback(index);
            }
        });
    }
    // Read every result independently before declaring the entire set committed.
    // A concurrent outside writer is not ours to undo.
    if !staged.iter().all(|entry| {
        safe_path(tools, &entry.raw).ok().as_ref() == Some(&entry.path)
            && text(&entry.path).is_ok_and(|current| current == entry.new)
    }) {
        lock.retain = true;
        record["state"] = json!("unknown");
        let _ = save(&journal, &record);
        return Ok(
            json!({"ok":false,"state":"unknown","execution_status":"unknown","current_files_verified":false,"automatic_retry":false,"error":"Final independent readback mismatch; reconcile files manually"}),
        );
    }
    record["state"] = json!(if reverse { "rolled_back" } else { "committed" });
    if let Err(error) = save(&journal, &record) {
        lock.retain = true;
        return Ok(
            json!({"ok":false,"state":"unknown","execution_status":"unknown","error":error.to_string(),"automatic_retry":false}),
        );
    }
    Ok(
        json!({"ok":true,"state":record["state"],"files":staged.len(),"automatic_retry":false,"multi_file_atomic":false,"current_files_verified":true}),
    )
}

pub(crate) fn definition() -> Value {
    json!({"type":"function","function":{"name":"agent_propose_edits","description":"Propose changes only; never write workspace files. Existing granted regular UTF-8 files only, no symlinks/hardlinks, 1..4 files, 4096 bytes each, 16KiB combined old/new. Supply base SHA256 and complete replacement content. Parent must review and explicitly approve the pinned proposal.","parameters":{"type":"object","properties":{"files":{"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","properties":{"path":{"type":"string"},"expected_sha256":{"type":"string"},"content":{"type":"string"}},"required":["path","expected_sha256","content"],"additionalProperties":false}}},"required":["files"],"additionalProperties":false}}})
}

#[cfg(test)]
type WriteHook = Option<Box<dyn FnMut(usize)>>;
#[cfg(test)]
thread_local! { static AFTER_WRITE: std::cell::RefCell<WriteHook> = const { std::cell::RefCell::new(None) }; }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{ApprovalMode, ApprovalRequest};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    fn fixture() -> (tempfile::TempDir, ToolBox, Value) {
        let dir = tempfile::tempdir().unwrap();
        let tools = ToolBox::new(dir.path().canonicalize().unwrap(), ApprovalMode::ReadOnly);
        fs::write(dir.path().join("a.txt"), "old A\n").unwrap();
        fs::write(dir.path().join("b.txt"), "قديم B 🐉\n").unwrap();
        let args = json!({"files":[{"path":"a.txt","expected_sha256":hash(b"old A\n"),"content":"new A\n"},{"path":"b.txt","expected_sha256":hash("قديم B 🐉\n".as_bytes()),"content":"جديد B 🐉\n"}]});
        (dir, tools, args)
    }
    fn approved(root: &Path, before: impl FnOnce() + Send + 'static) -> ToolBox {
        let (tx, rx) = std::sync::mpsc::channel::<ApprovalRequest>();
        std::thread::spawn(move || {
            let request = rx.recv().unwrap();
            assert!(request.detail.contains("a/a.txt"));
            before();
            request.respond_to.send(true).unwrap();
        });
        ToolBox::new(root.to_path_buf(), ApprovalMode::Tui(tx))
    }
    #[test]
    fn proposal_is_read_only_scoped_bounded_and_versioned() {
        let (dir, tools, args) = fixture();
        let proposal = propose(&tools, &args, |_| true).unwrap();
        assert_eq!(proposal["workspace_writes"], 0);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "old A\n"
        );
        assert!(propose(&tools, &args, |p| p.ends_with("a.txt")).is_err());
        let mut bad = args.clone();
        bad["files"][1]["expected_sha256"] = json!("stale");
        assert!(propose(&tools, &bad, |_| true).is_err());
        bad = args.clone();
        bad["files"][1] = bad["files"][0].clone();
        assert!(propose(&tools, &bad, |_| true).is_err());
        bad = args.clone();
        bad["files"][0]["content"] = json!("x".repeat(4097));
        assert!(propose(&tools, &bad, |_| true).is_err());
        bad = args.clone();
        bad["files"][0]["content"] = json!("\u{1b}[2J");
        assert!(propose(&tools, &bad, |_| true).is_err());
        bad = args.clone();
        bad["files"][0]["path"] = json!("../a.txt");
        assert!(propose(&tools, &bad, |_| true).is_err());
        let journal = dir.path().join("team");
        assert_eq!(
            parent_action(&tools, &journal, "p", &proposal, "apply_edits").unwrap()["denied"],
            true
        );
        assert!(!journal.exists());
    }
    #[test]
    fn approval_rejects_bare_cr_and_displays_crlf_and_literal_backslash_distinctly() {
        assert!(validate_text("DANGEROUS_CHANGE\r+SAFE_TEXT").is_err());
        assert!(validate_text("trailing\r").is_err());
        assert!(validate_text("valid\r\nnext\n").is_ok());
        let staged = vec![Staged {
            path: PathBuf::from("a.txt"),
            raw: "a.txt".into(),
            old: "old\r\n".into(),
            new: "new\\r\r\n".into(),
        }];
        let shown = diff(&staged).unwrap();
        assert!(!shown.contains('\r'));
        assert!(shown.contains("-old\\r\n"));
        assert!(shown.contains("+new\\\\r\\r\n"));
        assert_eq!(staged[0].new, "new\\r\r\n");
    }
    #[test]
    fn approved_multi_file_apply_and_explicit_rollback_are_pinned_once() {
        let (dir, tools, args) = fixture();
        let proposal = propose(&tools, &args, |_| true).unwrap();
        let journal = dir.path().join("team");
        assert_eq!(
            parent_action(&tools, &journal, "p", &proposal, "review_edits").unwrap()
                ["workspace_writes"],
            0
        );
        let tools = approved(tools.workspace_root(), || {});
        assert_eq!(
            parent_action(&tools, &journal, "p", &proposal, "apply_edits").unwrap()["state"],
            "committed"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("b.txt")).unwrap(),
            "جديد B 🐉\n"
        );
        assert!(parent_action(&tools, &journal, "p", &proposal, "apply_edits").is_err());
        let tools = approved(tools.workspace_root(), || {});
        assert_eq!(
            parent_action(&tools, &journal, "p", &proposal, "rollback_edits").unwrap()["state"],
            "rolled_back"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "old A\n"
        );
        assert!(parent_action(&tools, &journal, "p", &proposal, "apply_edits").is_err());
        assert!(parent_action(&tools, &journal, "p", &proposal, "rollback_edits").is_err());
    }
    #[test]
    fn approval_time_conflict_changes_no_other_file() {
        let (dir, tools, args) = fixture();
        let proposal = propose(&tools, &args, |_| true).unwrap();
        let path = dir.path().join("b.txt");
        let tools = approved(tools.workspace_root(), move || {
            fs::write(path, "outside change").unwrap()
        });
        assert!(parent_action(
            &tools,
            &dir.path().join("team"),
            "p",
            &proposal,
            "apply_edits"
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "old A\n"
        );
        assert!(!dir.path().join(".blue-dragon-edits.lock").exists());
    }
    #[test]
    fn cancellation_after_first_write_rolls_back_without_retry() {
        let (dir, tools, args) = fixture();
        let proposal = propose(&tools, &args, |_| true).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        AFTER_WRITE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |_| signal.store(true, Ordering::SeqCst)))
        });
        let mut tools = approved(tools.workspace_root(), || {});
        tools.set_cancellation(cancel);
        let result = parent_action(
            &tools,
            &dir.path().join("team"),
            "p",
            &proposal,
            "apply_edits",
        )
        .unwrap();
        AFTER_WRITE.with(|hook| *hook.borrow_mut() = None);
        assert_eq!(result["state"], "failed_rolled_back");
        assert_eq!(result["rollback_complete"], true);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "old A\n"
        );
        assert!(parent_action(
            &tools,
            &dir.path().join("team"),
            "p",
            &proposal,
            "apply_edits"
        )
        .is_err());
    }
    #[test]
    fn outside_write_during_transaction_is_preserved_and_reported_unknown() {
        let (dir, tools, args) = fixture();
        let proposal = propose(&tools, &args, |_| true).unwrap();
        let path = dir.path().join("b.txt");
        AFTER_WRITE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |_| fs::write(&path, "outside").unwrap()))
        });
        let tools = approved(tools.workspace_root(), || {});
        let result = parent_action(
            &tools,
            &dir.path().join("team"),
            "p",
            &proposal,
            "apply_edits",
        )
        .unwrap();
        AFTER_WRITE.with(|hook| *hook.borrow_mut() = None);
        assert_eq!(result["state"], "unknown");
        assert_eq!(result["rollback_complete"], false);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "old A\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("b.txt")).unwrap(),
            "outside"
        );
        assert!(dir.path().join(".blue-dragon-edits.lock").exists());
    }
    #[test]
    fn final_readback_detects_change_after_last_write_without_overwriting_it() {
        let (dir, tools, args) = fixture();
        let proposal = propose(&tools, &args, |_| true).unwrap();
        let path = dir.path().join("a.txt");
        AFTER_WRITE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |index| {
                if index == 1 {
                    fs::write(&path, "user edit after last replacement").unwrap();
                }
            }))
        });
        let tools = approved(tools.workspace_root(), || {});
        let result = parent_action(
            &tools,
            &dir.path().join("team"),
            "p",
            &proposal,
            "apply_edits",
        )
        .unwrap();
        AFTER_WRITE.with(|hook| *hook.borrow_mut() = None);
        assert_eq!(result["state"], "unknown");
        assert_eq!(result["current_files_verified"], false);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "user edit after last replacement"
        );
        assert!(dir.path().join(".blue-dragon-edits.lock").exists());
        assert!(parent_action(
            &tools,
            &dir.path().join("team"),
            "p",
            &proposal,
            "apply_edits"
        )
        .is_err());
    }
    #[test]
    fn rollback_refuses_drift_and_journal_substitution() {
        let (dir, tools, args) = fixture();
        let proposal = propose(&tools, &args, |_| true).unwrap();
        let journal = dir.path().join("team");
        let tools = approved(tools.workspace_root(), || {});
        parent_action(&tools, &journal, "p", &proposal, "apply_edits").unwrap();
        let path = journal_path(&journal, "p");
        let mut saved = load(&path).unwrap();
        saved["files"][0]["path"] = json!("b.txt");
        save(&path, &saved).unwrap();
        assert!(
            parent_action(&tools, &journal, "p", &proposal, "rollback_edits")
                .unwrap_err()
                .to_string()
                .contains("pinned proposal")
        );
        saved["files"][0]["path"] = json!("a.txt");
        save(&path, &saved).unwrap();
        fs::write(dir.path().join("b.txt"), "outside").unwrap();
        assert!(parent_action(&tools, &journal, "p", &proposal, "rollback_edits").is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "new A\n"
        );
    }
    #[test]
    fn stale_lock_blocks_native_file_write() {
        let (dir, tools, _) = fixture();
        let _lock = WriteLock::acquire(tools.workspace_root()).unwrap();
        let tools = approved(tools.workspace_root(), || {});
        assert!(tools.write_file("a.txt", "replacement").is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "old A\n"
        );
    }
    #[test]
    #[cfg(unix)]
    fn symlinks_and_hard_links_are_rejected() {
        let (dir, tools, mut args) = fixture();
        std::os::unix::fs::symlink("a.txt", dir.path().join("alias")).unwrap();
        args["files"][0]["path"] = json!("alias");
        assert!(propose(&tools, &args, |_| true).is_err());
        fs::hard_link(dir.path().join("a.txt"), dir.path().join("hard")).unwrap();
        args["files"][0]["path"] = json!("a.txt");
        assert!(propose(&tools, &args, |_| true).is_err());
    }
    #[test]
    fn process_death_helper() {
        let Ok(root) = std::env::var("BLUE_PROPOSAL_CRASH_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let tools = approved(&root, || {});
        let args = json!({"files":[{"path":"a.txt","expected_sha256":hash(b"old A\n"),"content":"new A\n"},{"path":"b.txt","expected_sha256":hash("قديم B 🐉\n".as_bytes()),"content":"new B\n"}]});
        let proposal = propose(&tools, &args, |_| true).unwrap();
        AFTER_WRITE.with(|hook| *hook.borrow_mut() = Some(Box::new(|_| std::process::exit(73))));
        parent_action(
            &tools,
            &root.join("team"),
            "crash",
            &proposal,
            "apply_edits",
        )
        .unwrap();
        panic!("Crash hook not reached");
    }
    #[test]
    fn abrupt_process_exit_leaves_unknown_journal_and_never_replays() {
        let (dir, tools, args) = fixture();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "edit_proposals::tests::process_death_helper",
                "--nocapture",
            ])
            .env("BLUE_PROPOSAL_CRASH_TEST_ROOT", tools.workspace_root())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(73));
        let journal = dir.path().join("team");
        let saved = load(&journal_path(&journal, "crash")).unwrap();
        assert_eq!(saved["state"], "unknown");
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "new A\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("b.txt")).unwrap(),
            "قديم B 🐉\n"
        );
        assert!(dir.path().join(".blue-dragon-edits.lock").exists());
        assert!(parent_action(&tools, &journal, "crash", &args, "apply_edits").is_err());
        assert!(parent_action(&tools, &journal, "crash", &args, "rollback_edits").is_err());
        assert_eq!(
            parent_action(&tools, &journal, "crash", &args, "edit_status").unwrap()
                ["actions_replayed"],
            0
        );
    }
}
