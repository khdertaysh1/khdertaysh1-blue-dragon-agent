use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct Settings {
    pub root: PathBuf,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub sessions_dir: PathBuf,
}

impl Settings {
    pub fn new(root: PathBuf, base_url: String, model: String) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_owned();
        if !(base_url.starts_with("https://") || base_url.starts_with("http://localhost:") || base_url.starts_with("http://127.0.0.1:")) {
            anyhow::bail!("Provider must use HTTPS, or HTTP on localhost only");
        }
        let home = if let Ok(custom) = std::env::var("BLUE_DRAGON_HOME") {
            PathBuf::from(custom)
        } else {
            dirs::data_local_dir().context("Unable to find app-data directory; set BLUE_DRAGON_HOME")?.join("blue-dragon")
        };
        let sessions_dir = home.join("sessions");
        std::fs::create_dir_all(&sessions_dir).with_context(|| format!("Could not create {}", sessions_dir.display()))?;
        Ok(Self {
            root,
            base_url,
            model,
            api_key: std::env::var("BLUE_DRAGON_API_KEY").ok().filter(|x| !x.is_empty())
                .or_else(|| std::env::var("OPENAI_API_KEY").ok().filter(|x| !x.is_empty())),
            sessions_dir,
        })
    }

    pub fn safe_endpoint(&self) -> String {
        let clean = self.base_url.split('?').next().unwrap_or(&self.base_url);
        // Show protocol + host only; do not echo path tokens or query parameters.
        reqwest::Url::parse(clean).ok().and_then(|url| {
            url.host_str().map(|host| format!("{}://{}", url.scheme(), host))
        }).unwrap_or_else(|| "configured".to_owned())
    }
}
