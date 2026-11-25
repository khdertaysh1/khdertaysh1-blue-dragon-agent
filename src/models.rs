//! Provider presets and explicit, credential-free saved selections.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io::Read, path::Path, time::Duration};

pub const FREE_BASE: &str = "https://api.jarvisclaw.ai/v1";
pub const FREE_MODEL: &str = "auto/free";
pub const KILO_BASE: &str = "https://ai-gateway.kilo.ai/api/v1";
pub const KILO_MODEL: &str = "kilo-auto/free";

pub fn kilo_id(id: &str) -> bool {
    (id == KILO_MODEL || id.ends_with(":free"))
        && id.contains('/')
        && id.len() <= 200
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"/-_.:".contains(&c))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub provider: String,
    pub model: String,
}

impl Selection {
    pub fn new(provider: &str, model: Option<&str>) -> Result<Self> {
        // Migrate the old JarvisClaw free selection without silently changing
        // the destination of an existing user's conversation.
        let provider = match (provider, model) {
            ("free", Some(FREE_MODEL)) => "jarvis",
            ("free" | "blue-free", _) => "kilo",
            (p, _) => p,
        };
        let default = match provider {
            "kilo" => KILO_MODEL,
            "jarvis" => FREE_MODEL,
            "ollama" => "",
            _ => {
                anyhow::bail!("Unknown provider. Use kilo, jarvis or ollama")
            }
        };
        let model = model.unwrap_or(default);
        if model.is_empty() || model.len() > 200 || model.chars().any(char::is_control) {
            anyhow::bail!("Provide a valid model ID (Ollama needs an installed model ID)");
        }
        if provider == "kilo" && !kilo_id(model) {
            anyhow::bail!("Kilo free access only permits kilo-auto/free or catalog :free IDs");
        }
        if provider == "jarvis" && model != FREE_MODEL {
            anyhow::bail!("The keyless free preset only permits auto/free; no paid routing");
        }
        Ok(Self {
            provider: provider.into(),
            model: model.into(),
        })
    }
    pub fn base(&self) -> &'static str {
        match self.provider.as_str() {
            "kilo" => KILO_BASE,
            "jarvis" => FREE_BASE,
            "ollama" => "http://127.0.0.1:11434/v1",
            _ => unreachable!("validated provider"),
        }
    }
}

pub const CLOUD_PROVIDERS: [&str; 2] = ["kilo", "jarvis"];

pub fn fallback_providers(order: &str) -> Result<Vec<(Selection, String)>> {
    let mut routes = Vec::new();
    for name in order.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        anyhow::ensure!(
            CLOUD_PROVIDERS.contains(&name),
            "Unknown cloud fallback provider: {name}"
        );
        let selection = Selection::new(name, None)?;
        let endpoint = selection.base().to_owned();
        if !routes.iter().any(|(_, url)| url == &endpoint) {
            routes.push((selection, endpoint));
        }
    }
    Ok(routes)
}

pub fn cloud_provider(endpoint: &str) -> Option<&'static str> {
    CLOUD_PROVIDERS
        .into_iter()
        .find(|name| Selection::new(name, None).is_ok_and(|selection| selection.base() == endpoint))
}

pub fn load(home: &Path) -> Result<Option<Selection>> {
    let path = home.join("model.json");
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        anyhow::bail!("Saved model selection exceeds limit");
    }
    let saved: Selection =
        serde_json::from_slice(&bytes).context("Invalid saved model selection")?;
    if matches!(
        saved.provider.as_str(),
        "openrouter" | "openai" | "anthropic" | "groq" | "gemini"
    ) {
        anyhow::bail!("Saved provider requires a key and has been removed. Choose blue-dragon model kilo or blue-dragon model jarvis.");
    }
    Ok(Some(Selection::new(&saved.provider, Some(&saved.model))?))
}

pub fn save(home: &Path, selection: &Selection) -> Result<()> {
    Selection::new(&selection.provider, Some(&selection.model))?;
    std::fs::create_dir_all(home)?;
    let mut file = tempfile::NamedTempFile::new_in(home)?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(selection)?)?;
    file.as_file().sync_all()?;
    file.persist(home.join("model.json"))?;
    Ok(())
}

