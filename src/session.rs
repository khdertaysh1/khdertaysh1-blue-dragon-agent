use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub workspace: String,
    pub model: String,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<Value>,
    #[serde(default)]
    pub plan: crate::plan::Plan,
    #[serde(default)]
    pub last_report: Value,
    #[serde(default)]
    pub observations: Vec<crate::reasoning::Observation>,
    #[serde(default)]
    pub findings: Vec<crate::reasoning::Finding>,
    #[serde(default)]
    pub task_checks: crate::intelligence::checks::Contract,
    #[serde(default)]
    pub memory_scope: Option<String>,
    #[serde(default)]
    pub vision_destination: Option<String>,
    #[serde(default)]
    pub scope: crate::scope::Policy,
    #[serde(default)]
    pub audit: crate::audit::Ledger,
    #[serde(default)]
    pub test_coverage: crate::test_coverage::Ledger,
}

#[derive(Debug, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub workspace: String,
    pub updated_at: DateTime<Utc>,
}

pub fn new(root: &Path, model: &str) -> Session {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = format!(
        "{}-{}-{}",
        Utc::now().format("%Y%m%dT%H%M%S"),
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    Session {
        id,
        workspace: root.to_string_lossy().to_string(),
        model: model.to_string(),
        updated_at: Utc::now(),
        messages: Vec::new(),
        plan: crate::plan::Plan::default(),
        last_report: Value::Null,
        observations: Vec::new(),
        findings: Vec::new(),
        task_checks: crate::intelligence::checks::Contract::default(),
        memory_scope: None,
        vision_destination: None,
        scope: crate::scope::Policy::default(),
        audit: crate::audit::Ledger::default(),
        test_coverage: crate::test_coverage::Ledger::default(),
    }
}

/// Complete only missing tool-result envelopes after an interrupted run.
/// Execution is UNKNOWN, never replayed or reported as successful.
pub fn recover_tool_history(session: &mut Session) -> usize {
    let mut fixed = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut count = 0;
    let old_len = session.messages.len();
    let mut positions = Vec::with_capacity(old_len + 1);
    let flush = |fixed: &mut Vec<Value>, pending: &mut Vec<String>, count: &mut usize| {
        for id in pending.drain(..) {
            fixed.push(serde_json::json!({"role":"tool","tool_call_id":id,"content":serde_json::json!({"ok":false,"execution_status":"unknown","error":"Previous run ended before this result was saved. Inspect the workspace before proposing a retry."}).to_string()}));
            *count += 1;
        }
    };
    for message in std::mem::take(&mut session.messages) {
        if message["role"] != "tool" {
            flush(&mut fixed, &mut pending, &mut count);
        }
        if message["role"] == "assistant" {
            if let Some(calls) = message["tool_calls"].as_array() {
                pending = calls
                    .iter()
                    .filter_map(|c| c["id"].as_str().map(str::to_owned))
                    .collect();
            }
        } else if message["role"] == "tool" {
            if let Some(id) = message["tool_call_id"].as_str() {
                pending.retain(|x| x != id);
            }
        }
        positions.push(fixed.len());
        fixed.push(message);
    }
    flush(&mut fixed, &mut pending, &mut count);
    positions.push(fixed.len());
    // Evidence IDs are stable citations; only transcript positions move when
    // missing envelopes are inserted before later saved messages.
    for observation in &mut session.observations {
        observation.message_index = positions
            .get(observation.message_index)
            .filter(|_| observation.message_index < old_len)
            .copied()
            .unwrap_or(usize::MAX);
    }
    session.task_checks.declared_at = positions
        .get(session.task_checks.declared_at)
        .copied()
        .unwrap_or(usize::MAX);
    for proof in &mut session.task_checks.observations {
        if let Some(saved) = proof {
            if let Some(position) = positions
                .get(saved.message_index)
                .filter(|_| saved.message_index < old_len)
            {
                saved.message_index = *position;
            } else {
                *proof = None;
            }
        }
    }
    session.messages = fixed;
    count
}

const MAX_SESSION_BYTES: u64 = 16_000_000;

fn validate_id(id: &str) -> Result<()> {
    if id == "latest"
        || id.is_empty()
        || id.len() > 200
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        anyhow::bail!("Invalid session ID");
    }
    Ok(())
}

/// Hold this handle for the lifetime of an active agent. OS locks are released
/// on process exit, so a crash does not require deleting a stale PID file.
pub fn lock(sessions_dir: &Path, id: &str) -> Result<fs::File> {
    validate_id(id)?;
    let path = sessions_dir.join(format!("{id}.lock"));
    if fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
        anyhow::bail!("Unsafe session lock file");
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    file.try_lock().map_err(|error| {
        anyhow::anyhow!("Session is already in use or cannot be locked: {error}")
    })?;
    Ok(file)
}

fn read_session(path: &Path) -> Result<Vec<u8>> {
    read_session_bounded(path, MAX_SESSION_BYTES)
}

