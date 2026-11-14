//! First-use setup uses only the user's official Termux package manager.
use crate::tools::ToolBox;
use anyhow::{Context, Result};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn termux() -> bool {
    cfg!(target_os = "android")
        || std::env::var_os("TERMUX_VERSION").is_some()
        || std::env::var("PREFIX").is_ok_and(|p| {
            p.starts_with("/data/data/com.termux/") || p.starts_with("/data/user/0/com.termux/")
        })
}
pub fn headless() -> bool {
    (cfg!(target_os = "linux") || termux())
        && std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none()
}
pub fn install(home: &Path, tools: &ToolBox) -> Result<()> {
    if !termux() {
        anyhow::bail!("Install Chrome, Chromium or Edge first, then paste the URL again. No ADB is needed for a locally installed desktop browser.");
    }
    if !tools.allows_url_inspection() {
        anyhow::bail!("Browser preparation denied by read-only mode or cancellation");
    }
    std::fs::create_dir_all(home)?;
    let log_path = home.join("browser-install.log");
    let log = std::fs::File::create(&log_path)?;
    for package in ["x11-repo", "chromium"] {
        let mut child = Command::new("pkg")
            .args(["install", "-y", package])
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log.try_clone()?)
            .spawn()
            .context(
                "Termux pkg unavailable; install the official Termux app and its package manager",
            )?;
        let started = Instant::now();
        loop {
            if !tools.allows_url_inspection() || started.elapsed() > Duration::from_secs(900) {
                child.kill().ok();
                child.wait().ok();
                anyhow::bail!(
                    "Browser installation cancelled/timed out; log: {}",
                    log_path.display()
                );
            }
            if let Some(status) = child.try_wait()? {
                if !status.success() {
                    anyhow::bail!("Official Termux package installation failed for {package}; log: {}. Fix the package/mirror error and paste the URL again.", log_path.display());
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}
