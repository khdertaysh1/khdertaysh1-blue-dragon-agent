//! Reopen data that a previous tool result proved was saved. Every read during
//! the continuation is served from the verified bytes, not a mutable pathname.
use crate::{session::Session, tools::ToolBox};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    sync::atomic::AtomicBool,
};

const MAX_SIZE: u64 = 8 * 1024 * 1024;
pub(crate) const TOOLS: &[&str] = &["file_info", "read_file_window", "search_file"];

struct Checkpoint {
    id: Option<String>,
    path: String,
    size: u64,
    sha256: String,
    browser_source: Option<Value>,
}
impl Checkpoint {
    fn metadata(&self) -> Value {
        let mut metadata = json!({"saved_evidence_id":self.id,"path":self.path,"bytes":self.size,"sha256":self.sha256});
        if let Some(source) = &self.browser_source {
            metadata["browser_source"] = source.clone();
        }
        metadata
    }
}

fn checkpoint(session: &Session, id: &str) -> Result<Checkpoint> {
    let (tool, args, result) = crate::audit::request_for(session, id)?;
    ensure!(
        matches!(
            tool.as_str(),
            "write_file" | "edit_file" | "multi_edit_file"
        ) || (tool == "browser" && args["action"] == "save_response"),
        "Evidence is not a native file save"
    );
    ensure!(
        result["ok"] == true
            && result["saved_to_disk"] == true
            && result["storage"] == "disk"
            && !crate::reasoning::claims::is_historical(&result),
        "Evidence does not establish a completed save"
    );
    let path = args["path"].as_str().context("Missing saved path")?;
    ensure!(
        result["path"] == path,
        "Saved path disagrees with the original request"
    );
    let digest = result["sha256"].as_str().context("Missing saved SHA-256")?;
    ensure!(
        digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid saved SHA-256"
    );
    let size = result["bytes"]
        .as_u64()
        .context("Missing saved byte count")?;
    ensure!(size <= MAX_SIZE, "Saved file exceeds continuation limit");
    Ok(Checkpoint {
        id: Some(id.into()),
        path: path.into(),
        size,
        sha256: digest.into(),
        browser_source: (tool == "browser").then(|| json!({
            "origin":result["source_origin"], "request_id":result.get("request_id").unwrap_or(&args["request_id"])
        })),
    })
}

fn recent_checkpoints(session: &Session) -> impl Iterator<Item = Checkpoint> + '_ {
    session.observations.iter().rev().take(300).filter_map(|o| {
        if matches!(
            o.tool.as_str(),
            "browser" | "write_file" | "edit_file" | "multi_edit_file"
        ) {
            checkpoint(session, &o.id).ok()
        } else {
            None
        }
    })
}

pub(crate) fn list(session: &Session) -> Value {
    json!({"saved_files":recent_checkpoints(session).take(20)
        .map(|entry|entry.metadata()).collect::<Vec<_>>(),
        "note":"Historical save records only. /resume-file verifies the current bytes before requesting analysis."})
}

pub(crate) struct SavedFile {
    checkpoint: Checkpoint,
    snapshot: File,
}

impl SavedFile {
    pub(crate) fn open(session: &Session, tools: &ToolBox, id: &str) -> Result<Self> {
        let checkpoint = if id == "latest" {
            recent_checkpoints(session)
                .next()
                .context("No recent verified file save in this session")?
        } else {
            checkpoint(session, id)?
        };
        let (snapshot, size, digest) = capture(tools, &checkpoint.path, checkpoint.size)?;
        ensure!(
            size == checkpoint.size && digest == checkpoint.sha256,
            "Saved file no longer matches its recorded size or SHA-256; no analysis was requested"
        );
        Ok(Self {
            checkpoint,
            snapshot,
        })
    }

    pub(crate) fn open_local(tools: &ToolBox, path: &str) -> Result<Self> {
        let (snapshot, size, sha256) = capture(tools, path, MAX_SIZE)?;
        Ok(Self {
            checkpoint: Checkpoint {
                id: None,
                path: path.into(),
                size,
                sha256,
                browser_source: None,
            },
            snapshot,
        })
    }

