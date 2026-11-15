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
#[derive(Debug)]
pub(super) struct InstallationDenied;
impl std::fmt::Display for InstallationDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Browser package installation declined; no packages installed and no browser action sent")
    }
}
impl std::error::Error for InstallationDenied {}

fn approve_install(tools: &ToolBox) -> Result<()> {
    if !tools.approve(
        "Install official Termux browser packages",
        "No local Chromium browser was found. Run `pkg install -y x11-repo` and `pkg install -y chromium`? This enables the official Termux X11 repository and downloads Chromium plus dependencies, using mobile data and device storage. Nothing will be installed if you decline. Installed packages remain after the browser closes.",
    )? {
        return Err(InstallationDenied.into());
    }
    Ok(())
}
pub fn install(home: &Path, tools: &ToolBox) -> Result<()> {
    if !termux() {
        anyhow::bail!("Install Chrome, Chromium or Edge first, then paste the URL again. No ADB is needed for a locally installed desktop browser.");
    }
    if !tools.allows_url_inspection() {
        anyhow::bail!("Browser preparation denied by read-only mode or cancellation");
    }
    approve_install(tools)?;
    std::fs::create_dir_all(home)?;
    let log_path = home.join("browser-install.log");
    let log = std::fs::File::create(&log_path)?;
    for package in ["x11-repo", "chromium"] {
        tools.progress(format!("Installing official Termux package {package}; details: {}. This may take several minutes.", log_path.display()));
        let mut last_notice = Instant::now();
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
                tools.progress(format!("Termux package {package} installed successfully"));
                break;
            }
            if last_notice.elapsed() >= Duration::from_secs(5) {
                tools.progress(format!(
                    "Still installing {package} ({}s elapsed; {} log bytes). Package output: {}",
                    started.elapsed().as_secs(),
                    log.metadata().map(|m| m.len()).unwrap_or(0),
                    log_path.display()
                ));
                last_notice = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_setup_requires_separate_explicit_consent() {
        let root = tempfile::tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel::<crate::tools::ApprovalRequest>();
        let tools = ToolBox::new(root.path().into(), crate::tools::ApprovalMode::Tui(tx));
        let approval = std::thread::spawn(move || {
            let request = rx.recv().unwrap();
            assert!(request.action.contains("Install"));
            assert!(request.detail.contains("pkg install -y x11-repo"));
            assert!(request.detail.contains("pkg install -y chromium"));
            request.respond_to.send(false).unwrap();
        });
        assert!(approve_install(&tools)
            .unwrap_err()
            .is::<InstallationDenied>());
        approval.join().unwrap();
    }

    #[test]
    fn readonly_denies_package_setup() {
        let root = tempfile::tempdir().unwrap();
        let tools = ToolBox::new(root.path().into(), crate::tools::ApprovalMode::ReadOnly);
        assert!(approve_install(&tools).is_err());
    }
}
