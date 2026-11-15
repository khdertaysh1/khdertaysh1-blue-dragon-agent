//! Cross-process request admission and cooldowns, without conversation or key storage.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
const REQUESTS_PER_MINUTE: u64 = 60;

pub struct Coordinator {
    directory: PathBuf,
    identity: String,
    model: String,
}
impl Coordinator {
    pub fn new(home: &Path, url: &str, key: Option<&str>, model: &str) -> Result<Self> {
        let endpoint = reqwest::Url::parse(url)?;
        let identity = crate::intelligence::code::hash(
            format!(
                "{}\n{}",
                endpoint.origin().ascii_serialization(),
                key.unwrap_or("")
            )
            .as_bytes(),
        );
        Ok(Self {
            directory: home.join("provider-budgets"),
            identity,
            model: crate::intelligence::code::hash(model.as_bytes()),
        })
    }
    fn change(&self, update: impl FnOnce(&mut Value, u64) -> Result<()>) -> Result<()> {
        fs::create_dir_all(&self.directory)?;
        let lock_path = self.directory.join(format!("{}.lock", self.identity));
        if fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("Invalid provider admission lock");
        }
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        let mut acquired = false;
        for attempt in 0..20 {
            if lock.try_lock().is_ok() {
                acquired = true;
                break;
            }
            if attempt < 19 {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        if !acquired {
            bail!("Provider admission busy in another session; no request sent");
        }
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        let path = self.directory.join(format!("{}.json", self.identity));
        let mut value = if path.exists() {
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 65536 {
                bail!("Provider budget state invalid; no request sent");
            }
            let mut bytes = Vec::new();
            fs::File::open(&path)?.take(65537).read_to_end(&mut bytes)?;
            if bytes.len() > 65536 {
                bail!("Provider budget state oversized");
            }
            serde_json::from_slice::<Value>(&bytes)
                .context("Provider budget state corrupt; no request sent")?
        } else {
            json!({"schema":1,"window":now,"requests":0,"until":0,"models":{}})
        };
        if value["schema"] != 1
            || !value["models"].is_object()
            || value["models"].as_object().unwrap().len() > 64
            || !value["window"].is_u64()
            || !value["requests"].is_u64()
            || !value["until"].is_u64()
        {
            bail!("Provider budget schema invalid; no request sent");
        }
        if value["models"]
            .as_object()
            .unwrap()
            .values()
            .any(|v| !v["until"].is_u64())
        {
            bail!("Provider model cooldown state invalid; no request sent");
        }
        update(&mut value, now)?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.directory)?;
        temp.write_all(value.to_string().as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(&path).map_err(|e| e.error)?;
        // File lock is released by Drop, including on process exit. Intent is
        // charged before transmission; interrupted requests are never refunded.
        Ok(())
    }
    pub fn admit(&self) -> Result<()> {
        self.change(|state, now| {
            let until = state["until"]
                .as_u64()
                .unwrap()
                .max(state["models"][&self.model]["until"].as_u64().unwrap_or(0));
            if until > now {
                bail!(
                    "Provider cooldown active for {} seconds across sessions; no request sent",
                    until - now
                );
            }
            let start = state["window"].as_u64().unwrap();
            if now < start {
                bail!("Clock moved backwards; provider budget cannot be safely reset");
            }
            if now - start >= 60 {
                state["window"] = json!(now);
                state["requests"] = json!(0);
            }
            let requests = state["requests"].as_u64().unwrap();
            if requests >= REQUESTS_PER_MINUTE {
                bail!("Local shared request budget exhausted (60 per minute); no request sent");
            }
            state["requests"] = json!(requests + 1);
            Ok(())
        })
    }
    pub fn failure(&self, failure: &crate::rotation::ProviderFailure) -> Result<()> {
        if !matches!(failure.status, 429 | 502 | 503)
            && failure.retry_after_source == "program_default"
        {
            return Ok(());
        }
        self.change(|state, now| {
            // Persisted timestamps have second precision. Round a positive wait
            // outwards so truncating the current clock cannot shorten Retry-After.
            let until = now
                .saturating_add(failure.retry_after)
                .saturating_add(u64::from(failure.retry_after > 0));
            if failure.model_specific {
                state["models"]
                    .as_object_mut()
                    .unwrap()
                    .retain(|_, value| value["until"].as_u64().is_some_and(|u| u > now));
                if state["models"].as_object().unwrap().len() >= 64
                    && state["models"].get(&self.model).is_none()
                {
                    bail!("Model cooldown registry full; no rotation is safe");
                }
                let old = state["models"][&self.model]["until"].as_u64().unwrap_or(0);
                state["models"][&self.model] =
                    json!({"until":old.max(until),"source":failure.retry_after_source});
            } else {
                state["until"] = json!(state["until"].as_u64().unwrap().max(until));
                state["source"] = json!(failure.retry_after_source);
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_sessions_share_quota_and_cooldown_without_storing_key() {
        let home = tempfile::tempdir().unwrap();
        let first = Coordinator::new(
            home.path(),
            "https://provider.test/v1/chat",
            Some("PRIVATE_KEY"),
            "model-a",
        )
        .unwrap();
        let second = Coordinator::new(
            home.path(),
            "https://provider.test/v1/chat",
            Some("PRIVATE_KEY"),
            "model-b",
        )
        .unwrap();
        first.admit().unwrap();
        second.admit().unwrap();
        first
            .failure(&crate::rotation::ProviderFailure::from_response(
                429,
                Some(900000),
                "{}",
            ))
            .unwrap();
        assert!(second.admit().unwrap_err().to_string().contains("cooldown"));
        let text =
            fs::read_to_string(first.directory.join(format!("{}.json", first.identity))).unwrap();
        assert!(!text.contains("PRIVATE_KEY"));
        assert!(!text.contains("model-a"));
        let stored: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(stored["requests"], 2);
    }
    #[test]
    fn cross_process_probe() {
        let Ok(home) = std::env::var("BLUE_PROVIDER_COORDINATOR_TEST") else {
            return;
        };
        let coordinator = Coordinator::new(
            Path::new(&home),
            "https://cross-process.test",
            None,
            "other-model",
        )
        .unwrap();
        assert!(coordinator
            .admit()
            .unwrap_err()
            .to_string()
            .contains("cooldown"));
    }
    #[test]
    fn a_separate_process_observes_durable_account_cooldown() {
        let home = tempfile::tempdir().unwrap();
        let coordinator = Coordinator::new(
            home.path(),
            "https://cross-process.test",
            None,
            "first-model",
        )
        .unwrap();
        coordinator.admit().unwrap();
        coordinator
            .failure(&crate::rotation::ProviderFailure::from_response(
                429,
                Some(300),
                "{}",
            ))
            .unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "provider_budget::tests::cross_process_probe"])
            .env("BLUE_PROVIDER_COORDINATOR_TEST", home.path())
            .status()
            .unwrap();
        assert!(status.success());
    }
    #[test]
    fn quotas_survive_new_instances_and_corrupt_state_fails_closed() {
        let home = tempfile::tempdir().unwrap();
        for _ in 0..60 {
            Coordinator::new(home.path(), "https://provider.test", None, "a")
                .unwrap()
                .admit()
                .unwrap();
        }
        let next = Coordinator::new(home.path(), "https://provider.test", None, "b").unwrap();
        assert!(next
            .admit()
            .unwrap_err()
            .to_string()
            .contains("budget exhausted"));
        fs::write(
            next.directory.join(format!("{}.json", next.identity)),
            "broken",
        )
        .unwrap();
        assert!(next.admit().unwrap_err().to_string().contains("corrupt"));
    }
}