fn read_session_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        anyhow::bail!("Session must be a regular non-symlink file");
    }
    if meta.len() > limit {
        anyhow::bail!("Session exceeds byte limit");
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        anyhow::bail!("Session exceeds byte limit");
    }
    Ok(bytes)
}

// Stream into the temporary file with a hard cap, avoiding another full-sized
// copy of the transcript on memory-constrained devices.
struct BoundedWriter<W> {
    inner: W,
    remaining: u64,
}
impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other(
                "Session exceeds 16 MB limit; previous checkpoint preserved",
            ));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub fn save(sessions_dir: &Path, session: &mut Session) -> Result<()> {
    validate_id(&session.id)?;
    let output = sessions_dir.join(format!("{}.json", session.id));
    let mut file = tempfile::NamedTempFile::new_in(sessions_dir)?;
    let previous_time = session.updated_at;
    session.updated_at = Utc::now();
    let result = (|| -> Result<()> {
        let mut writer = BoundedWriter {
            inner: std::io::BufWriter::with_capacity(64 * 1024, file.as_file_mut()),
            remaining: MAX_SESSION_BYTES,
        };
        serde_json::to_writer_pretty(&mut writer, session)?;
        writer.flush()?;
        drop(writer);
        file.as_file().sync_all()?;
        file.persist(&output)
            .map_err(|error| error.error)
            .with_context(|| format!("Could not persist session {}", session.id))?;
        Ok(())
    })();
    if result.is_err() {
        session.updated_at = previous_time;
    }
    result
}

pub fn load(sessions_dir: &Path, id: &str, root: &Path) -> Result<Session> {
    if id == "latest" {
        for summary in list_sessions(sessions_dir)?
            .iter()
            .filter(|s| s.workspace == root.to_string_lossy())
        {
            if let Ok(session) = load(sessions_dir, &summary.id, root) {
                return Ok(session);
            }
        }
        anyhow::bail!("No resumable sessions for this workspace");
    }
    load_bounded(sessions_dir, id, root, MAX_SESSION_BYTES)
}

pub(crate) fn load_bounded(
    sessions_dir: &Path,
    id: &str,
    root: &Path,
    limit: u64,
) -> Result<Session> {
    validate_id(id)?;
    let path = sessions_dir.join(format!("{id}.json"));
    let session: Session =
        serde_json::from_slice(&read_session_bounded(&path, limit.min(MAX_SESSION_BYTES))?)?;
    if session.id != id {
        anyhow::bail!("Session ID does not match its filename; refusing to resume");
    }
    if session.workspace != root.to_string_lossy() {
        anyhow::bail!("Session belongs to another workspace; refusing to resume");
    }
    if session.task_checks.checks.len() != session.task_checks.observations.len() {
        anyhow::bail!("Session acceptance-check metadata is inconsistent");
    }
    Ok(session)
}

