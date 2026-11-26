use super::{Client, Request};
use anyhow::{ensure, Context, Result};
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

fn reply(stream: &mut TcpStream, status: &str, mime: &str, body: &[u8]) -> Result<()> {
    write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'none'\r\n\r\n",body.len())?;
    stream.write_all(body)?;
    Ok(())
}

fn handle(stream: &mut TcpStream, host: &str, token: &str, client: &Client) -> Result<()> {
    let timeout = Some(Duration::from_secs(3));
    stream.set_read_timeout(timeout)?;
    stream.set_write_timeout(timeout)?;
    let mut reader = BufReader::new(&mut *stream);
    let mut raw = Vec::new();
    while !raw.ends_with(b"\r\n\r\n") {
        let limit = 8192 - raw.len();
        ensure!(
            limit > 0
                && reader
                    .by_ref()
                    .take(limit as u64)
                    .read_until(b'\n', &mut raw)?
                    > 0,
            "Invalid headers"
        );
    }
    let mut slots = [httparse::EMPTY_HEADER; 48];
    let mut req = httparse::Request::new(&mut slots);
    ensure!(req.parse(&raw)?.is_complete(), "Invalid request");
    let header = |name: &str| -> Option<&str> {
        let mut found = req
            .headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case(name));
        let value = std::str::from_utf8(found.next()?.value).ok()?;
        found.next().is_none().then_some(value)
    };
    if header("Host") != Some(host) {
        return reply(stream, "403 Forbidden", "text/plain", b"Invalid host");
    }
    let (mime, body) = match (req.method, req.path) {
        (Some("GET"), Some("/")) => ("text/html; charset=utf-8", include_str!("index.html")),
        (Some("GET"), Some("/app.js")) => ("application/javascript", include_str!("app.js")),
        (Some("GET"), Some("/app.css")) => ("text/css", include_str!("app.css")),
        (Some("POST"), Some("/api")) => {
            if header("Origin") != Some(&format!("http://{host}"))
                || header("Authorization") != Some(&format!("Bearer {token}"))
            {
                return reply(
                    stream,
                    "403 Forbidden",
                    "text/plain",
                    b"Open the private URL from Blue Dragon",
                );
            }
            ensure!(
                !req.headers
                    .iter()
                    .any(|h| h.name.eq_ignore_ascii_case("Transfer-Encoding")),
                "Invalid framing"
            );
            let len = header("Content-Length")
                .context("Missing length")?
                .parse::<usize>()?;
            ensure!(len <= 65536, "Request too large");
            let mut bytes = vec![0; len];
            reader.read_exact(&mut bytes)?;
            let result = serde_json::from_slice::<Request>(&bytes)
                .map_err(anyhow::Error::from)
                .and_then(|req| client.api(req));
            let (status, data) = match result {
                Ok(v) => ("200 OK", v),
                Err(e) => ("400 Bad Request", json!({"error":e.to_string()})),
            };
            return reply(
                stream,
                status,
                "application/json",
                &serde_json::to_vec(&data)?,
            );
        }
        _ => return reply(stream, "404 Not Found", "text/plain", b"Not found"),
    };
    reply(stream, "200 OK", mime, body.as_bytes())
}

pub(super) fn serve(client: Client, launch: bool) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let host = listener.local_addr()?.to_string();
    let mut key = [0; 32];
    getrandom::fill(&mut key).map_err(|e| anyhow::anyhow!("{e}"))?;
    use base64::Engine;
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key);
    let url = format!("http://{host}/#{token}");
    println!("Blue Dragon GUI\n{url}\nKeep Blue Dragon running while using this window.");
    if launch {
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = std::process::Command::new("rundll32.exe");
            c.arg("url.dll,FileProtocolHandler");
            c
        } else {
            std::process::Command::new(if cfg!(target_os = "macos") {
                "open"
            } else if cfg!(target_os = "android") || std::env::var_os("TERMUX_VERSION").is_some() {
                "termux-open-url"
            } else {
                "xdg-open"
            })
        };
        let _ = std::thread::spawn(move || cmd.arg(url).status());
    }
    let shared = Arc::new((host, token, client));
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let mut stream = stream?;
        if active.load(Ordering::Relaxed) >= 16 {
            continue;
        }
        active.fetch_add(1, Ordering::Relaxed);
        let count = active.clone();
        let shared = shared.clone();
        std::thread::spawn(move || {
            let (host, token, client) = &*shared;
            let _ = handle(&mut stream, host, token, client);
            count.fetch_sub(1, Ordering::Relaxed);
        });
    }
    Ok(())
}
