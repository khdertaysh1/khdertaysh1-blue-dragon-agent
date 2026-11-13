use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
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
}

#[derive(Debug)]
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
    }
}

pub fn save(sessions_dir: &Path, session: &mut Session) -> Result<()> {
    session.updated_at = Utc::now();
    let output = sessions_dir.join(format!("{}.json", session.id));
    let mut file = tempfile::NamedTempFile::new_in(sessions_dir)?;
    serde_json::to_writer_pretty(file.as_file_mut(), session)?;
    use std::io::Write;
    file.as_file_mut().flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file_mut()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.persist(&output)
        .map_err(|error| error.error)
        .with_context(|| format!("Could not persist session {}", session.id))?;
    Ok(())
}

pub fn load(sessions_dir: &Path, id: &str, root: &Path) -> Result<Session> {
    let id = if id == "latest" {
        list_sessions(sessions_dir)?
            .iter()
            .find(|s| s.workspace == root.to_string_lossy())
            .map(|s| s.id.clone())
            .context("No saved sessions")?
    } else {
        id.to_owned()
    };
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!("Invalid session ID");
    }
    let path = sessions_dir.join(format!("{id}.json"));
    if fs::metadata(&path)?.len() > 16_000_000 {
        anyhow::bail!("Session exceeds 16 MB limit");
    }
    let session: Session = serde_json::from_slice(&fs::read(path)?)?;
    if session.workspace != root.to_string_lossy() {
        anyhow::bail!("Session belongs to another workspace; refusing to resume");
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
        if fs::metadata(&path)
            .map(|m| m.len() > 16_000_000)
            .unwrap_or(true)
        {
            continue;
        }
        if let Ok(bytes) = fs::read(path) {
            if let Ok(session) = serde_json::from_slice::<Session>(&bytes) {
                out.push(SessionSummary {
                    id: session.id,
                    workspace: session.workspace,
                    updated_at: session.updated_at,
                });
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
}
