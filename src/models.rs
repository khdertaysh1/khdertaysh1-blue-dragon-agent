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
            ("free", _) => "kilo",
            (p, _) => p,
        };
        let default = match provider {
            "kilo" => KILO_MODEL,
            "jarvis" => FREE_MODEL,
            "openrouter" => "openrouter/free",
            "openai" => "gpt-4o-mini",
            "anthropic" => "claude-sonnet-4-5",
            "ollama" => "",
            _ => {
                anyhow::bail!("Unknown provider. Use free/kilo, jarvis, openrouter, openai, anthropic or ollama")
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
        if provider == "openrouter" && model != "openrouter/free" && !model.ends_with(":free") {
            anyhow::bail!("The OpenRouter preset only permits openrouter/free or a :free model");
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
            "openrouter" => "https://openrouter.ai/api/v1",
            "anthropic" => "https://api.anthropic.com/v1",
            "ollama" => "http://127.0.0.1:11434/v1",
            _ => "https://api.openai.com/v1",
        }
    }
    pub fn key(&self) -> Option<String> {
        // Never forward an unrelated provider's key to an anonymous gateway.
        let name = match self.provider.as_str() {
            "openrouter" => "OPENROUTER_API_KEY",
            "openai" => "OPENAI_API_KEY",
            "anthropic" => "ANTHROPIC_API_KEY",
            _ => return None,
        };
        std::env::var(name).ok().filter(|s| !s.is_empty())
    }
    pub fn require_key(&self) -> Result<()> {
        if matches!(
            self.provider.as_str(),
            "openrouter" | "openai" | "anthropic"
        ) && self.key().is_none()
        {
            anyhow::bail!(
                "{} needs its own API key environment variable; use /model free for keyless access",
                self.provider
            );
        }
        Ok(())
    }
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
    "Choose with /model PROVIDER [MODEL_ID] (saved for next launch).\nfree/kilo   kilo-auto/free  Kilo anonymous free models; no key\njarvis      auto/free       JarvisClaw anonymous free gateway; no key\nopenrouter  openrouter/free Free models; OPENROUTER_API_KEY required\nollama      <installed-id>  Local server; no key\nopenai      <model-id>      OPENAI_API_KEY; may incur charges\nanthropic   <model-id>      ANTHROPIC_API_KEY; may incur charges\n/models lists Kilo free choices; /models refresh updates them. /models openrouter lists OpenRouter free metadata (key required to use).\nSwitching sends subsequent conversation context to the selected service. Free availability and quotas are provider-controlled. No silent fallback."
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

pub fn free_catalog(value: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    for row in value["data"].as_array().into_iter().flatten().take(10_000) {
        let Some(id) = row["id"].as_str() else {
            continue;
        };
        let tool_capable = row["supported_parameters"]
            .as_array()
            .is_some_and(|p| p.iter().any(|v| v == "tools"));
        let Some(pricing) = row["pricing"].as_object() else {
            continue;
        };
        if !zero(pricing.get("prompt"))
            || !zero(pricing.get("completion"))
            || !pricing.values().all(|v| zero(Some(v)))
            || !tool_capable
            || Selection::new("openrouter", Some(id)).is_err()
        {
            continue;
        }
        ids.push(id.to_owned());
    }
    ids.sort();
    ids.dedup();
    ids.truncate(200);
    ids
}

pub fn refresh() -> Result<String> {
    // Public metadata only; no credentials or conversation are sent.
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?
        .get("https://openrouter.ai/api/v1/models")
        .send()?;
    if !response.status().is_success() {
        anyhow::bail!("Catalog returned HTTP {}", response.status().as_u16());
    }
    let mut bytes = Vec::new();
    response.take(4_000_001).read_to_end(&mut bytes)?;
    if bytes.len() > 4_000_000 {
        anyhow::bail!("Model catalog exceeds limit");
    }
    let ids = free_catalog(&serde_json::from_slice(&bytes)?);
    Ok(format!("OpenRouter zero-priced, tool-capable catalog (not an availability test; key required):\n{}", ids.join("\n")))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelChoice {
    pub id: String,
    pub name: String,
    pub context_length: u64,
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
            name,
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
    let cached = read_cache(home).ok().flatten();
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
    let fetch = || -> Result<Vec<ModelChoice>> {
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()?
            .get(format!("{KILO_BASE}/models"))
            .send()?;
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
    Ok(format!("{source}\n{}\nSelect: /model kilo MODEL_ID. Prompts/tool results go to Kilo and upstreams, which may log and use them for improvement; no confidential data.", choices.iter().map(|c| format!("{} | {} | context {}", c.id, c.name, c.context_length)).collect::<Vec<_>>().join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
        assert_eq!(Selection::new("kilo", None).unwrap().key(), None);
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
        assert_eq!(Selection::new("free", None).unwrap().key(), None);
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
    fn catalog_requires_all_known_prices_zero_and_tools() {
        let row = json!({"id":"vendor/model:free","pricing":{"prompt":"0","completion":"0"},"supported_parameters":["tools"]});
        let mut paid = row.clone();
        paid["pricing"]["request"] = json!("0.1");
        let mut unknown = row.clone();
        unknown["pricing"]["completion"] = Value::Null;
        let mut no_tools = row.clone();
        no_tools["supported_parameters"] = json!([]);
        let mut named_only = row.clone();
        named_only["id"] = json!("vendor/paid");
        assert_eq!(
            free_catalog(&json!({"data":[row.clone(),paid,unknown,no_tools,named_only,row]})),
            vec!["vendor/model:free"]
        );
    }
}
