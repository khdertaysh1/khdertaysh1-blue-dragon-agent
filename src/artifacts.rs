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
// Bare packages are not relative browser URLs. Import maps/bundler resolution
// require information this bounded cached-source analysis does not possess.
fn resolve_import(base: &str, specifier: &str) -> Option<reqwest::Url> {
    if specifier.contains('\\') {
        return None;
    }
    if !(specifier.starts_with("./")
        || specifier.starts_with("../")
        || specifier.starts_with('/')
        || specifier.starts_with("https://")
        || specifier.starts_with("http://"))
    {
        return None;
    }
    let resolved = reqwest::Url::parse(base).ok()?.join(specifier).ok()?;
    matches!(resolved.scheme(), "https" | "http").then_some(resolved)
}
// Linear scan, constant auxiliary space; deliberately not an edit-distance diff.
fn changed_span(before: &str, after: &str) -> (usize, usize, usize) {
    let mut prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(prefix) || !after.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = before.as_bytes()[prefix..]
        .iter()
        .rev()
        .zip(after.as_bytes()[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(before.len() - suffix)
        || !after.is_char_boundary(after.len() - suffix)
    {
        suffix -= 1;
    }
    (prefix, before.len() - suffix, after.len() - suffix)
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
    pub fn require_local_project_sources(&self) -> Result<()> {
        for entry in self.entries()? {
            let url = reqwest::Url::parse(&entry.url)?;
            anyhow::ensure!(
                crate::browser::local_project_url(&url),
                "Cross-file tracing and reference discovery require a local development project"
            );
        }
        Ok(())
    }
    fn metadata(entry: &Entry, reused: bool) -> Value {
        json!({"ok":entry.status<400,"artifact_id":entry.id,"sha256":entry.id,"bytes":entry.bytes,"status":entry.status,"content_type":entry.content_type,"reused_local_copy":reused,"fresh_evidence":!reused,"note":"Unauthenticated source copy. Body retained locally; use source_artifact read/search with artifact_id. Reuse never means fresh remote verification. No scripts executed, redirects or cookies."})
    }
    pub fn fetch(&self, tools: &crate::tools::ToolBox, raw: &str) -> Result<Value> {
        self.download(tools, raw, false)
    }
    pub fn refresh(&self, tools: &crate::tools::ToolBox, raw: &str) -> Result<Value> {
        self.download(tools, raw, true)
    }
    fn download(&self, tools: &crate::tools::ToolBox, raw: &str, refresh: bool) -> Result<Value> {
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
        let previous = entries
            .iter()
            .rev()
            .find(|e| e.url == url.as_str())
            .cloned();
        if let Some(entry) = &previous {
            self.body(&entry.id)?;
            if !refresh {
                return Ok(Self::metadata(entry, true));
            }
        } else if refresh {
            bail!("Refresh requires an existing source in this session; use fetch first");
        }
        if entries.len() >= 32 || entries.iter().map(|e| e.bytes).sum::<usize>() >= MAX_TOTAL {
            bail!("Session source cache full (32 sources / 64 MiB); start a new session");
        }
        if !tools.approve(if refresh {"Refresh saved source with one new GET; preserve prior version"} else {"Download source once to local session cache"}, &format!("GET {url}\nAt most 8 MiB, no cookies or redirects. Search/read bounded excerpts afterward; cached copies may become stale."))? {
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
        if refresh && !response.status().is_success() {
            return Ok(
                json!({"ok":false,"status":status,"refresh_completed":false,"previous_artifact_id":previous.as_ref().map(|e|&e.id),"cache_preserved":true,"note":"Refresh failed; previous source remains available. No redirect followed."}),
            );
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
        let id = crate::intelligence::code::hash(&bytes);
        let unchanged = previous.as_ref().is_some_and(|e| e.id == id);
        if bytes.len() > MAX_BODY
            || entries.iter().map(|e| e.bytes).sum::<usize>()
                + if unchanged { 0 } else { bytes.len() }
                > MAX_TOTAL
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
            id,
            url: url.to_string(),
            bytes: bytes.len(),
            status,
            content_type,
        };
        let mut metadata = Self::metadata(&entry, false);
        if refresh {
            metadata["refresh_completed"] = json!(true);
            metadata["previous_artifact_id"] = json!(previous.as_ref().map(|e| &e.id));
            metadata["content_changed"] = json!(!unchanged);
            metadata["version_history_preserved"] = json!(true);
            metadata["note"]=json!("One newly approved unauthenticated GET. Hash change is content change only, not vulnerability or semantic proof. Prior content remains readable by artifact_id.");
        }
        if unchanged {
            return Ok(metadata);
        }
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
        Ok(metadata)
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
    pub fn discover(
        &self,
        tools: &crate::tools::ToolBox,
        id: &str,
        selected: Option<usize>,
    ) -> Result<Value> {
        let entries = self.entries()?;
        let roots = entries.iter().filter(|e| e.id == id).collect::<Vec<_>>();
        if roots.len() != 1 {
            bail!("Discovery requires one unambiguous cached source URL");
        }
        let entry = roots[0];
        if entry.bytes > 2 * 1024 * 1024
            || fs::symlink_metadata(self.directory.join(id))?.len() > 2 * 1024 * 1024
        {
            bail!("Discovery source exceeds 2 MiB; use bounded read/search");
        }
        let body = self.body(id)?;
        let is_html = entry
            .content_type
            .split(';')
            .next()
            .is_some_and(|s| s.trim().eq_ignore_ascii_case("text/html"));
        let scan = if is_html {
            crate::source_discovery::html(&body)
        } else {
            let analysis =
                crate::source_trace::inspect(&body, "__blue_discovery_unused_identifier")?;
            let mut references = analysis["imports"].as_array().into_iter().flatten().map(|i|json!({"specifier":i["specifier"],"byte_offset":i["byte_offset"],"kind":"static_module_candidate"})).collect::<Vec<_>>();
            references.extend(
                analysis["dynamic_imports"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            references.sort_by_key(|r| r["byte_offset"].as_u64().unwrap_or(0));
            json!({"references":references,"limited":analysis["limited"]==true || analysis["parse_errors"]==true,"runtime_coverage_complete":false})
        };
        let mut rows = Vec::new();
        let mut targets = Vec::new();
        let mut seen = std::collections::BTreeMap::new();
        let mut missing = 0;
        let mut indexed = 0;
        let mut outside = 0;
        let mut unresolved = 0;
        let mut duplicates = 0;
        for (index, reference) in scan["references"]
            .as_array()
            .into_iter()
            .flatten()
            .take(80)
            .enumerate()
        {
            let specifier = reference["specifier"].as_str().unwrap_or("");
            let resolved = if specifier.is_empty()
                || scan["base_element_seen"] == true
                || specifier.contains('\\')
            {
                None
            } else if is_html {
                reqwest::Url::parse(&entry.url)?.join(specifier).ok()
            } else {
                resolve_import(&entry.url, specifier)
            };
            let resolved = resolved.filter(|u| {
                u.as_str().len() <= 2000
                    && u.fragment().is_none()
                    && crate::browser::validate_url(u.as_str()).is_ok()
            });
            let mut target = None;
            let mut cached = None;
            let mut duplicate = None;
            let state = if let Some(url) = resolved.as_ref() {
                if let Some(first) = seen.get(url.as_str()) {
                    duplicates += 1;
                    duplicate = Some(*first);
                    "duplicate_reference"
                } else {
                    seen.insert(url.to_string(), index);
                    if !tools.scope.enabled || tools.scope.check(url.as_str(), "read").is_err() {
                        outside += 1;
                        "outside_current_read_scope"
                    } else {
                        target = Some(url.to_string());
                        cached = entries
                            .iter()
                            .rev()
                            .find(|e| e.url == url.as_str())
                            .map(|e| e.id.clone());
                        if cached.is_some() {
                            indexed += 1;
                            "cache_index_match_not_revalidated"
                        } else {
                            missing += 1;
                            "missing_local_source"
                        }
                    }
                }
            } else {
                unresolved += 1;
                "unresolved_or_unsupported_reference"
            };
            rows.push(json!({"reference_index":index,"kind":reference["kind"],"source_byte_offset":reference["byte_offset"],"target_redacted":resolved.as_ref().map(|u|crate::audit::safe_url(u.as_str())),"specifier_sha256":crate::intelligence::code::hash(specifier.as_bytes()),"state":state,"duplicate_of":duplicate,"indexed_artifact_id":cached,"fetch_available":target.is_some(),"loaded_by_browser":false}));
            targets.push(target);
        }
        if let Some(index) = selected {
            let url = targets.get(index).and_then(Option::as_ref).context(
                "Reference is unresolved, duplicate, or outside current scope; no fetch performed",
            )?;
            // Re-derive from the hash-verified root on every call; never accept an
            // arbitrary caller URL as a discovered dependency.
            anyhow::ensure!(
                crate::browser::local_project_url(&reqwest::Url::parse(url)?),
                "Automatic dependency loading is limited to local project files"
            );
            let mut result = self.fetch(tools, url)?;
            result["discovered_from_artifact"] = json!(id);
            result["reference_index"] = json!(index);
            return Ok(result);
        }
        Ok(
            json!({"ok":true,"root_artifact_id":id,"collection":if is_html{"bounded_HTML_lexical_candidates"}else{"bounded_JavaScript_module_candidates"},"references":rows,"limited":scan["limited"],"base_element_seen":scan["base_element_seen"],"inline_script_blocks_not_analyzed":scan["inline_script_blocks_not_analyzed"],"incomplete_script_tags":scan["incomplete_script_tags"],"uncollected_reference_count":if scan["limited"]==true {None}else{Some(0)},"counts":{"unique_missing_in_scope":missing,"unique_index_matches":indexed,"outside_scope":outside,"unresolved":unresolved,"duplicates":duplicates},"network_requests":0,"fresh_evidence":false,"runtime_coverage_complete":false,"limits":"Static candidates only, not browser-loaded resources or full site coverage. HTML parsing is conservative lexical scanning, not a DOM; templates, dynamic insertion, computed or escaped import arguments, import maps and runtime fetches are not resolved. Literal import() references are only syntactic collection candidates, not confirmed loads. A base element conservatively prevents all reference resolution. Query values are redacted. Cache matches refer to the latest index version and are verified only when opened/fetched. Fetch one reference with separate explicit approval; never auto-download the manifest."}),
        )
    }
    pub fn trace(&self, query: &str, selected: Option<&str>) -> Result<Value> {
        if query.is_empty()
            || query.len() > 100
            || !query
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
        {
            bail!("Trace query must be a bounded identifier/member name");
        }
        let entries = self.entries()?;
        if selected.is_some_and(|id| !entries.iter().any(|entry| entry.id == id)) {
            bail!("Selected artifact is not in this session");
        }
        let eligible = entries
            .iter()
            .filter(|entry| {
                selected.map_or_else(
                    || {
                        entries
                            .iter()
                            .rev()
                            .find(|e| e.url == entry.url)
                            .is_some_and(|latest| std::ptr::eq(latest, *entry))
                    },
                    |id| entry.id == id,
                )
            })
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        let mut bytes_scanned = 0;
        for entry in eligible.iter().take(8) {
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
                    let specifier = item["specifier"].as_str().unwrap_or("");
                    let resolved = resolve_import(&entry.url, specifier);
                    item["resolution"] = json!(if resolved.is_some() {
                        "url_only_import_maps_unverified"
                    } else {
                        "unresolved_requires_package_or_import_map_context"
                    });
                    item["target_version_basis"] =
                        json!("latest_cached_version_not_same_capture_guaranteed");
                    item["cached_target_artifact"] = resolved
                        .as_ref()
                        .and_then(|url| entries.iter().rev().find(|e| e.url == url.as_str()))
                        .map(|e| json!(e.id))
                        .unwrap_or(Value::Null);
                    let follow_up=item["bindings"].as_array().into_iter().flatten()
                        .filter_map(|binding| {
                            let local=binding["local_or_exported"].as_str()?;
                            let imported=binding["imported"].as_str()?;
                            let (name, reason)=if imported=="*" && binding["kind"]=="namespace_import" {
                                (query.strip_prefix(local)?.strip_prefix('.')?, "syntactic_namespace_member_candidate")
                            } else if local==query {
                                (imported,"syntactic_import_alias_candidate")
                            } else { return None; };
                            let target=item["cached_target_artifact"].as_str()?;
                            if name.is_empty() || name=="default" || name=="*" || name.len()>100 || !name.chars().all(|c|c.is_alphanumeric() || c=='_' || c=='$') { return None; }
                            Some(json!({"action":"trace","artifact_id":target,"query":name,"reason":reason,"semantic_link_verified":false}))
                        }).take(4).collect::<Vec<_>>();
                    item["suggested_local_follow_up"] = json!(follow_up);
                }
            }
            rows.push(json!({"artifact_id":entry.id,"source_location_sha256":crate::intelligence::code::hash(entry.url.as_bytes()),"analysis":result}));
        }
        Ok(
            json!({"ok":true,"query":query,"sources":rows,"bytes_scanned":bytes_scanned,"sources_omitted":eligible.len().saturating_sub(rows.len()),"sources_outside_selection":entries.len().saturating_sub(eligible.len()),"version_selection":if selected.is_some(){"explicit_artifact_history"}else{"latest_per_url"},"network_requests":0,"fresh_evidence":false,"verdict":"syntactic_candidates_only_not_confirmed_dataflow"}),
        )
    }
    /// Follow explicit cached import/re-export aliases, never fetch missing files.
    pub fn bridge_call(
        &self,
        query: &str,
        id: &str,
        offset: usize,
        argument: usize,
    ) -> Result<Value> {
        if query.contains('.') {
            bail!("Call bridge currently requires a simple named import identifier");
        }
        if argument >= 8 {
            bail!("argument_index must be 0..7");
        }
        let entries = self.entries()?;
        let roots = entries.iter().filter(|e| e.id == id).collect::<Vec<_>>();
        if roots.len() != 1
            || roots[0].bytes > 1024 * 1024
            || fs::symlink_metadata(self.directory.join(id))?.len() > 1024 * 1024
        {
            bail!("Call bridge requires one unambiguous caller of at most 1 MiB");
        }
        let root = self.trace(query, Some(id))?;
        let sources = root["sources"].as_array().context("Missing source")?;
        if sources.len() != 1 {
            bail!("Call selection requires one unambiguous cached source URL");
        }
        let analysis = &sources[0]["analysis"];
        let candidates = analysis["candidates"]
            .as_array()
            .context("No parsed candidates")?;
        let calls = candidates
            .iter()
            .filter(|c| {
                c["kind"] == "call_candidate_not_a_validation_guarantee"
                    && c["target"] == query
                    && c["byte_offset"].as_u64() == Some(offset as u64)
            })
            .collect::<Vec<_>>();
        if calls.len() != 1 {
            bail!("Select the exact byte offset of one named call candidate");
        }
        let call = calls[0];
        let blocked = |reason: &str| json!({"ok":true,"mapping_available":false,"reason":reason,"artifact_id":id,"call_byte_offset":offset,"network_requests":0,"fresh_evidence":false,"semantic_dataflow_verified":false});
        if call["function_scope"]["kind"] != "program" {
            return Ok(blocked("non_module_caller_binding_scope_not_supported"));
        }
        if analysis["limited"] == true || analysis["parse_errors"] == true {
            return Ok(blocked("caller_analysis_incomplete"));
        }
        let shadowed = candidates.iter().any(|c| {
            let written = matches!(
                c["kind"].as_str(),
                Some("syntactic_assignment" | "syntactic_update")
            ) && c["target"] == query;
            let parameter = c["declaration_parameters"]["names"]
                .as_array()
                .is_some_and(|names| names.iter().any(|n| n == query))
                && c["byte_offset"]
                    .as_u64()
                    .is_some_and(|start| start <= offset as u64)
                && c["end_byte"]
                    .as_u64()
                    .is_some_and(|end| end > offset as u64);
            written || parameter
        });
        if shadowed {
            return Ok(blocked("possible_local_shadow_or_reassignment"));
        }
        let chain = self.trace_chain(query, id)?;
        if chain["limited"] == true {
            return Ok(blocked("import_chain_incomplete_or_unresolved"));
        }
        if chain["nodes"].as_array().into_iter().flatten().any(|node| {
            node["candidates"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|candidate| {
                    matches!(
                        candidate["kind"].as_str(),
                        Some("syntactic_assignment" | "syntactic_update")
                    ) && candidate["target"] == node["query"]
                })
        }) {
            return Ok(blocked("possible_import_chain_binding_write"));
        }
        let definitions = chain["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|node| {
                node["candidates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(move |c| {
                        (node["artifact_id"] != id
                            && c["kind"]
                                == "function_declaration_candidate_not_resolved_call_target"
                            && c["target"] == node["query"])
                            .then_some((node, c))
                    })
            })
            .collect::<Vec<_>>();
        if definitions.len() != 1 {
            return Ok(blocked("no_unique_named_cross_file_definition"));
        }
        let (node, definition) = definitions[0];
        if definition["direct_named_module_export"] != true {
            return Ok(blocked("definition_not_direct_named_module_export"));
        }
        let args = &call["call_arguments"];
        let params = &definition["declaration_parameters"];
        if args["simple_identifiers_only"] != true
            || params["simple_identifiers_only"] != true
            || args["count"] != params["count"]
            || args["count"] == 0
        {
            return Ok(blocked("unsupported_arguments_parameters_or_arity"));
        }
        let argument_name = args["names"]
            .get(argument)
            .and_then(Value::as_str)
            .context("Argument index outside the selected call")?;
        let parameter = params["names"]
            .get(argument)
            .and_then(Value::as_str)
            .context("Parameter unavailable")?;
        let names = params["names"].as_array().unwrap();
        if names
            .iter()
            .filter(|n| n.as_str() == Some(parameter))
            .count()
            != 1
        {
            return Ok(blocked("duplicate_parameter_name"));
        }
        let target = node["artifact_id"]
            .as_str()
            .context("Missing callee artifact")?;
        let anchor = definition["byte_offset"]
            .as_u64()
            .context("Missing declaration offset")? as usize;
        let callee_flow = self.flow(parameter, target, anchor)?;
        Ok(
            json!({"ok":true,"mapping_available":true,"mapping_kind":"syntactic_positional_candidate","caller_artifact_id":id,"call_byte_offset":offset,"argument_index":argument,"argument_symbol":argument_name,"callee_artifact_id":target,"callee_symbol":node["query"],"parameter_symbol":parameter,"import_chain":chain,"callee_flow":callee_flow,"network_requests":0,"fresh_evidence":false,"semantic_dataflow_verified":false,"limits":"Module-level named cached import/re-export candidate plus positional simple-identifier correspondence only. Calls inside functions are rejected because lexical binding resolution is incomplete. No runtime dispatch, execution, taint, return-value, sanitizer, branch or exploit proof. Local declarations/reassignments and named-function parameter shadowing reject the bridge; other shadowing forms may remain unresolved. Defaults, rest/destructuring, expression arguments and unequal arity are unsupported. Target versions may not share the caller capture. Inspect exact source ranges and independent behavior before a finding."}),
        )
    }
    pub fn flow(&self, query: &str, id: &str, anchor: usize) -> Result<Value> {
        if query.is_empty()
            || query.len() > 100
            || !query
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
        {
            bail!("Flow query must be an identifier/member name");
        }
        let entry = self
            .entries()?
            .into_iter()
            .find(|e| e.id == id)
            .context("Unknown source artifact")?;
        if entry.bytes > 1024 * 1024
            || fs::symlink_metadata(self.directory.join(id))?.len() > 1024 * 1024
        {
            bail!("Scoped flow source exceeds 1 MiB; use bounded read/search");
        }
        let body = self.body(id)?;
        let started = std::time::Instant::now();
        let mut queue = std::collections::VecDeque::from([query.to_owned()]);
        let mut seen = std::collections::BTreeSet::new();
        let mut steps = Vec::new();
        let mut limited = false;
        let mut scope = Value::Null;
        while let Some(symbol) = queue.pop_front() {
            if !seen.insert(symbol.clone()) {
                continue;
            }
            if seen.len() > 6 || steps.len() >= 24 || started.elapsed() > Duration::from_secs(1) {
                queue.push_front(symbol);
                limited = true;
                break;
            }
            let analysis = crate::source_trace::inspect_scoped(&body, &symbol, anchor)?;
            scope = analysis["selected_function_scope"].clone();
            limited |= analysis["limited"] == true || analysis["parse_errors"] == true;
            let candidates = analysis["candidates"]
                .as_array()
                .context("Missing syntax candidates")?;
            let writes = candidates
                .iter()
                .filter(|c| {
                    matches!(
                        c["kind"].as_str(),
                        Some("syntactic_assignment" | "syntactic_update")
                    ) && c["target"] == symbol
                })
                .count();
            let has_updates = candidates.iter().any(|c| c["kind"] == "syntactic_update");
            limited |= writes > 1 || has_updates;
            for candidate in candidates {
                if candidate["query_in_input"] != true {
                    continue;
                }
                if steps.len() >= 24 {
                    limited = true;
                    break;
                }
                let target = candidate["target"].as_str().unwrap_or("");
                let can_follow = candidate["kind"] == "syntactic_assignment"
                    && candidate["target_is_identifier"] == true
                    && !target.is_empty()
                    && target.len() <= 100
                    && writes <= 1
                    && !has_updates;
                let from = candidate["byte_offset"].as_u64().unwrap_or(0) as usize;
                let end = candidate["end_byte"].as_u64().unwrap_or(from as u64) as usize;
                steps.push(json!({"from_symbol":symbol,"syntax":candidate,"possible_competing_writes":writes>1 || has_updates,"followed_local_identifier":can_follow,"read_suggestion":{"action":"read","artifact_id":id,"offset":from,"count":end.saturating_sub(from).clamp(1,16000)},"semantic_flow_verified":false}));
                if can_follow && !seen.contains(target) {
                    queue.push_back(target.to_owned());
                }
                if writes > 1 {
                    limited = true;
                }
            }
        }
        Ok(
            json!({"ok":true,"artifact_id":id,"query":query,"scope_anchor_byte":anchor,"selected_function_scope":scope,"steps":steps,"limited":limited || !queue.is_empty(),"pending_symbols":queue.len(),"symbols_examined":seen.len().min(6),"network_requests":0,"fresh_evidence":false,"semantic_dataflow_verified":false,"budgets":{"source_bytes":1048576,"symbols":6,"steps":24,"cooperative_milliseconds":1000},"limits":"Bounded same-function syntax paths, not execution/taint proof. Nested function bodies are excluded from input references. Alias propagation follows simple identifier assignments only; property writes/calls/returns are inspection endpoints, not proven sinks or validators. Multiple writes block automatic propagation from that symbol. Branch order, block shadowing, object aliasing, async effects and call return semantics are unresolved. Exact read suggestions recover context; source excerpts remain untrusted."}),
        )
    }
    pub fn trace_chain(&self, query: &str, root: &str) -> Result<Value> {
        let entries = self.entries()?;
        if query.is_empty()
            || query.len() > 100
            || !query
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
        {
            bail!("Trace query must be a bounded identifier/member name");
        }
        if !entries.iter().any(|e| e.id == root) {
            bail!("Root artifact is not in this session");
        }
        let started = std::time::Instant::now();
        let mut queue =
            std::collections::VecDeque::from([(root.to_owned(), query.to_owned(), 0usize)]);
        let mut seen = std::collections::BTreeSet::new();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut blocked = Vec::new();
        let mut bytes = 0usize;
        let mut limited = false;
        while let Some((id, symbol, depth)) = queue.pop_front() {
            if !seen.insert((id.clone(), symbol.clone())) {
                continue;
            }
            if nodes.len() >= 6 || started.elapsed() > Duration::from_secs(1) {
                queue.push_front((id, symbol, depth));
                limited = true;
                break;
            }
            let associated = entries.iter().filter(|e| e.id == id).collect::<Vec<_>>();
            if associated.len() != 1 {
                blocked.push(json!({"artifact_id":id,"query":symbol,"reason":"ambiguous_source_URL_association"}));
                limited = true;
                continue;
            }
            let entry = associated[0];
            let disk_bytes = fs::symlink_metadata(self.directory.join(&id))?.len();
            if entry.bytes > 2 * 1024 * 1024usize - bytes
                || disk_bytes > (2 * 1024 * 1024usize - bytes) as u64
            {
                blocked.push(
                    json!({"artifact_id":id,"query":symbol,"reason":"cumulative_byte_budget"}),
                );
                limited = true;
                continue;
            }
            let traced = self.trace(&symbol, Some(&id))?;
            bytes += traced["bytes_scanned"].as_u64().unwrap_or(0) as usize;
            let analysis = &traced["sources"][0]["analysis"];
            let candidates = analysis["candidates"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            limited |= analysis["limited"] == true
                || analysis["parse_errors"] == true
                || candidates.len() > 8;
            let incomplete_import_collections = analysis["imports"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|item| item["bindings_limited"] == true)
                .count();
            limited |= incomplete_import_collections > 0;
            let node_index = nodes.len();
            nodes.push(json!({"artifact_id":id,"query":symbol,"depth":depth,"candidates":candidates.iter().take(8).collect::<Vec<_>>(),"candidates_omitted":candidates.len().saturating_sub(8),"parse_errors":analysis["parse_errors"],"analysis_limited":analysis["limited"],"import_bindings_limited":incomplete_import_collections>0,"incomplete_import_collections":incomplete_import_collections,"semantic_link_verified":false}));
            let mut followed = 0;
            for import in analysis["imports"].as_array().into_iter().flatten() {
                // A discarded name may be the queried symbol; report before
                // filtering relevance using the surviving binding prefix.
                if import["bindings_limited"] == true && blocked.len() < 24 {
                    blocked.push(json!({"from_node":node_index,"import_byte_offset":import["byte_offset"],"reason":"import_binding_collection_incomplete","bindings_retained":import["bindings"].as_array().map(Vec::len),"bindings_omitted_count":null}));
                }
                let relevant = import["bindings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|binding| {
                        let local = binding["local_or_exported"].as_str().unwrap_or("");
                        local == symbol
                            || binding["kind"] == "namespace_import"
                                && symbol
                                    .strip_prefix(local)
                                    .is_some_and(|tail| tail.starts_with('.'))
                    });
                if !relevant {
                    continue;
                }
                let follows = import["suggested_local_follow_up"].as_array();
                if follows.is_none_or(|f| f.is_empty()) {
                    if blocked.len() < 24 {
                        blocked.push(json!({"from_node":node_index,"import_byte_offset":import["byte_offset"],"reason":"unsupported_binding_or_missing_cached_target","resolution":import["resolution"]}));
                    }
                    limited = true;
                }
                for follow in follows.into_iter().flatten() {
                    if followed >= 4 || edges.len() >= 24 {
                        limited = true;
                        break;
                    }
                    followed += 1;
                    let target = follow["artifact_id"]
                        .as_str()
                        .context("Missing cached target")?;
                    let next = follow["query"].as_str().context("Missing cached symbol")?;
                    let repeated = seen.contains(&(target.to_owned(), next.to_owned()));
                    let state = if repeated {
                        "already_visited_candidate"
                    } else if depth >= 3 {
                        "depth_limit"
                    } else {
                        "queued_cached_candidate"
                    };
                    edges.push(json!({"from_node":node_index,"to_artifact_id":target,"to_query":next,"import_byte_offset":import["byte_offset"],"reason":follow["reason"],"state":state,"semantic_link_verified":false,"target_version_basis":"latest_cached_not_same_capture_guaranteed"}));
                    if depth >= 3 {
                        limited = true;
                    } else if !repeated {
                        queue.push_back((target.to_owned(), next.to_owned(), depth + 1));
                    }
                }
            }
        }
        let pending = queue.len();
        Ok(
            json!({"ok":true,"root_artifact_id":root,"query":query,"nodes":nodes,"edges":edges,"blocked":blocked,"pending_candidates":pending,"limited":limited || pending>0,"bytes_scanned":bytes,"budgets":{"nodes":6,"depth":3,"bytes":2097152,"milliseconds":1000,"candidates_per_node":8},"fresh_evidence":false,"network_requests":0,"semantic_dataflow_verified":false,"verdict":"cached_syntactic_chain_only","limits":"Follows explicit cached aliases and re-exports only. No automatic download, package/import-map/type resolution, runtime call dispatch, taint, sanitizer correctness or exploitability proof. Shadowing may invalidate syntactic candidates. Default/wildcard exports may remain unresolved. Latest target versions may not belong to the root capture. Source excerpts are untrusted project data."}),
        )
    }
    pub fn compare_versions(&self, baseline_id: &str, variant_id: &str) -> Result<Value> {
        let entries = self.entries()?;
        let common = entries
            .iter()
            .filter(|entry| {
                entry.id == baseline_id
                    && entries
                        .iter()
                        .any(|other| other.id == variant_id && other.url == entry.url)
            })
            .map(|entry| crate::intelligence::code::hash(entry.url.as_bytes()))
            .collect::<std::collections::BTreeSet<_>>();
        if common.is_empty() {
            bail!("Comparison requires two session artifacts recorded for a common source URL");
        }
        let before = self.body(baseline_id)?;
        let after = self.body(variant_id)?;
        let (start, before_end, after_end) = changed_span(&before, &after);
        let span = |id: &str, body: &str, end: usize| {
            let length = end - start;
            json!({"artifact_id":id,"total_bytes":body.len(),"start_byte":start,"end_byte_exclusive":end,"enclosing_changed_bytes":length,
                "read_suggestion":if length==0 {Value::Null}else{json!({"action":"read","artifact_id":id,"offset":start,"count":length.min(16000)})},"suggested_read_is_partial":length>16000})
        };
        Ok(
            json!({"ok":true,"fresh_evidence":false,"network_requests":0,"content_equal":before==after,"common_source_location_hashes":common,"baseline":span(baseline_id,&before,before_end),"variant":span(variant_id,&after,after_end),"offset_unit":"UTF-8 bytes","bytes_compared":before.len()+after.len(),"semantic_change_verified":false,"vulnerability_confirmed":false,
            "limits":"One enclosing changed span after a common prefix/suffix scan, not a minimal diff. Separate edits may enclose unchanged content. No source excerpts or raw URLs exported; use bounded read suggestions. Hash-verified local bodies and session URL associations do not prove deployment identity, simultaneous capture, semantic effect or security impact."}),
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
    json!({"type":"function","function":{"name":"source_artifact","description":"Fetch large UTF-8 JavaScript/text once into a bounded local session cache, then read/search without refetching or sending the whole body. Cross-file tracing and discovery are restricted to loopback project sources. bridge_call requires artifact_id, query (callee), offset (exact call start), optional argument_index; links a unique named cached definition to a simple parameter and its scoped flow as a syntactic candidate only. flow requires artifact_id, query and offset (UTF-8 function-scope anchor); it follows bounded same-function identifier assignments and use sites, never semantic taint. discover lists bounded static HTML script or JS import references from artifact_id with current scope/cache status and no requests; fetch_reference re-derives one reference_index from that artifact and requires approval. trace_chain requires artifact_id and query and follows at most 6 cached alias/re-export candidates under a 2 MiB/1-second budget, without downloads. trace with query and optional artifact_id gives bounded AST assignment/call/return candidates and cached import links, never taint or sanitizer proof. fetch reuses the latest local copy; refresh explicitly downloads one new approved version while preserving old hashes. Both network actions require approval; max 8 MiB/source, 32 version records/64 MiB/session. Historical cache is not fresh remote evidence. compare with baseline_artifact_id and artifact_id returns an enclosing changed byte span between stored versions of the same URL, not a semantic diff. read/search use returned SHA256 artifact_id and UTF-8 byte offsets.","parameters":{"type":"object","properties":{"action":{"type":"string","enum":["fetch","refresh","compare","read","search","trace","trace_chain","discover","fetch_reference","flow","bridge_call"]},"url":{"type":"string"},"artifact_id":{"type":"string"},"argument_index":{"type":"integer","minimum":0,"maximum":7},"reference_index":{"type":"integer","minimum":0,"maximum":79},"baseline_artifact_id":{"type":"string"},"offset":{"type":"integer","minimum":0},"count":{"type":"integer","minimum":1,"maximum":16000},"query":{"type":"string"}},"required":["action"],"additionalProperties":false}}})
}
#[cfg(test)]
mod tests {
    #[test]
    fn import_resolution_does_not_invent_package_links() {
        let base = "https://example.test/app/main.js";
        for specifier in ["react", "pkg/path", "#alias", "data:text/javascript,x", ""] {
            assert!(super::resolve_import(base, specifier).is_none());
        }
        assert_eq!(
            super::resolve_import(base, "./clean.js").unwrap().as_str(),
            "https://example.test/app/clean.js"
        );
        assert_eq!(
            super::resolve_import(base, "/clean.js").unwrap().as_str(),
            "https://example.test/clean.js"
        );
    }

    use super::*;
    fn chain_store(sources: &[(&str, &str)]) -> (tempfile::TempDir, Store, Vec<Entry>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path(), "chain");
        fs::create_dir_all(&store.directory).unwrap();
        let entries = sources
            .iter()
            .map(|(url, body)| {
                let id = crate::intelligence::code::hash(body.as_bytes());
                fs::write(store.directory.join(&id), body).unwrap();
                Entry {
                    id,
                    url: (*url).into(),
                    bytes: body.len(),
                    status: 200,
                    content_type: "text/javascript".into(),
                }
            })
            .collect::<Vec<_>>();
        fs::write(
            store.directory.join("index.json"),
            serde_json::to_vec(&entries).unwrap(),
        )
        .unwrap();
        (dir, store, entries)
    }
    #[test]
    fn call_bridge_maps_named_alias_parameter_and_preserves_uncertainty() {
        for property in ["innerHTML", "textContent"] {
            let caller =
                "import {render} from './bridge.js'; const input=location.hash; render(input);";
            let callee=format!("export function show(value){{const normalized=value.trim(); node.{property}=normalized;}}");
            let (_dir, store, e) = chain_store(&[
                ("https://example.test/app.js", caller),
                (
                    "https://example.test/bridge.js",
                    "export {show as render} from './impl.js';",
                ),
                ("https://example.test/impl.js", &callee),
            ]);
            let result = store
                .bridge_call("render", &e[0].id, caller.find("render(input)").unwrap(), 0)
                .unwrap();
            assert_eq!(result["mapping_available"], true);
            assert_eq!(result["argument_symbol"], "input");
            assert_eq!(result["parameter_symbol"], "value");
            assert_eq!(result["callee_symbol"], "show");
            assert_eq!(result["semantic_dataflow_verified"], false);
            assert!(result["callee_flow"]["steps"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["syntax"]["target"] == format!("node.{property}")));
        }
    }
    #[test]
    fn call_bridge_blocks_expression_arguments_shadowing_and_unsupported_scopes() {
        for (caller, needle, reason) in [
            (
                "import {render} from './impl.js'; render(input.trim());",
                "render(input",
                "unsupported_arguments_parameters_or_arity",
            ),
            (
                "import {render} from './impl.js'; render=other; render(input);",
                "render(input",
                "possible_local_shadow_or_reassignment",
            ),
            (
                "import {render} from './impl.js'; function run(render){render(input);}",
                "render(input",
                "non_module_caller_binding_scope_not_supported",
            ),
        ] {
            let (_dir, store, e) = chain_store(&[
                ("https://example.test/app.js", caller),
                (
                    "https://example.test/impl.js",
                    "export function render(value){node.textContent=value;}",
                ),
            ]);
            let result = store
                .bridge_call("render", &e[0].id, caller.find(needle).unwrap(), 0)
                .unwrap();
            assert_eq!(result["mapping_available"], false);
            assert_eq!(result["reason"], reason);
        }
    }
    #[test]
    fn call_bridge_rejects_reassigned_exported_definition() {
        let caller = "import {render} from './bridge.js'; render(input);";
        for update in ["show = replacement;", "show += replacement;", "show++;"] {
            let callee = format!("export function show(value){{node.innerHTML=value;}} {update}");
            let (_dir, store, entries) = chain_store(&[
                ("https://example.test/app.js", caller),
                (
                    "https://example.test/bridge.js",
                    "export {show as render} from './impl.js';",
                ),
                ("https://example.test/impl.js", &callee),
            ]);
            let output = store
                .bridge_call(
                    "render",
                    &entries[0].id,
                    caller.find("render(input)").unwrap(),
                    0,
                )
                .unwrap();
            assert_eq!(output["mapping_available"], false, "{update}: {output}");
            assert_eq!(output["reason"], "possible_import_chain_binding_write");
        }
    }
    #[test]
    fn call_bridge_requires_direct_named_module_export() {
        let caller = "import {render} from './impl.js'; render(input);";
        for callee in [
            "function outer(){function render(value){node.innerHTML=value;}} export const render=0;",
            "function render(value){node.innerHTML=value;}",
            "export default function render(value){node.innerHTML=value;}",
            "function render(value){node.innerHTML=value;} export {render};",
        ] {
            let (_dir, store, e) = chain_store(&[("https://example.test/app.js", caller),("https://example.test/impl.js", callee)]);
            let output = store.bridge_call("render", &e[0].id, caller.find("render(input)").unwrap(), 0).unwrap();
            assert_eq!(output["mapping_available"], false, "{callee}: {output}");
        }
    }
    #[test]
    fn scoped_flow_links_assignments_and_use_sites_without_claiming_validation() {
        for property in ["innerHTML", "textContent"] {
            let body=format!("function run(input) {{ const transformed=clean(input); if(valid(transformed)) node.{property}=transformed; const closure=()=>otherSink(input); }} function unrelated(input){{ wrongSink(input); }}");
            let (_dir, store, e) = chain_store(&[("https://example.test/app.js", &body)]);
            let output = store
                .flow("input", &e[0].id, body.find("const transformed").unwrap())
                .unwrap();
            let steps = output["steps"].as_array().unwrap();
            assert!(steps.iter().any(|s| s["from_symbol"] == "input"
                && s["syntax"]["target"] == "transformed"
                && s["followed_local_identifier"] == true));
            assert!(steps.iter().any(|s| s["from_symbol"] == "transformed"
                && s["syntax"]["target"] == format!("node.{property}")));
            assert!(steps.iter().any(|s| s["syntax"]["target"] == "valid"));
            assert!(!steps.iter().any(|s| matches!(
                s["syntax"]["target"].as_str(),
                Some("otherSink" | "wrongSink" | "closure")
            )));
            assert_eq!(output["semantic_dataflow_verified"], false);
            assert_eq!(output["network_requests"], 0);
        }
    }
    #[test]
    fn scoped_flow_counts_compound_and_update_writes_from_the_parser() {
        for mutation in [
            "staged += suffix;",
            "staged++;",
            "++staged;",
            "staged *= 2;",
        ] {
            let body=format!("function run(input){{let staged=input; {mutation} const alias=staged; node.innerHTML=alias;}}");
            let (_dir, store, e) = chain_store(&[("https://example.test/app.js", &body)]);
            let output = store
                .flow("input", &e[0].id, body.find("let staged").unwrap())
                .unwrap();
            assert_eq!(output["limited"], true, "{mutation}");
            let steps = output["steps"].as_array().unwrap();
            let alias = steps
                .iter()
                .find(|s| s["syntax"]["target"] == "alias")
                .unwrap();
            assert_eq!(alias["possible_competing_writes"], true, "{mutation}");
            assert_eq!(alias["followed_local_identifier"], false, "{mutation}");
            assert!(
                !steps
                    .iter()
                    .any(|s| s["syntax"]["target"] == "node.innerHTML"),
                "{mutation}"
            );
        }
    }
    #[test]
    fn scoped_flow_stops_updates_of_parameters_without_local_declarations() {
        for mutation in ["staged++;", "staged += suffix;"] {
            let body = format!(
                "function run(staged){{{mutation} const alias=staged; node.innerHTML=alias;}}"
            );
            let (_dir, store, e) = chain_store(&[("https://example.test/app.js", &body)]);
            let output = store
                .flow("staged", &e[0].id, body.find(mutation).unwrap())
                .unwrap();
            assert_eq!(output["limited"], true);
            let steps = output["steps"].as_array().unwrap();
            assert!(!steps.iter().any(|s| s["followed_local_identifier"] == true));
            assert!(!steps
                .iter()
                .any(|s| s["syntax"]["target"] == "node.innerHTML"));
        }
    }
    #[test]
    fn scoped_flow_stops_alias_expansion_at_competing_writes() {
        let body="function run(input){let staged=input; staged='constant'; const alias=staged; node.innerHTML=alias;}";
        let (_dir, store, e) = chain_store(&[("https://example.test/app.js", body)]);
        let output = store
            .flow("input", &e[0].id, body.find("let staged").unwrap())
            .unwrap();
        assert_eq!(output["limited"], true);
        let alias = output["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["syntax"]["target"] == "alias")
            .unwrap();
        assert_eq!(alias["possible_competing_writes"], true);
        assert_eq!(alias["followed_local_identifier"], false);
        assert!(!output["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["syntax"]["target"] == "node.innerHTML"));
        assert!(store.flow("input", &e[0].id, body.len()).is_err());
    }
    #[test]
    fn dynamic_discovery_preserves_unresolved_inputs_scope_and_budget() {
        let source = r#"import './static.js'; import('./lazy.js'); import('./lazy.js'); import(name); import(`./${name}.js`); import('./esc\u0061ped.js'); obj.import('./not-a-module.js'); const text="import('./fake.js')"; import('https://outside.test/x.js');"#;
        let (_dir, store, entries) = chain_store(&[("https://example.test/app.js", source)]);
        let root = tempfile::tempdir().unwrap();
        let mut tools =
            crate::tools::ToolBox::new(root.path().into(), crate::tools::ApprovalMode::ReadOnly);
        tools
            .scope
            .command("allow https://example.test/ read")
            .unwrap();
        let output = store.discover(&tools, &entries[0].id, None).unwrap();
        assert_eq!(output["references"].as_array().unwrap().len(), 7);
        assert_eq!(
            output["counts"],
            json!({"unique_missing_in_scope":2,"unique_index_matches":0,"outside_scope":1,"unresolved":3,"duplicates":1})
        );
        assert_eq!(output["limited"], true);
        assert_eq!(
            output["references"][1]["kind"],
            "literal_dynamic_import_candidate"
        );
        assert_eq!(output["references"][3]["target_redacted"], Value::Null);
        assert!(store.discover(&tools, &entries[0].id, Some(3)).is_err());
        assert!(store.discover(&tools, &entries[0].id, Some(6)).is_err());
        assert_eq!(
            store.discover(&tools, &entries[0].id, Some(1)).unwrap()["denied"],
            true
        );
        assert!(!output.to_string().contains("not-a-module"));
        let source = (0..81)
            .map(|n| format!("import('./{n}.js');"))
            .collect::<String>();
        let (_dir, store, entries) = chain_store(&[("https://example.test/app.js", &source)]);
        let output = store.discover(&tools, &entries[0].id, None).unwrap();
        assert_eq!(output["references"].as_array().unwrap().len(), 80);
        assert_eq!(output["limited"], true);
        assert_eq!(output["uncollected_reference_count"], Value::Null);
    }
    #[test]
    fn discovery_tracks_unique_missing_indexed_outside_and_duplicate_sources() {
        let html="<script src='/a.js'></script><script src='/b.js?token=PRIVATE'></script><script src='/b.js?token=PRIVATE'></script><script src='https://outside.test/x.js'></script>";
        let (_dir, store, mut e) = chain_store(&[
            ("https://example.test/page", html),
            ("https://example.test/a.js", "const cached=true;"),
        ]);
        e[0].content_type = "text/html; charset=utf-8".into();
        fs::write(
            store.directory.join("index.json"),
            serde_json::to_vec(&e).unwrap(),
        )
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut tools =
            crate::tools::ToolBox::new(root.path().into(), crate::tools::ApprovalMode::ReadOnly);
        tools
            .scope
            .command("allow https://example.test/ read")
            .unwrap();
        let result = store.discover(&tools, &e[0].id, None).unwrap();
        assert_eq!(
            result["counts"],
            json!({"unique_missing_in_scope":1,"unique_index_matches":1,"outside_scope":1,"unresolved":0,"duplicates":1})
        );
        assert!(!result.to_string().contains("PRIVATE"));
        assert_eq!(result["network_requests"], 0);
        assert_eq!(result["runtime_coverage_complete"], false);
        assert!(store.discover(&tools, &e[0].id, Some(3)).is_err());
        assert!(store.discover(&tools, &e[0].id, Some(2)).is_err());
        let denied = store.discover(&tools, &e[0].id, Some(1)).unwrap();
        assert_eq!(denied["denied"], true);
        let cached = store.discover(&tools, &e[0].id, Some(0)).unwrap();
        assert_eq!(cached["fresh_evidence"], false);
        assert_eq!(cached["reused_local_copy"], true);
    }
    #[test]
    fn cached_chain_follows_reexport_aliases_and_stops_cycles() {
        let (_dir, store, e) = chain_store(&[
            (
                "https://example.test/app.js",
                "import {sanitize as clean} from './bridge.js'; const output=clean(input);",
            ),
            (
                "https://example.test/bridge.js",
                "export {decode as sanitize} from './parse.js';",
            ),
            (
                "https://example.test/parse.js",
                "export function decode(value) { return value.trim(); }",
            ),
        ]);
        let result = store.trace_chain("clean", &e[0].id).unwrap();
        assert_eq!(result["nodes"].as_array().unwrap().len(), 3);
        assert_eq!(result["nodes"][1]["query"], "sanitize");
        assert_eq!(result["nodes"][2]["query"], "decode");
        assert_eq!(result["network_requests"], 0);
        assert_eq!(result["semantic_dataflow_verified"], false);
        fs::write(store.directory.join(&e[2].id), "changed").unwrap();
        assert!(store.trace_chain("clean", &e[0].id).is_err());
        let (_dir, store, e) = chain_store(&[
            ("https://example.test/a.js", "export {x} from './b.js';"),
            ("https://example.test/b.js", "export {x} from './a.js';"),
        ]);
        let result = store.trace_chain("x", &e[0].id).unwrap();
        assert_eq!(result["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(result["edges"][1]["state"], "already_visited_candidate");
    }
    #[test]
    fn chain_propagates_truncated_import_bindings_for_an_omitted_symbol() {
        let names = (0..40).map(|i| format!("name{i}")).collect::<Vec<_>>();
        let source = format!("import {{ {} }} from './target.js';", names.join(","));
        let analysis = crate::source_trace::inspect(&source, "name0").unwrap();
        assert_eq!(analysis["imports"][0]["bindings_limited"], true);
        let kept = analysis["imports"][0]["bindings"].as_array().unwrap();
        let omitted = names
            .iter()
            .find(|name| !kept.iter().any(|b| b["local_or_exported"] == name.as_str()))
            .unwrap();
        let (_dir, store, e) = chain_store(&[
            ("https://example.test/app.js", &source),
            ("https://example.test/target.js", "export const name0=1;"),
        ]);
        let result = store.trace_chain(omitted, &e[0].id).unwrap();
        assert_eq!(result["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(result["edges"].as_array().unwrap().len(), 0);
        assert_eq!(
            result["limited"], true,
            "An omitted binding must not look fully explored"
        );
        assert_eq!(result["nodes"][0]["import_bindings_limited"], true);
        assert!(result["blocked"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["reason"] == "import_binding_collection_incomplete"));
    }
    #[test]
    fn chain_depth_budget_is_explicit_not_silent_completion() {
        let (_dir, store, e) = chain_store(&[
            ("https://example.test/0.js", "export {x} from './1.js';"),
            ("https://example.test/1.js", "export {x} from './2.js';"),
            ("https://example.test/2.js", "export {x} from './3.js';"),
            ("https://example.test/3.js", "export {x} from './4.js';"),
            ("https://example.test/4.js", "export const x=1;"),
        ]);
        let output = store.trace_chain("x", &e[0].id).unwrap();
        assert_eq!(output["nodes"].as_array().unwrap().len(), 4);
        assert_eq!(output["edges"][3]["state"], "depth_limit");
        assert_eq!(output["limited"], true);
    }
    #[test]
    fn chain_reports_missing_ambiguous_and_over_budget_sources() {
        let (_dir, store, e) = chain_store(&[(
            "https://example.test/a.js",
            "import clean from 'package'; clean(input);",
        )]);
        let result = store.trace_chain("clean", &e[0].id).unwrap();
        assert_eq!(result["limited"], true);
        assert_eq!(result["blocked"].as_array().unwrap().len(), 1);
        let (_dir, store, e) = chain_store(&[
            ("https://example.test/a.js", "export const x=1;"),
            ("https://example.test/b.js", "export const x=1;"),
        ]);
        let result = store.trace_chain("x", &e[0].id).unwrap();
        assert_eq!(result["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(
            result["blocked"][0]["reason"],
            "ambiguous_source_URL_association"
        );
        let body = " ".repeat(2097153);
        let (_dir, store, e) = chain_store(&[("https://example.test/large.js", &body)]);
        let result = store.trace_chain("x", &e[0].id).unwrap();
        assert_eq!(result["bytes_scanned"], 0);
        assert_eq!(result["blocked"][0]["reason"], "cumulative_byte_budget");
    }
    #[test]
    fn cached_module_graph_links_aliases_but_not_bare_packages_or_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path(), "graph");
        fs::create_dir_all(&store.directory).unwrap();
        let sources = [
            ("https://example.test/app.js", "import {decode as transform} from './parse.js'; import * as helpers from './parse.js'; import external from 'pkg'; import missing from './missing.js'; const output=transform(input);"),
            ("https://example.test/parse.js", "export function decode(value) { return value.trim(); }"),
            ("https://example.test/pkg", "export default function fake(value) { return value; }"),
        ];
        let entries = sources
            .iter()
            .map(|(url, body)| {
                let id = crate::intelligence::code::hash(body.as_bytes());
                fs::write(store.directory.join(&id), body).unwrap();
                Entry {
                    id,
                    url: (*url).into(),
                    bytes: body.len(),
                    status: 200,
                    content_type: "text/javascript".into(),
                }
            })
            .collect::<Vec<_>>();
        fs::write(
            store.directory.join("index.json"),
            serde_json::to_vec(&entries).unwrap(),
        )
        .unwrap();
        let result = store.trace("transform", None).unwrap();
        let imports = result["sources"][0]["analysis"]["imports"]
            .as_array()
            .unwrap();
        let resolved = imports
            .iter()
            .find(|row| {
                row["specifier"] == "./parse.js"
                    && row["bindings"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|b| b["local_or_exported"] == "transform")
            })
            .unwrap();
        assert_eq!(resolved["cached_target_artifact"], entries[1].id);
        let follow = &resolved["suggested_local_follow_up"][0];
        assert_eq!(follow["query"], "decode");
        let traced = store
            .trace(
                follow["query"].as_str().unwrap(),
                follow["artifact_id"].as_str(),
            )
            .unwrap();
        assert_eq!(traced["sources"].as_array().unwrap().len(), 1);
        assert_eq!(traced["sources"][0]["artifact_id"], entries[1].id);
        assert!(traced["sources"][0]["analysis"]["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|candidate| candidate["target"] == "decode"));
        assert_eq!(traced["sources_outside_selection"], 2);
        assert!(store.trace("decode", Some("unknown")).is_err());
        assert_eq!(resolved["bindings"][0]["local_or_exported"], "transform");
        for name in ["pkg", "./missing.js"] {
            assert!(imports.iter().find(|row| row["specifier"] == name).unwrap()
                ["cached_target_artifact"]
                .is_null());
        }
        assert_eq!(result["network_requests"], 0);
        assert_eq!(result["fresh_evidence"], false);
        for (query, expected) in [
            ("helpers.decode", 1),
            ("helpers.decode.nested", 0),
            ("helpers", 0),
            ("helpersExtra.decode", 0),
        ] {
            let trace = store.trace(query, Some(&entries[0].id)).unwrap();
            let imports = trace["sources"][0]["analysis"]["imports"]
                .as_array()
                .unwrap();
            let follows = imports
                .iter()
                .flat_map(|row| row["suggested_local_follow_up"].as_array().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(follows.len(), expected, "{query}");
            if expected == 1 {
                assert_eq!(follows[0]["query"], "decode");
                assert_eq!(follows[0]["artifact_id"], entries[1].id);
                assert_eq!(follows[0]["semantic_link_verified"], false);
            }
        }
    }
    #[test]
    fn explicit_trace_can_reach_sources_outside_the_first_eight() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path(), "selection");
        fs::create_dir_all(&store.directory).unwrap();
        let mut entries = Vec::new();
        for n in 0..10 {
            let body = format!("export function Handler{n}(value) {{ return value; }}");
            let id = crate::intelligence::code::hash(body.as_bytes());
            fs::write(store.directory.join(&id), &body).unwrap();
            entries.push(Entry {
                id,
                url: format!("https://example.test/{n}.js"),
                bytes: body.len(),
                status: 200,
                content_type: "text/javascript".into(),
            });
        }
        fs::write(
            store.directory.join("index.json"),
            serde_json::to_vec(&entries).unwrap(),
        )
        .unwrap();
        let broad = store.trace("Handler9", None).unwrap();
        assert_eq!(broad["sources_omitted"], 2);
        let targeted = store.trace("Handler9", Some(&entries[9].id)).unwrap();
        assert_eq!(targeted["sources_omitted"], 0);
        assert_eq!(targeted["sources_outside_selection"], 9);
        assert_eq!(
            targeted["sources"][0]["analysis"]["candidates"][0]["target"],
            "Handler9"
        );
        assert_eq!(targeted["network_requests"], 0);
    }
    #[test]
    fn enclosing_span_reconstructs_unicode_insertions_deletions_and_disjoint_changes() {
        let cases = [
            "",
            "a",
            "ab",
            "abc",
            "axc",
            "é",
            "ê",
            "عربي",
            "a😀z",
            "a😁z",
            "first\nlast",
            "First\nlasT",
        ];
        for before in cases {
            for after in cases {
                let (start, a, b) = changed_span(before, after);
                assert!(start <= a && start <= b);
                assert!(before.is_char_boundary(start) && after.is_char_boundary(start));
                assert!(before.is_char_boundary(a) && after.is_char_boundary(b));
                assert_eq!(
                    format!("{}{}{}", &before[..start], &after[start..b], &before[a..]),
                    after
                );
                if before == after {
                    assert_eq!(start, a);
                    assert_eq!(start, b);
                }
            }
        }
    }
    #[test]
    fn version_comparison_is_bounded_and_rejects_unrelated_or_changed_artifacts() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path(), "compare");
        fs::create_dir_all(&store.directory).unwrap();
        let shared = "x".repeat(2 * 1024 * 1024);
        let bodies = [
            format!("{shared}const value='ع';"),
            format!("{shared}const value='ب';"),
            "unrelated".into(),
        ];
        let mut entries = Vec::new();
        for (n, body) in bodies.iter().enumerate() {
            let id = crate::intelligence::code::hash(body.as_bytes());
            fs::write(store.directory.join(&id), body).unwrap();
            entries.push(Entry {
                id,
                url: if n == 2 {
                    "https://example.test/other.js"
                } else {
                    "https://example.test/app.js?key=PRIVATE"
                }
                .into(),
                bytes: body.len(),
                status: 200,
                content_type: "text/javascript".into(),
            });
        }
        fs::write(
            store.directory.join("index.json"),
            serde_json::to_vec(&entries).unwrap(),
        )
        .unwrap();
        let out = store
            .compare_versions(&entries[0].id, &entries[1].id)
            .unwrap();
        assert_eq!(out["content_equal"], false);
        assert_eq!(out["network_requests"], 0);
        assert_eq!(out["fresh_evidence"], false);
        assert!(!out.to_string().contains("PRIVATE"));
        assert!(out.to_string().len() < 2000);
        let suggestion = &out["variant"]["read_suggestion"];
        assert_eq!(store.inspect(suggestion).unwrap()["excerpt"], "ب");
        let same = store
            .compare_versions(&entries[0].id, &entries[0].id)
            .unwrap();
        assert_eq!(same["content_equal"], true);
        assert!(same["variant"]["read_suggestion"].is_null());
        assert!(store
            .compare_versions(&entries[0].id, &entries[2].id)
            .is_err());
        assert!(store.compare_versions("unknown", &entries[1].id).is_err());
        fs::write(store.directory.join(&entries[1].id), "tampered").unwrap();
        assert!(store
            .compare_versions(&entries[0].id, &entries[1].id)
            .is_err());
    }
    #[test]
    fn explicit_refresh_preserves_old_source_and_unchanged_versions_do_not_grow_cache() {
        let root = tempfile::tempdir().unwrap();
        let first = "export const version=1;";
        let second = "export const version=2;";
        let (url, server) = crate::test_support::http(vec![
            ("text/javascript", first.into()),
            ("text/javascript", second.into()),
            ("text/javascript", second.into()),
        ]);
        let (tx, rx) = std::sync::mpsc::channel::<crate::tools::ApprovalRequest>();
        let mut tools =
            crate::tools::ToolBox::new(root.path().into(), crate::tools::ApprovalMode::Tui(tx));
        let consent = std::thread::spawn(move || {
            for _ in 0..3 {
                rx.recv().unwrap().respond_to.send(true).unwrap();
            }
        });
        let store = Store::new(root.path(), "refresh");
        assert!(store.refresh(&tools, &url).is_err());
        let a = store.fetch(&tools, &url).unwrap();
        let b = store.refresh(&tools, &url).unwrap();
        let same = store.refresh(&tools, &url).unwrap();
        consent.join().unwrap();
        assert_eq!(server.join().unwrap().len(), 3);
        assert_eq!(b["previous_artifact_id"], a["artifact_id"]);
        assert_eq!(b["content_changed"], true);
        assert_eq!(same["content_changed"], false);
        assert_eq!(same["fresh_evidence"], true);
        assert_eq!(store.entries().unwrap().len(), 2);
        assert_eq!(
            store.body(a["artifact_id"].as_str().unwrap()).unwrap(),
            first
        );
        assert_eq!(
            store.fetch(&tools, &url).unwrap()["artifact_id"],
            b["artifact_id"]
        );
        let latest = store.trace("version", None).unwrap();
        assert_eq!(latest["sources"].as_array().unwrap().len(), 1);
        assert_eq!(latest["sources"][0]["artifact_id"], b["artifact_id"]);
        assert_eq!(
            store.trace("version", a["artifact_id"].as_str()).unwrap()["sources"][0]["artifact_id"],
            a["artifact_id"]
        );
        tools.scope.command("clear").unwrap();
        assert!(store.refresh(&tools, &url).is_err());
        assert_eq!(store.entries().unwrap().len(), 2);
    }
    #[test]
    fn denied_or_failed_refresh_preserves_current_source() {
        let root = tempfile::tempdir().unwrap();
        let (url, server) = crate::test_support::http_status(vec![
            (200, "text/javascript", "const valid=1;".into()),
            (503, "text/plain", "PRIVATE_ERROR_BODY".into()),
        ]);
        let (tx, rx) = std::sync::mpsc::channel::<crate::tools::ApprovalRequest>();
        let tools =
            crate::tools::ToolBox::new(root.path().into(), crate::tools::ApprovalMode::Tui(tx));
        let consent = std::thread::spawn(move || {
            for choice in [true, false, true] {
                rx.recv().unwrap().respond_to.send(choice).unwrap();
            }
        });
        let store = Store::new(root.path(), "failure");
        let initial = store.fetch(&tools, &url).unwrap();
        assert_eq!(store.refresh(&tools, &url).unwrap()["denied"], true);
        let failed = store.refresh(&tools, &url).unwrap();
        assert_eq!(failed["status"], 503);
        assert_eq!(failed["cache_preserved"], true);
        assert!(!failed.to_string().contains("PRIVATE_ERROR_BODY"));
        assert_eq!(
            store.fetch(&tools, &url).unwrap()["artifact_id"],
            initial["artifact_id"]
        );
        assert_eq!(store.entries().unwrap().len(), 1);
        consent.join().unwrap();
        assert_eq!(server.join().unwrap().len(), 2);
    }
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
