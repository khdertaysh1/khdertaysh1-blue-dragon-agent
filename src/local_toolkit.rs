//! Human-invoked, offline coding utilities shared by the CLI, chat and TUI.
use anyhow::{bail, Context, Result};
use std::{fs, path::Path, process::Command};

macro_rules! module {
    ($name:literal) => {
        ($name, include_bytes!(concat!("../scripts/", $name)).as_slice())
    };
}

pub fn matches(line: &str) -> bool {
    line.split_whitespace().next() == Some("/toolkit")
}

pub fn chat(root: &Path, line: &str) -> Result<String> {
    let args = shlex::split(line.trim_start_matches("/toolkit"))
        .context("Unclosed quote in toolkit arguments")?;
    run(root, &args)
}

pub fn run(root: &Path, args: &[String]) -> Result<String> {
    let modules = tempfile::tempdir()?;
    for (name, source) in [
        module!("local_developer_toolkit.py"),
        module!("local_python_navigation.py"),
        module!("local_review_batch.py"),
        module!("local_toolkit_evidence.py"),
        module!("local_transcript_diagnostics.py"),
    ] {
        fs::write(modules.path().join(name), source)?;
    }

    // Load only the bundled toolkit and Python standard library.
    let bootstrap = "import sys; sys.path.insert(0, sys.argv.pop(1)); import local_developer_toolkit as kit; sys.exit(kit.main())";
    let output = Command::new(if cfg!(windows) { "python" } else { "python3" })
        .args(["-I", "-S", "-B", "-c", bootstrap])
        .arg(modules.path())
        .args(args)
        .current_dir(root)
        .output()
        .context("Could not start the offline Python toolkit")?;
    let stdout = String::from_utf8(output.stdout)?;
    if !output.status.success() {
        bail!("Toolkit {}\n{}{}", output.status, stdout, String::from_utf8_lossy(&output.stderr));
    }
    Ok(stdout)
}