    pub(crate) fn seed(&self, tail: bool, cancelled: &AtomicBool) -> Result<(Value, Value)> {
        let mut offset = if tail { self.checkpoint.size.saturating_sub(2048) } else { 0 };
        if offset > 0 {
            let mut file = self.snapshot.try_clone()?;
            file.seek(SeekFrom::Start(offset))?;
            let mut prefix = [0; 4];
            file.read_exact(&mut prefix)?;
            let skip = prefix.iter().position(|byte| byte & 0xc0 != 0x80)
                .context("Invalid UTF-8 boundary in file tail")?;
            offset += skip as u64;
        }
        let args = json!({"path":self.checkpoint.path,"byte_offset":offset,"byte_count":2048});
        let result = self.invoke("read_file_window", &args, cancelled)?;
        Ok((args, result))
    }

    pub(crate) fn metadata(&self) -> Value {
        let mut meta = self.checkpoint.metadata();
        meta["verified"] = json!(true);
        meta["source"] = json!(if self.checkpoint.id.is_some() {
            "recorded save"
        } else {
            "local snapshot"
        });
        meta
    }

    pub(crate) fn invoke(&self, name: &str, args: &Value, cancelled: &AtomicBool) -> Result<Value> {
        ensure!(
            TOOLS.contains(&name),
            "Continuation permits only local file reads"
        );
        ensure!(
            args["path"] == self.checkpoint.path,
            "Continuation can read only the verified file"
        );
        let size = self.checkpoint.size;
        let mut result = match name {
            "file_info" => json!({"bytes":size,"directory":false}),
            "read_file_window" => crate::local_files::read_open(
                &mut self.snapshot.try_clone()?,
                size,
                args["byte_offset"].as_u64().unwrap_or(0),
                args["byte_count"].as_u64().unwrap_or(4096),
                cancelled,
            )?,
            "search_file" => crate::local_files::search_open(
                &mut self.snapshot.try_clone()?,
                size,
                args["query"].as_str().context("Missing query")?,
                args["byte_offset"].as_u64().unwrap_or(0),
                args["scan_bytes"].as_u64().unwrap_or(262144),
                args["max_matches"].as_u64().unwrap_or(20),
                cancelled,
            )?,
            _ => unreachable!(),
        };
        result["path"] = json!(self.checkpoint.path);
        result["source_sha256"] = json!(self.checkpoint.sha256);
        result["saved_evidence_id"] = json!(self.checkpoint.id);
        result["consistency"] =
            json!("immutable snapshot of disk bytes verified at continuation start");
        Ok(result)
    }
}

fn capture(tools: &ToolBox, relative: &str, limit: u64) -> Result<(File, u64, String)> {
    let path = tools.local_file_path(relative)?;
    ensure!(
        path.metadata()?.is_file(),
        "Saved path is no longer a regular file"
    );
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    let before = file.metadata()?;
    ensure!(before.is_file(), "Saved path is no longer a regular file");
    ensure!(before.len() <= limit, "Saved file no longer matches its recorded size or SHA-256, or exceeds the 8 MiB review limit; no analysis was requested");
    let mut source = (&file).take(limit + 1);
    let mut snapshot = tempfile::tempfile()?;
    let mut digest = Sha256::new();
    let mut size = 0;
    let mut buffer = [0u8; 32 * 1024];
    loop {
        ensure!(!tools.is_cancelled(), "File snapshot cancelled");
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        ensure!(size <= limit, "File exceeded the snapshot byte limit");
        digest.update(&buffer[..count]);
        snapshot.write_all(&buffer[..count])?;
    }
    let after = file.metadata()?;
    ensure!(
        size == before.len()
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "File changed while taking snapshot; no analysis was requested"
    );
    Ok((snapshot, size, format!("{:x}", digest.finalize())))
}
