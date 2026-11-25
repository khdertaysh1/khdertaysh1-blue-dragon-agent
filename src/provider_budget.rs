//! Cross-process request admission and cooldowns, without conversation or key storage.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_EVENTS: usize = 10000;
// 10,000 events with maximum-width u64 fields plus 64 model cooldowns fit
// below this bound. Read and write the same limit; never persist unreadable state.
const MAX_STATE_BYTES: usize = 1024 * 1024;

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
                if crate::provider_traffic::is_kilo(url) {
                    "kilo-free-gateway".into()
                } else {
                    endpoint.origin().ascii_serialization()
                },
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
            if !meta.is_file()
                || meta.file_type().is_symlink()
                || meta.len() > MAX_STATE_BYTES as u64
            {
                bail!("Provider budget state invalid; no request sent");
            }
            let mut bytes = Vec::new();
            fs::File::open(&path)?
                .take(MAX_STATE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_STATE_BYTES {
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
        let encoded = serde_json::to_vec(&value)?;
        if encoded.len() > MAX_STATE_BYTES {
            bail!("Provider budget state would exceed storage limit; no request sent");
        }
        let mut temp = tempfile::NamedTempFile::new_in(&self.directory)?;
        temp.write_all(&encoded)?;
        temp.as_file().sync_all()?;
        temp.persist(&path).map_err(|e| e.error)?;
        // File lock is released by Drop, including on process exit. Intent is
        // charged before transmission; interrupted requests are never refunded.
        Ok(())
    }
    #[cfg(test)]
    pub fn admit(&self) -> Result<()> {
        self.admit_tokens(0)
    }
    pub fn cooldown(&self) -> Result<Option<(u64, bool)>> {
        let mut result = None;
        self.change(|state, now| {
            let model = &state["models"][&self.model];
            let global_until = state["until"].as_u64().unwrap();
            let model_until = model["until"].as_u64().unwrap_or(0);
            let until = global_until.max(model_until);
            if until > now {
                let quota = (global_until > now && state["http_status"] == 429)
                    || (model_until > now && model["http_status"] == 429);
                result = Some((until, quota));
            }
            Ok(())
        })?;
        Ok(result)
    }
    pub fn admit_tokens(&self, reserved_tokens: u64) -> Result<()> {
        let rpm = crate::provider_traffic::positive_env("BLUE_DRAGON_RPM", MAX_EVENTS as u64)
            .unwrap_or(u64::MAX);
        let tpm = crate::provider_traffic::positive_env("BLUE_DRAGON_TPM", 100_000_000);
        self.admit_limits(reserved_tokens, rpm, tpm)
    }
    fn admit_limits(&self, reserved_tokens: u64, rpm: u64, tpm: Option<u64>) -> Result<()> {
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
            if rpm != u64::MAX || tpm.is_some() {
                admit_window(state, now, reserved_tokens, rpm, tpm)?;
            }
            let requests = state["requests"].as_u64().unwrap();
            if requests >= rpm {
                bail!("Local shared request budget exhausted ({rpm} per minute); no request sent");
            }
            state["requests"] = json!(requests + 1);
            Ok(())
        })
    }
    pub fn failure(&self, failure: &crate::rotation::ProviderFailure) -> Result<()> {
        if failure.retry_after == 0
            || (!matches!(failure.status, 429 | 502 | 503)
                && failure.retry_after_source == "program_default")
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
                if until >= old {
                    state["models"][&self.model] =
                        json!({"until":until,"source":failure.retry_after_source,"http_status":failure.status});
                }
            } else if until >= state["until"].as_u64().unwrap() {
                state["http_status"] = json!(failure.status);
                state["until"] = json!(until);
                state["source"] = json!(failure.retry_after_source);
            }
            Ok(())
        })
    }
}

