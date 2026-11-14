use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct Settings {
    pub root: PathBuf,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub sessions_dir: PathBuf,
    pub mcp_config: Option<PathBuf>,
    pub stream: bool,
    pub free_rotation: bool,
    pub max_steps: usize,
}

impl Settings {
    pub fn new(root: PathBuf, base_url: String, model: String) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_owned();
        let parsed = reqwest::Url::parse(&base_url).context("Invalid provider URL")?;
        let local = matches!(
            parsed.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("[::1]")
        );
        if !(parsed.scheme() == "https" || (parsed.scheme() == "http" && local)) {
            anyhow::bail!("Provider must use HTTPS, or HTTP on localhost only");
        }
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            anyhow::bail!(
                "Provider URL must not include credentials, query parameters or fragments"
            );
        }
        let home = app_home()?;
        let sessions_dir = home.join("sessions");
        std::fs::create_dir_all(&sessions_dir)
            .with_context(|| format!("Could not create {}", sessions_dir.display()))?;
        Ok(Self {
            root,
            base_url,
            model,
            api_key: std::env::var("BLUE_DRAGON_API_KEY")
                .ok()
                .filter(|x| !x.is_empty())
                .or_else(|| {
                    let key_name = if parsed.host_str() == Some("api.anthropic.com") {
                        "ANTHROPIC_API_KEY"
                    } else {
                        "OPENAI_API_KEY"
                    };
                    std::env::var(key_name).ok().filter(|x| !x.is_empty())
                }),
            sessions_dir,
            mcp_config: None,
            stream: true,
            free_rotation: true,
            max_steps: 24,
        })
    }

    pub fn safe_endpoint(&self) -> String {
        let clean = self.base_url.split('?').next().unwrap_or(&self.base_url);
        // Show protocol + host only; do not echo path tokens or query parameters.
        reqwest::Url::parse(clean)
            .ok()
            .and_then(|url| {
                url.host_str()
                    .map(|host| format!("{}://{}", url.scheme(), host))
            })
            .unwrap_or_else(|| "configured".to_owned())
    }
}

/// Application data is separate from the inspected project.
pub fn app_home() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("BLUE_DRAGON_HOME") {
        Ok(PathBuf::from(custom))
    } else {
        Ok(dirs::data_local_dir()
            .context("Unable to find app-data directory; set BLUE_DRAGON_HOME")?
            .join("blue-dragon"))
    }
}
