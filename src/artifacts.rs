//! Session-local source artifacts. Large bodies stay on disk, not in model context.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
const MAX_BODY: usize = 8 * 1024 * 1024;
const MAX_TOTAL: usize = 64 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    id: String,
    url: String,
    bytes: usize,
    status: u16,
    content_type: String,
}
pub struct Store {
    directory: PathBuf,
}
impl Store {
    pub fn new(home: &Path, session: &str) -> Self {
        Self {
            directory: home.join("source-artifacts").join(session),
        }
    }
    fn entries(&self) -> Result<Vec<Entry>> {
        let path = self.directory.join("index.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 128_000 {
            bail!("Unsafe source index");
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(128_001)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 128_000 {
            bail!("Source index exceeds limit");
        }
        let entries: Vec<Entry> = serde_json::from_slice(&bytes)?;
        if entries.len() > 32
            || entries.iter().any(|e| {
                e.bytes > MAX_BODY
                    || e.id.len() != 64
                    || !e.id.bytes().all(|c| c.is_ascii_hexdigit())
            })
        {
            bail!("Invalid source index");
        }
        Ok(entries)
    }
    fn metadata(entry: &Entry, reused: bool) -> Value {
        json!({"ok":entry.status<400,"artifact_id":entry.id,"sha256":entry.id,"bytes":entry.bytes,"status":entry.status,"content_type":entry.content_type,"reused_local_copy":reused,"fresh_evidence":!reused,"note":"Unauthenticated source copy. Body retained locally; use source_artifact read/search with artifact_id. Reuse never means fresh remote verification. No scripts executed, redirects or cookies."})
    }
    pub fn fetch(&self, tools: &crate::tools::ToolBox, raw: &str) -> Result<Value> {
        crate::browser::validate_url(raw)?;
        tools.scope.check(raw, "read")?;
        let url = reqwest::Url::parse(raw)?;
        if url.as_str().len() > 2000 {
            bail!("Source URL exceeds 2000 bytes");
        }
        if url.fragment().is_some() {
            bail!("Source URL must not contain a fragment");
        }
        let mut entries = self.entries()?;
        if let Some(entry) = entries.iter().find(|e| e.url == url.as_str()) {
            self.body(&entry.id)?;
            return Ok(Self::metadata(entry, true));
        }
        if entries.len() >= 32 || entries.iter().map(|e| e.bytes).sum::<usize>() >= MAX_TOTAL {
            bail!("Session source cache full (32 sources / 64 MiB); start a new session");
        }
        if !tools.approve("Download source once to local session cache", &format!("GET {url}\nAt most 8 MiB, no cookies or redirects. Search/read bounded excerpts afterward; cached copies may become stale."))? {
            return Ok(json!({"ok":false,"denied":true,"execution_status":"not_executed"}));
        }
        if !tools.allows_url_inspection() {
            bail!("Cancelled before source request");
        }
        tools.progress("Downloading bounded source to local cache...".into());
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()?
            .get(url.clone())
            .send()?;
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            bail!("Source redirect refused; authorize the destination explicitly");
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .chars()
            .take(256)
            .collect::<String>();
        let mut bytes = Vec::new();
        response
            .take((MAX_BODY + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_BODY
            || entries.iter().map(|e| e.bytes).sum::<usize>() + bytes.len() > MAX_TOTAL
        {
            bail!("Source exceeds local cache byte budget; no truncated copy accepted");
        }
        std::str::from_utf8(&bytes).context("Source must be UTF-8 text")?;
        if bytes.contains(&0) {
            bail!("Binary source rejected");
        }
        if !tools.allows_url_inspection() {
            bail!("Cancelled; source not recorded");
        }
        let entry = Entry {
            id: crate::intelligence::code::hash(&bytes),
            url: url.to_string(),
            bytes: bytes.len(),
            status,
            content_type,
        };
        fs::create_dir_all(&self.directory)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        file.write_all(&bytes)?;
        file.persist(self.directory.join(&entry.id))
            .map_err(|e| e.error)?;
        entries.push(entry.clone());
        let mut index = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer(index.as_file_mut(), &entries)?;
        index
            .persist(self.directory.join("index.json"))
            .map_err(|e| e.error)?;
        Ok(Self::metadata(&entry, false))
    }
    fn body(&self, id: &str) -> Result<String> {
        let entry = self
            .entries()?
            .into_iter()
            .find(|e| e.id == id)
            .context("Unknown artifact in this session")?;
        let path = self.directory.join(&entry.id);
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_BODY as u64
        {
            bail!("Unsafe source artifact");
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take((MAX_BODY + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() != entry.bytes || crate::intelligence::code::hash(&bytes) != entry.id {
            bail!("Source artifact changed; hash mismatch");
        }
        Ok(String::from_utf8(bytes)?)
    }
    pub fn trace(&self, query: &str) -> Result<Value> {
        if query.is_empty()
            || query.len() > 100
            || !query
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
        {
            bail!("Trace query must be a bounded identifier/member name");
        }
        let entries = self.entries()?;
        let mut rows = Vec::new();
        let mut bytes_scanned = 0;
        for entry in entries.iter().take(8) {
            if bytes_scanned + entry.bytes > 16 * 1024 * 1024 {
                break;
            }
            let body = self.body(&entry.id)?;
            bytes_scanned += body.len();
            let analysis = crate::source_trace::inspect(&body, query);
            let mut result = match analysis {
                Ok(v) => v,
                Err(_) => {
                    json!({"limited":true,"error":"JavaScript parse unavailable within deadline"})
                }
            };
            if let Some(imports) = result["imports"].as_array_mut() {
                for item in imports {
                    let resolved = reqwest::Url::parse(&entry.url)
                        .ok()
                        .and_then(|base| base.join(item["specifier"].as_str().unwrap_or("")).ok());
                    item["cached_target_artifact"] = resolved
                        .as_ref()
                        .and_then(|url| entries.iter().find(|e| e.url == url.as_str()))
                        .map(|e| json!(e.id))
                        .unwrap_or(Value::Null);
                }
            }
            rows.push(json!({"artifact_id":entry.id,"analysis":result}));
        }
        Ok(
            json!({"ok":true,"query":query,"sources":rows,"bytes_scanned":bytes_scanned,"sources_omitted":entries.len().saturating_sub(rows.len()),"network_requests":0,"fresh_evidence":false,"verdict":"syntactic_candidates_only_not_confirmed_dataflow"}),
        )
    }
    pub fn inspect(&self, args: &Value) -> Result<Value> {
        let id = args["artifact_id"]
            .as_str()
            .context("artifact_id required")?;
        let body = self.body(id)?;
        let mut result = json!({"ok":true,"artifact_id":id,"untrusted_source_data":true,"fresh_evidence":false,"total_bytes":body.len(),"offset_unit":"UTF-8 bytes","network_requests":0});
        match args["action"].as_str() {
            Some("read") => {
                for field in ["offset", "count"] {
                    if args.get(field).is_some() && args[field].as_u64().is_none() {
                        bail!("Source offsets/counts must be unsigned integers");
                    }
                }
                let offset = usize::try_from(args["offset"].as_u64().unwrap_or(0))?;
                let count = usize::try_from(args["count"].as_u64().unwrap_or(4000))?;
                if offset > body.len()
                    || !body.is_char_boundary(offset)
                    || !(1..=16000).contains(&count)
                {
                    bail!("Use a valid UTF-8 byte offset and count 1..16000");
                }
                let mut end = (offset + count).min(body.len());
                while !body.is_char_boundary(end) {
                    end -= 1;
                }
                if end == offset && offset < body.len() {
                    bail!("Count is smaller than the next UTF-8 character");
                }
                result["offset"] = json!(offset);
                result["excerpt"] = json!(&body[offset..end]);
                result["next_offset"] = if end < body.len() {
                    json!(end)
                } else {
                    Value::Null
                };
            }
            Some("search") => {
                let query = args["query"].as_str().context("query required")?;
                if query.is_empty() || query.len() > 200 {
                    bail!("Search needs 1..200 UTF-8 bytes");
                }
                let mut rows = Vec::new();
                let mut hits = body.match_indices(query);
                for (at, _) in hits.by_ref().take(20) {
                    let mut start = at.saturating_sub(80);
                    while !body.is_char_boundary(start) {
                        start += 1;
                    }
                    let mut end = (at + query.len() + 120).min(body.len());
                    while !body.is_char_boundary(end) {
                        end -= 1;
                    }
                    rows.push(
                        json!({"offset":at,"excerpt_offset":start,"excerpt":&body[start..end]}),
                    );
                }
                result["matches"] = json!(rows);
                result["more_matches"] = json!(hits.next().is_some());
                result["analysis"] =
                    json!("Literal matches only; not semantic dataflow or a vulnerability verdict");
            }
            _ => bail!("Action must be read/search"),
        }
        Ok(result)
    }
}
pub fn definition() -> Value {
    json!({"type":"function","function":{"name":"source_artifact","description":"Fetch large UTF-8 JavaScript/text once into a bounded local session cache, then read/search without refetching or sending the whole body. trace with query gives bounded AST assignment/call/return candidates and cached import links, never taint or sanitizer proof. fetch requires explicit approval; max 8 MiB/source, 32 sources/64 MiB/session. Historical cache is not fresh remote evidence. read/search use returned SHA256 artifact_id and UTF-8 byte offsets.","parameters":{"type":"object","properties":{"action":{"type":"string","enum":["fetch","read","search","trace"]},"url":{"type":"string"},"artifact_id":{"type":"string"},"offset":{"type":"integer","minimum":0},"count":{"type":"integer","minimum":1,"maximum":16000},"query":{"type":"string"}},"required":["action"],"additionalProperties":false}}})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_source_downloaded_once_and_searchable_after_reload() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            "{}TAIL_TARGET=location.hash;",
            "const x='ع';\n".repeat(50000)
        );
        let (base, server) = crate::test_support::http(vec![("text/javascript", body.clone())]);
        let (tx, rx) = std::sync::mpsc::channel::<crate::tools::ApprovalRequest>();
        let tools =
            crate::tools::ToolBox::new(dir.path().into(), crate::tools::ApprovalMode::Tui(tx));
        let consent = std::thread::spawn(move || rx.recv().unwrap().respond_to.send(true).unwrap());
        let store = Store::new(dir.path(), "fixture");
        let first = store.fetch(&tools, &base).unwrap();
        consent.join().unwrap();
        server.join().unwrap();
        assert!(first.to_string().len() < 1500);
        assert_eq!(first["bytes"], body.len());
        let store = Store::new(dir.path(), "fixture");
        // Server is gone: successful reuse proves no second network request.
        assert_eq!(
            store.fetch(&tools, &base).unwrap()["reused_local_copy"],
            true
        );
        let id = first["artifact_id"].as_str().unwrap();
        let found = store
            .inspect(&json!({"action":"search","artifact_id":id,"query":"TAIL_TARGET"}))
            .unwrap();
        let offset = found["matches"][0]["offset"].as_u64().unwrap();
        let read = store
            .inspect(&json!({"action":"read","artifact_id":id,"offset":offset,"count":100}))
            .unwrap();
        assert_eq!(read["excerpt"], "TAIL_TARGET=location.hash;");
        assert!(Store::new(dir.path(), "other")
            .inspect(&json!({"action":"read","artifact_id":id}))
            .is_err());
        fs::write(store.directory.join(id), "tampered").unwrap();
        assert!(store
            .inspect(&json!({"action":"read","artifact_id":id}))
            .is_err());
    }
}