fn admit_window(
    state: &mut Value,
    now: u64,
    tokens: u64,
    rpm: u64,
    tpm: Option<u64>,
) -> Result<()> {
    if tpm.is_some_and(|limit| tokens > limit) {
        bail!("One request exceeds configured TPM; reduce input/output budget. No request sent");
    }
    if state.get("events").is_none() {
        state["events"] = json!([]);
    }
    let events = state["events"]
        .as_array_mut()
        .context("Invalid admission history")?;
    if events.len() > MAX_EVENTS
        || events
            .iter()
            .any(|e| !e["at"].is_u64() || !e["tokens"].is_u64() || e["at"].as_u64().unwrap() > now)
    {
        bail!("Invalid admission history; no request sent");
    }
    events.retain(|e| now.saturating_sub(e["at"].as_u64().unwrap()) < 60);
    let recent = events
        .iter()
        .filter(|e| now.saturating_sub(e["at"].as_u64().unwrap()) < 60)
        .collect::<Vec<_>>();
    let charged = recent
        .iter()
        .fold(0u64, |n, e| n.saturating_add(e["tokens"].as_u64().unwrap()));
    if recent.len() as u64 >= rpm || tpm.is_some_and(|limit| charged.saturating_add(tokens) > limit)
    {
        let wait = recent
            .first()
            .map_or(60, |e| 60 - now.saturating_sub(e["at"].as_u64().unwrap()));
        bail!("Local rolling-minute budget exhausted; retry after at least {wait}s or reduce input/output. Tokens are conservative reservations, not billed usage. No request sent");
    }
    events.push(json!({"at":now,"tokens":tokens}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rolling_reservations_cross_fixed_minute_boundaries_and_preserve_rejected_state() {
        let mut s = json!({});
        admit_window(&mut s, 59, 600, 10, Some(1000)).unwrap();
        let before = s.clone();
        assert!(admit_window(&mut s, 61, 500, 10, Some(1000)).is_err());
        assert_eq!(s, before);
        admit_window(&mut s, 119, 500, 10, Some(1000)).unwrap();
        assert!(admit_window(&mut s, 120, 1001, 10, Some(1000)).is_err());
    }
    #[test]
    fn kilo_alias_hosts_share_cooldowns_without_default_hourly_or_minute_caps() {
        let home = tempfile::tempdir().unwrap();
        let a = Coordinator::new(
            home.path(),
            "https://api.kilo.ai/api/gateway",
            None,
            "kilo-auto/free",
        )
        .unwrap();
        let b = Coordinator::new(
            home.path(),
            "https://ai-gateway.kilo.ai/api/v1",
            None,
            "x:free",
        )
        .unwrap();
        assert_eq!(a.identity, b.identity);
        a.change(|state, now| {
            state["requests"] = json!(201);
            state["events"] = json!(vec![json!({"at":now,"tokens":1}); 201]);
            Ok(())
        })
        .unwrap();
        b.admit().unwrap();
    }
    #[test]
    fn old_64k_boundary_can_be_crossed_read_again_and_expired() {
        let home = tempfile::tempdir().unwrap();
        let coordinator =
            Coordinator::new(home.path(), "https://provider.test", None, "model").unwrap();
        fs::create_dir_all(&coordinator.directory).unwrap();
        let path = coordinator
            .directory
            .join(format!("{}.json", coordinator.identity));
        let now = chrono::Utc::now().timestamp() as u64;
        let state = json!({"schema":1,"window":now,"requests":0,"until":0,"models":{},"events":vec![json!({"at":now,"tokens":1000});2045]});
        let encoded = serde_json::to_vec(&state).unwrap();
        assert_eq!(encoded.len(), 65518);
        fs::write(&path, encoded).unwrap();
        coordinator.admit_limits(10000, 10000, None).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), 65551);
        coordinator.admit_limits(10000, 10000, None).unwrap();
        let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        state["window"] = json!(now - 61);
        for event in state["events"].as_array_mut().unwrap() {
            event["at"] = json!(now - 61);
        }
        fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
        coordinator.admit_limits(10000, 10000, None).unwrap();
        let restored: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(restored["events"].as_array().unwrap().len(), 1);
        assert_eq!(restored["requests"], 1);
    }
    #[test]
    fn largest_allowed_event_count_remains_readable_and_write_limit_is_atomic() {
        let home = tempfile::tempdir().unwrap();
        let coordinator =
            Coordinator::new(home.path(), "https://provider.test", None, "model").unwrap();
        coordinator
            .change(|state, now| {
                state["events"] = json!(vec![json!({"at":now,"tokens":u64::MAX}); MAX_EVENTS - 1]);
                Ok(())
            })
            .unwrap();
        coordinator
            .admit_limits(u64::MAX, MAX_EVENTS as u64, None)
            .unwrap();
        assert!(coordinator
            .admit_limits(1, MAX_EVENTS as u64, None)
            .unwrap_err()
            .to_string()
            .contains("budget exhausted"));
        let path = coordinator
            .directory
            .join(format!("{}.json", coordinator.identity));
        let before = fs::read(&path).unwrap();
        assert!(before.len() < MAX_STATE_BYTES);
        assert!(coordinator
            .change(|state, _| {
                state["padding"] = json!("x".repeat(MAX_STATE_BYTES));
                Ok(())
            })
            .unwrap_err()
            .to_string()
            .contains("storage limit"));
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::write(&path, vec![b' '; MAX_STATE_BYTES + 1]).unwrap();
        assert!(coordinator
            .admit_limits(1, MAX_EVENTS as u64, None)
            .unwrap_err()
            .to_string()
            .contains("state invalid"));
    }
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
                .admit_limits(0, 60, None)
                .unwrap();
        }
        let next = Coordinator::new(home.path(), "https://provider.test", None, "b").unwrap();
        assert!(next
            .admit_limits(0, 60, None)
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