pub fn menu() -> &'static str {
    "Direct keyless providers: /model kilo or /model jarvis. Use /models to see free tool-capable Kilo models. Local: /model ollama INSTALLED_ID. On quota limits choose the destination before conversation context is sent."
}

fn zero(value: Option<&Value>) -> bool {
    // Decimal zero only. Floating point parsing could round tiny paid prices
    // down to zero; missing, negative, exponential and unknown values fail closed.
    match value {
        Some(Value::String(s)) => {
            let parts: Vec<_> = s.split('.').collect();
            parts.len() <= 2
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|c| c == b'0'))
        }
        Some(Value::Number(n)) => n.as_u64() == Some(0),
        _ => false,
    }
}

fn catalog_get(url: &str) -> Result<reqwest::blocking::Response> {
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = format!(
        "catalog-{}-{}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id(),
        SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let start = chrono::Utc::now().timestamp_millis();
    let mut event = serde_json::json!({"dispatch_id":id,"unix_ms":start,"kind":"catalog","provider_origin":reqwest::Url::parse(url)?.origin().ascii_serialization(),"attempts":1,"estimated_message_tokens":0,"estimated_tool_tokens":0,"max_output_tokens":0,"outcome":"dispatch_started_outcome_unknown"});
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    crate::provider_traffic::record(event.clone());
    let response = client.get(url).send();
    event["elapsed_ms"] = serde_json::json!(chrono::Utc::now().timestamp_millis() - start);
    event["outcome"] = serde_json::json!(if response.is_ok() {
        "response_received"
    } else {
        "transport_failure"
    });
    if let Ok(response) = &response {
        event["http_status"] = serde_json::json!(response.status().as_u16());
        event["rate_limit_headers"] = crate::provider_traffic::headers(response.headers());
    }
    crate::provider_traffic::record(event);
    Ok(response?)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelChoice {
    pub id: String,
    pub name: String,
    pub context_length: u64,
}

// Published SWE-bench Verified results, in hundredths of a percent. Unknown
// model identities (including automatic routes) have no comparable score.
fn coding_score(id: &str) -> Option<u16> {
    match id.trim_end_matches(":free") {
        "nvidia/nemotron-3-ultra-550b-a55b" => Some(7070),
        "nvidia/nemotron-3.5-lightning" => Some(5156),
        "liquid/lfm-3-1.7b" => Some(4340),
        _ => None,
    }
}

pub fn rank_choices(choices: &mut [ModelChoice]) {
    choices.sort_by_key(|c| std::cmp::Reverse(coding_score(&c.id)));
}

pub fn rating(id: &str) -> String {
    coding_score(id).map_or_else(
        || "Unranked".into(),
        |n| format!("SWE-bench Verified {:.2}%", f32::from(n) / 100.0),
    )
}

/// Input choices must come from the free/tool-capable catalog filter.
pub fn fallback_ids(choices: &[ModelChoice], required_context: usize) -> Vec<String> {
    choices
        .iter()
        .filter(|c| c.context_length > 0 && c.context_length >= required_context as u64)
        .map(|c| c.id.clone())
        .collect()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedCatalog {
    fetched_at: u64,
    choices: Vec<ModelChoice>,
}

pub fn kilo_choices(value: &Value) -> Vec<ModelChoice> {
    let mut choices = Vec::new();
    for row in value["data"].as_array().into_iter().flatten().take(10_000) {
        let Some(id) = row["id"].as_str().filter(|id| kilo_id(id)) else {
            continue;
        };
        let Some(prices) = row["pricing"].as_object() else {
            continue;
        };
        let tools = row["supported_parameters"]
            .as_array()
            .is_some_and(|p| p.iter().any(|v| v == "tools"));
        if !tools
            || !zero(prices.get("prompt"))
            || !zero(prices.get("completion"))
            || !prices.values().all(|v| zero(Some(v)))
        {
            continue;
        }
        let name = row["name"]
            .as_str()
            .unwrap_or(id)
            .chars()
            .filter(|c| !c.is_control())
            .take(120)
            .collect();
        choices.push(ModelChoice {
            id: id.into(),
            name: if id == KILO_MODEL {
                "Blue Free (gateway automatic)".into()
            } else {
                name
            },
            context_length: row["context_length"].as_u64().unwrap_or(0),
        });
    }
    choices.sort_by(|a, b| (a.id != KILO_MODEL, &a.id).cmp(&(b.id != KILO_MODEL, &b.id)));
    choices.dedup_by(|a, b| a.id == b.id);
    choices.truncate(200);
    choices
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn read_cache(home: &Path) -> Result<Option<CachedCatalog>> {
    let file = match std::fs::File::open(home.join("kilo-models.json")) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(128_001).read_to_end(&mut bytes)?;
    if bytes.len() > 128_000 {
        anyhow::bail!("Cached catalog exceeds limit");
    }
    let cached: CachedCatalog = serde_json::from_slice(&bytes)?;
    if cached.choices.len() > 200
        || cached
            .choices
            .iter()
            .any(|c| !kilo_id(&c.id) || c.name.len() > 500 || c.name.chars().any(char::is_control))
    {
        anyhow::bail!("Invalid cached model catalog");
    }
    Ok(Some(cached))
}
pub fn fallback_catalog_is_recent(home: &Path) -> bool {
    read_cache(home)
        .ok()
        .flatten()
        .is_some_and(|c| c.fetched_at <= now() && now().saturating_sub(c.fetched_at) < 3600)
}
pub fn cached_context_limit(home: &Path, model: &str) -> Option<usize> {
    let cached = read_cache(home).ok().flatten()?;
    cached
        .choices
        .iter()
        .find(|c| c.id == model)
        .and_then(|c| usize::try_from(c.context_length).ok())
        .filter(|limit| *limit >= 8192)
}
fn write_cache(home: &Path, choices: &[ModelChoice]) -> Result<()> {
    std::fs::create_dir_all(home)?;
    let mut file = tempfile::NamedTempFile::new_in(home)?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec(&CachedCatalog {
        fetched_at: now(),
        choices: choices.to_vec(),
    })?)?;
    file.persist(home.join("kilo-models.json"))?;
    Ok(())
}

pub fn kilo_catalog(home: &Path, force: bool) -> Result<(Vec<ModelChoice>, String)> {
    let (mut choices, source) = catalog_at(home, force, KILO_BASE)?;
    rank_choices(&mut choices);
    Ok((choices, source))
}
fn catalog_at(home: &Path, force: bool, base: &str) -> Result<(Vec<ModelChoice>, String)> {
    // One cross-process fetch at a time, followed by a cache recheck. Force
    // refreshes within 30 seconds coalesce rather than amplifying UI requests.
    std::fs::create_dir_all(home)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(home.join("kilo-catalog.lock"))?;
    let started = std::time::Instant::now();
    loop {
        if lock.try_lock().is_ok() {
            break;
        }
        if started.elapsed() > Duration::from_secs(22) {
            anyhow::bail!("Catalog refresh already in progress; no duplicate request sent");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let cached = read_cache(home).ok().flatten();
    if let Some(c) = &cached {
        if c.fetched_at <= now() && now().saturating_sub(c.fetched_at) < 30 {
            return Ok((
                c.choices.clone(),
                "Shared catalog cache; duplicate refresh coalesced (30 seconds)".into(),
            ));
        }
    }
    let attempt = home.join("kilo-catalog-attempt.json");
    let last = std::fs::read(&attempt)
        .ok()
        .filter(|b| b.len() < 128)
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["at"].as_u64())
        .unwrap_or(0);
    if last <= now() && now().saturating_sub(last) < 60 {
        return Ok((
            cached.map(|c| c.choices).unwrap_or_else(|| {
                vec![ModelChoice {
                    id: KILO_MODEL.into(),
                    name: "Auto Free (catalog unavailable)".into(),
                    context_length: 0,
                }]
            }),
            "Recent catalog failure; no duplicate request sent".into(),
        ));
    }
    if !force {
        if let Some(c) = &cached {
            if c.fetched_at <= now() && now().saturating_sub(c.fetched_at) < 3600 {
                return Ok((
                    c.choices.clone(),
                    "Kilo public catalog, cached less than 1 hour; no API key".into(),
                ));
            }
        }
    }
    let mut stamp = tempfile::NamedTempFile::new_in(home)?;
    use std::io::Write;
    stamp.write_all(serde_json::json!({"at":now()}).to_string().as_bytes())?;
    stamp.persist(&attempt)?;
    let fetch = || -> Result<Vec<ModelChoice>> {
        let response = catalog_get(&format!("{base}/models"))?;
        if !response.status().is_success() {
            anyhow::bail!("Catalog HTTP {}", response.status().as_u16());
        }
        let mut bytes = Vec::new();
        response.take(4_000_001).read_to_end(&mut bytes)?;
        if bytes.len() > 4_000_000 {
            anyhow::bail!("Catalog exceeds limit");
        }
        let choices = kilo_choices(&serde_json::from_slice(&bytes)?);
        if choices.is_empty() {
            anyhow::bail!("No zero-priced tool-capable free models in catalog");
        }
        Ok(choices)
    };
    match fetch() {
        Ok(choices) => {
            let saved = write_cache(home, &choices).is_ok();
            Ok((choices, format!("Live Kilo public catalog; no API key. Cache {}. Availability is not guaranteed.", if saved { "saved" } else { "not saved" })))
        }
        Err(_) => {
            let choices = cached.map(|c| c.choices).unwrap_or_else(|| {
                vec![ModelChoice {
                    id: KILO_MODEL.into(),
                    name: "Auto Free".into(),
                    context_length: 0,
                }]
            });
            Ok((choices, "Catalog unavailable; showing cached choices or documented Auto Free only. Use /models refresh to retry. No live availability claim.".into()))
        }
    }
}

pub fn validate_kilo_selection(home: &Path, model: &str) -> Result<()> {
    if model == KILO_MODEL {
        return Ok(());
    }
    if !kilo_id(model) {
        anyhow::bail!("Not a Kilo free model ID");
    }
    let (choices, _) = kilo_catalog(home, false)?;
    if !choices.iter().any(|c| c.id == model) {
        anyhow::bail!("Model not found in the free tool-capable catalog; use /models refresh or kilo-auto/free");
    }
    Ok(())
}

pub fn kilo_listing(home: &Path, force: bool) -> Result<String> {
    let (choices, source) = kilo_catalog(home, force)?;
    Ok(format!("{source}\n{}\nSelect: /model kilo MODEL_ID. Prompts/tool results go to Kilo and upstreams, which may log and use them for improvement; no confidential data.", choices.iter().map(|c| format!("{} | {} | {} | context {}", c.id, c.name, rating(&c.id), c.context_length)).collect::<Vec<_>>().join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn fallback_catalog_rejects_stale_and_future_metadata() {
        let home = tempfile::tempdir().unwrap();
        for stamp in [0, now().saturating_sub(3601), now().saturating_add(60)] {
            std::fs::write(
                home.path().join("kilo-models.json"),
                json!({"fetched_at":stamp,"choices":[]}).to_string(),
            )
            .unwrap();
            assert!(!fallback_catalog_is_recent(home.path()));
        }
    }
    #[test]
    fn fallback_rejects_unknown_and_insufficient_context() {
        let choices = [0, 8192, 32768].map(|size| ModelChoice {
            id: format!("{size}:free"),
            name: "fixture".into(),
            context_length: size,
        });
        assert_eq!(fallback_ids(&choices, 32768), vec!["32768:free"]);
        assert!(fallback_ids(&choices, 65536).is_empty());
    }
    #[test]
    fn concurrent_forced_catalog_refreshes_share_one_response() {
        use std::io::{Read, Write};
        let home = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0u8; 4096];
            let _ = stream.read(&mut bytes).unwrap();
            let body=serde_json::json!({"data":[{"id":"fixture/model:free","name":"Fixture","pricing":{"prompt":"0","completion":"0"},"supported_parameters":["tools"],"context_length":32000}]}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        });
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let workers = (0..4)
            .map(|_| {
                let dir = home.path().to_owned();
                let url = base.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    catalog_at(&dir, true, &url).unwrap()
                })
            })
            .collect::<Vec<_>>();
        let results = workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .collect::<Vec<_>>();
        server.join().unwrap();
        assert!(results.iter().all(|r| r.0[0].id == "fixture/model:free"));
        assert_eq!(
            results.iter().filter(|r| r.1.contains("Live Kilo")).count(),
            1
        );
        assert_eq!(
            results.iter().filter(|r| r.1.contains("coalesced")).count(),
            3
        );
    }
    #[test]
    fn kilo_catalog_rejects_paid_routes_missing_prices_and_non_tools() {
        let row = json!({"id":"vendor/code:free", "name":"Code free", "context_length":32000,
            "pricing":{"prompt":"0.000","completion":"0","discount":0}, "supported_parameters":["tools"]});
        let mut paid = row.clone();
        paid["pricing"]["prompt"] = json!("1e-999");
        let mut missing = row.clone();
        missing["pricing"]["completion"] = Value::Null;
        let mut metered = row.clone();
        metered["id"] = json!("kilo-auto/efficient");
        let mut no_tools = row.clone();
        no_tools["supported_parameters"] = json!([]);
        let mut extra = row.clone();
        extra["pricing"]["request"] = json!("0.001");
        let choices =
            kilo_choices(&json!({"data":[row.clone(),paid,missing,metered,no_tools,extra,row]}));
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].id, "vendor/code:free");
        assert_eq!(choices[0].context_length, 32000);
    }
    #[test]
    fn cache_and_legacy_selection_preserve_explicit_destinations() {
        let home = tempfile::tempdir().unwrap();
        let choices = vec![ModelChoice {
            id: "vendor/code:free".into(),
            name: "Code free".into(),
            context_length: 32000,
        }];
        write_cache(home.path(), &choices).unwrap();
        assert_eq!(
            kilo_catalog(home.path(), false).unwrap().0[0].id,
            "vendor/code:free"
        );
        assert!(validate_kilo_selection(home.path(), "vendor/code:free").is_ok());
        assert!(validate_kilo_selection(home.path(), "vendor/absent:free").is_err());
        assert!(validate_kilo_selection(home.path(), KILO_MODEL).is_ok());
        std::fs::write(
            home.path().join("model.json"),
            r#"{"provider":"free","model":"auto/free"}"#,
        )
        .unwrap();
        assert_eq!(load(home.path()).unwrap().unwrap().provider, "jarvis");
        assert_eq!(Selection::new("free", None).unwrap().provider, "kilo");
    }
    #[test]
    fn cache_bounds_and_control_characters_are_rejected() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("kilo-models.json"), vec![b' '; 128_001]).unwrap();
        assert!(read_cache(home.path()).is_err());
        assert!(!kilo_id("vendor/\u{1b}model:free"));
        assert!(!kilo_id("kilo-auto/frontier"));
        assert!(!kilo_id("kilo-auto/small"));
        assert!(!zero(Some(&json!("NaN"))));
        assert!(!zero(Some(&json!("-0.0"))));
    }
    #[test]
    fn keyless_preset_never_uses_any_environment_key_or_paid_alias() {
        assert!(Selection::new("free", Some("auto/paid")).is_err());
        assert!(Selection::new("openrouter", Some("vendor/paid")).is_err());
        assert!(Selection::new("ollama", None).is_err());
    }
    #[test]
    fn selection_roundtrip_overwrites_and_rejects_tampering() {
        let home = tempfile::tempdir().unwrap();
        let free = Selection::new("free", None).unwrap();
        save(home.path(), &free).unwrap();
        assert_eq!(load(home.path()).unwrap(), Some(free));
        let other = Selection::new("ollama", Some("local-test")).unwrap();
        save(home.path(), &other).unwrap();
        assert_eq!(load(home.path()).unwrap(), Some(other));
        std::fs::write(
            home.path().join("model.json"),
            r#"{"provider":"free","model":"paid"}"#,
        )
        .unwrap();
        assert!(load(home.path()).is_err());
    }
    #[test]
    fn coding_rank_places_comparable_scores_first() {
        let mut choices = [
            "kilo-auto/free",
            "liquid/lfm-3-1.7b:free",
            "nvidia/nemotron-3-ultra-550b-a55b:free",
        ]
        .map(|id| ModelChoice {
            id: id.into(),
            name: id.into(),
            context_length: 100,
        });
        rank_choices(&mut choices);
        assert_eq!(choices[0].id, "nvidia/nemotron-3-ultra-550b-a55b:free");
        assert_eq!(choices[2].id, KILO_MODEL);
    }

    #[test]
    fn explicit_keyed_providers_are_rejected() {
        for name in ["openai", "openrouter", "anthropic", "groq", "gemini"] {
            assert!(Selection::new(name, None).is_err());
            assert!(fallback_providers(name).is_err());
        }
    }
}