pub fn list_sessions(sessions_dir: &Path) -> Result<Vec<SessionSummary>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(sessions_dir)? {
        let path: PathBuf = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Ok(bytes) = read_session(&path) {
            // Deserialize only metadata; transcript values are skipped instead
            // of allocating every message just to display a session picker.
            if let Ok(session) = serde_json::from_slice::<SessionSummary>(&bytes) {
                if validate_id(&session.id).is_ok()
                    && path.file_stem().and_then(|s| s.to_str()) == Some(session.id.as_str())
                {
                    out.push(session);
                }
            }
        }
    }
    out.sort_by_key(|item| std::cmp::Reverse(item.updated_at));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persists_and_resumes() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = new(dir.path(), "test-model");
        s.messages
            .push(serde_json::json!({"role":"user","content":"hello"}));
        save(dir.path(), &mut s).unwrap();
        assert_eq!(
            load(dir.path(), &s.id, dir.path()).unwrap().messages.len(),
            1
        );
        assert_eq!(list_sessions(dir.path()).unwrap().len(), 1);
    }
    #[test]
    fn prevents_session_id_traversal() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path(), "../../sensitive", dir.path()).is_err());
    }
    #[test]
    fn latest_session_is_scoped_to_workspace_and_ids_are_unique() {
        let home = tempfile::tempdir().unwrap();
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let mut first = new(a.path(), "a");
        save(home.path(), &mut first).unwrap();
        let mut second = new(b.path(), "b");
        save(home.path(), &mut second).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(load(home.path(), "latest", a.path()).unwrap().id, first.id);
    }
    #[test]
    fn only_one_live_writer_per_session_and_crash_style_drop_releases_lock() {
        let dir = tempfile::tempdir().unwrap();
        let first = lock(dir.path(), "session1").unwrap();
        assert!(lock(dir.path(), "session1").is_err());
        assert!(lock(dir.path(), "session2").is_ok());
        drop(first);
        assert!(lock(dir.path(), "session1").is_ok());
        assert!(lock(dir.path(), "../outside").is_err());
        assert!(lock(dir.path(), "latest").is_err());
    }
    #[test]
    fn latest_skips_corrupt_checkpoint_without_discarding_older_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = new(dir.path(), "fixture");
        save(dir.path(), &mut session).unwrap();
        let corrupt = serde_json::json!({"id":"broken", "workspace":session.workspace, "updated_at":Utc::now()});
        fs::write(dir.path().join("broken.json"), corrupt.to_string()).unwrap();
        assert_eq!(
            load(dir.path(), "latest", dir.path()).unwrap().id,
            session.id
        );
    }
    #[test]
    fn rejects_mismatched_and_unsafe_stored_identity() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = new(dir.path(), "fixture");
        let id = s.id.clone();
        save(dir.path(), &mut s).unwrap();
        s.id = "../outside".into();
        assert!(save(dir.path(), &mut s).is_err());
        fs::write(
            dir.path().join(format!("{id}.json")),
            serde_json::to_vec(&s).unwrap(),
        )
        .unwrap();
        assert!(load(dir.path(), &id, dir.path()).is_err());
        assert!(list_sessions(dir.path()).unwrap().is_empty());
        assert!(validate_id("").is_err());
    }
    #[test]
    fn oversized_checkpoint_preserves_previous_resumable_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = new(dir.path(), "fixture");
        save(dir.path(), &mut s).unwrap();
        let previous = fs::read(dir.path().join(format!("{}.json", s.id))).unwrap();
        let timestamp = s.updated_at;
        s.messages.push(
            serde_json::json!({"role":"tool","content":"x".repeat(MAX_SESSION_BYTES as usize)}),
        );
        assert!(save(dir.path(), &mut s).is_err());
        assert_eq!(s.updated_at, timestamp);
        assert_eq!(
            fs::read(dir.path().join(format!("{}.json", s.id))).unwrap(),
            previous
        );
        assert!(load(dir.path(), &s.id, dir.path())
            .unwrap()
            .messages
            .is_empty());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn does_not_resume_or_list_symlinked_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let mut s = new(dir.path(), "fixture");
        save(outside.path(), &mut s).unwrap();
        let name = format!("{}.json", s.id);
        std::os::unix::fs::symlink(outside.path().join(&name), dir.path().join(&name)).unwrap();
        assert!(load(dir.path(), &s.id, dir.path()).is_err());
        assert!(list_sessions(dir.path()).unwrap().is_empty());
    }
    #[test]
    fn recovery_preserves_later_evidence_positions_and_stable_ids() {
        let root = tempfile::tempdir().unwrap();
        let mut s = new(root.path(), "test");
        s.messages = vec![
            serde_json::json!({"role":"assistant","tool_calls":[{"id":"interrupted"}]}),
            serde_json::json!({"role":"user","content":"inspect before retry"}),
            serde_json::json!({"role":"assistant","tool_calls":[{"id":"observed","function":{"name":"web_fetch","arguments":"{\"url\":\"https://example.test/\"}"}}]}),
            serde_json::json!({"role":"tool","tool_call_id":"observed","content":"{\"ok\":true,\"status\":200}"}),
        ];
        s.observations.push(crate::reasoning::observe(
            3,
            "web_fetch",
            &serde_json::json!({"ok":true,"status":200}),
        ));
        s.task_checks.declared_at = 2;
        s.task_checks
            .observations
            .push(Some(crate::intelligence::checks::Proof {
                evidence_id: "E000003".into(),
                message_index: 3,
                workspace_fingerprint: "fixture".into(),
                coverage_complete: false,
            }));
        assert_eq!(recover_tool_history(&mut s), 1);
        assert_eq!(s.observations[0].id, "E000003");
        assert_eq!(s.observations[0].message_index, 4);
        assert_eq!(
            s.messages[s.observations[0].message_index]["tool_call_id"],
            "observed"
        );
        assert_eq!(s.task_checks.declared_at, 3);
        assert_eq!(
            s.task_checks.observations[0]
                .as_ref()
                .unwrap()
                .message_index,
            4
        );
        assert_eq!(
            crate::audit::reproduction_report(&s)["evidence"][0]["input_availability"],
            "paired_tool_request"
        );
        assert_eq!(recover_tool_history(&mut s), 0);
        assert_eq!(s.observations[0].message_index, 4);
    }
    #[test]
    fn interrupted_tools_get_unknown_results_before_next_user() {
        let root = tempfile::tempdir().unwrap();
        let mut session = new(root.path(), "test");
        session.messages = vec![
            serde_json::json!({"role":"assistant","tool_calls":[{"id":"a"},{"id":"b"}]}),
            serde_json::json!({"role":"tool","tool_call_id":"a","content":"observed"}),
            serde_json::json!({"role":"user","content":"Continue"}),
        ];
        assert_eq!(recover_tool_history(&mut session), 1);
        assert_eq!(session.messages[2]["tool_call_id"], "b");
        assert!(session.messages[2]["content"]
            .as_str()
            .unwrap()
            .contains("unknown"));
        assert_eq!(session.messages[3]["role"], "user");
        assert_eq!(recover_tool_history(&mut session), 0);
    }
}
