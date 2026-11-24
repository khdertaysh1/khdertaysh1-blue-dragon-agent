//! Local user commands. These commands are not advertised to model providers.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

const LIMIT: usize = 64 * 1024;

pub fn matches(line: &str) -> bool {
    matches!(
        line.split_whitespace().next(),
        Some("/clipboard" | "/websearch" | "/desktop" | "/httpflow" | "/swarm")
    )
}

pub fn run(line: &str) -> Result<Value> {
    let (name, args) = line.split_once(' ').unwrap_or((line, ""));
    match name {
        "/websearch" => search(args.trim()),
        "/clipboard" => clipboard(args),
        _ => bail!("Unknown operator command"),
    }
}

/// Open an existing workspace text file with a specific text editor.
pub fn open_editor(path: &std::path::Path) -> Result<Value> {
    ensure!(path.is_file(), "Expected an existing workspace file");
    ensure!(
        std::fs::metadata(path)?.len() <= 2 * 1024 * 1024,
        "File exceeds 2 MiB"
    );
    let contents = std::fs::read_to_string(path).context("Expected UTF-8 source or text")?;
    ensure!(
        !contents.contains('\0'),
        "Binary content cannot be opened as text"
    );
    let editors: &[(&str, &[&str])] = if cfg!(target_os = "windows") {
        &[("notepad.exe", &[])]
    } else if cfg!(target_os = "macos") {
        &[("/usr/bin/open", &["-a", "TextEdit"])]
    } else {
        &[("mousepad", &[]), ("gedit", &[]), ("kate", &[])]
    };
    for (program, flags) in editors {
        match Command::new(program)
            .args(*flags)
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                let _ = std::thread::spawn(move || child.wait());
                return Ok(
                    json!({"editor":program,"launch_requested":true,"window_verified":false}),
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!("No supported text editor found; Linux/Termux:X11 supports mousepad, gedit or kate")
}

fn clipboard(args: &str) -> Result<Value> {
    let value = args.strip_prefix("copy ");
    ensure!(
        args == "paste" || value.is_some(),
        "Use /clipboard copy TEXT or /clipboard paste"
    );
    ensure!(
        value.is_none_or(|s| s.len() <= LIMIT),
        "Clipboard input exceeds 64 KiB"
    );
    let copy = value.is_some();
    let (program, flags): (&str, &[&str]) = if cfg!(target_os = "android") {
        (
            if copy {
                "termux-clipboard-set"
            } else {
                "termux-clipboard-get"
            },
            &[],
        )
    } else if cfg!(target_os = "macos") {
        (if copy { "pbcopy" } else { "pbpaste" }, &[])
    } else if cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        if copy {
            ("wl-copy", &[])
        } else {
            ("wl-paste", &["--no-newline"])
        }
    } else if cfg!(target_os = "linux") {
        (
            "xclip",
            if copy {
                &["-selection", "clipboard", "-in"]
            } else {
                &["-selection", "clipboard", "-out"]
            },
        )
    } else {
        bail!("Clipboard integration is unavailable on this platform");
    };
    let mut child = Command::new(program)
        .args(flags)
        .stdin(if copy { Stdio::piped() } else { Stdio::null() })
        .stdout(if copy { Stdio::null() } else { Stdio::piped() })
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| {
            format!("Unable to launch {program}. On Termux, enable the Termux:API app and package.")
        })?;
    let transfer = (|| -> Result<Vec<u8>> {
        if let Some(text) = value {
            child
                .stdin
                .take()
                .context("Clipboard stdin unavailable")?
                .write_all(text.as_bytes())?;
            Ok(Vec::new())
        } else {
            let mut bytes = Vec::new();
            child
                .stdout
                .take()
                .context("Clipboard stdout unavailable")?
                .take((LIMIT + 1) as u64)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= LIMIT, "Clipboard exceeds 64 KiB");
            Ok(bytes)
        }
    })();
    if transfer.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let bytes = transfer?;
    ensure!(status.success(), "Clipboard backend failed: {program}");
    if copy {
        Ok(json!({"copied_bytes":value.unwrap_or_default().len()}))
    } else {
        Ok(json!({"text":String::from_utf8(bytes).context("Clipboard is not UTF-8 text")?}))
    }
}

fn search(query: &str) -> Result<Value> {
    ensure!(
        !query.is_empty() && query.len() <= 2048,
        "Use /websearch WORDS (up to 2048 bytes)"
    );
    let endpoint = std::env::var("BLUE_DRAGON_SEARCH_URL").context(
        "Set BLUE_DRAGON_SEARCH_URL to a SearXNG /search endpoint with JSON output enabled",
    )?;
    let url = reqwest::Url::parse(&endpoint)?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    ensure!(
        (url.scheme() == "https" || (local && url.scheme() == "http"))
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Expected an HTTPS search endpoint or loopback HTTP"
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client
        .get(url)
        .query(&[("q", query), ("format", "json")])
        .send()?
        .error_for_status()?;
    ensure!(
        response.status().is_success(),
        "Search service did not return success"
    );
    let mut bytes = Vec::new();
    response.take(1_048_577).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1_048_576, "Search response exceeds 1 MiB");
    let data: Value = serde_json::from_slice(&bytes).context("Expected SearXNG JSON results")?;
    let results:Vec<_>=data["results"].as_array().context("Missing search results array")?.iter().filter_map(|r|{
        let url=r["url"].as_str()?;
        let parsed=reqwest::Url::parse(url).ok()?;
        if !matches!(parsed.scheme(),"https"|"http"){return None;}
        Some(json!({"title":r["title"].as_str()?.chars().take(240).collect::<String>(),"url":url,"snippet":r["content"].as_str().unwrap_or("").chars().take(600).collect::<String>()}))
    }).take(10).collect();
    Ok(json!({"results":results}))
}
