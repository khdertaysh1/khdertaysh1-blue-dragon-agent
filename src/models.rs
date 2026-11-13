//! Provider presets and explicit, credential-free saved selections.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io::Read, path::Path, time::Duration};

pub const FREE_BASE: &str = "https://api.jarvisclaw.ai/v1";
pub const FREE_MODEL: &str = "auto/free";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub provider: String,
    pub model: String,
}

impl Selection {
    pub fn new(provider: &str, model: Option<&str>) -> Result<Self> {
        let default = match provider {
            "free" => FREE_MODEL,
            "openrouter" => "openrouter/free",
            "openai" => "gpt-4o-mini",
            "anthropic" => "claude-sonnet-4-5",
            "ollama" => "",
            _ => {
                anyhow::bail!("Unknown provider. Use free, openrouter, openai, anthropic or ollama")
            }
        };
        let model = model.unwrap_or(default);
        if model.is_empty() || model.len() > 200 || model.chars().any(char::is_control) {
            anyhow::bail!("Provide a valid model ID (Ollama needs an installed model ID)");
        }
        if provider == "free" && model != FREE_MODEL {
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
            "free" => FREE_BASE,
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
    "Choose with /model PROVIDER [MODEL_ID] (saved for next launch).\nfree        auto/free       JarvisClaw anonymous free gateway; no key\nopenrouter  openrouter/free Free models; OPENROUTER_API_KEY required\nollama      <installed-id>  Local server; no key\nopenai      <model-id>      OPENAI_API_KEY; may incur charges\nanthropic   <model-id>      ANTHROPIC_API_KEY; may incur charges\n/models refresh fetches OpenRouter's zero-priced tool-capable catalog.\nSwitching sends subsequent conversation context to the selected service. Free availability and quotas are provider-controlled. No silent fallback."
}

fn zero(value: Option<&Value>) -> bool {
    let number = match value {
        Some(Value::String(s)) => s.parse::<f64>().ok(),
        Some(Value::Number(n)) => n.as_f64(),
        _ => None,
    };
    number == Some(0.0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
