//! Bounded reads of live local files. Offsets are bytes, never line numbers.
use anyhow::{bail, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) const WINDOW: usize = 16 * 1024;
const SCAN: u64 = 8 * 1024 * 1024;

fn open(path: &Path) -> Result<(File, Metadata)> {
    // Reject devices/FIFOs before opening, which could otherwise block.
    if !path.metadata()?.is_file() {
        bail!("Not a regular file");
    }
    let file = File::open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        bail!("Not a regular file");
    }
    Ok((file, meta))
}

fn unchanged(file: &File, before: &Metadata) -> Result<()> {
    let after = file.metadata()?;
    if before.len() != after.len() || before.modified()? != after.modified()? {
        bail!("File changed during read; discard this result and restart");
    }
    Ok(())
}

// At most limit + 3 bytes are read. Trailing partial code points are deferred.
fn chunk(file: &mut File, offset: u64, limit: usize, size: u64) -> Result<String> {
    if offset > size {
        bail!("Byte offset exceeds file size");
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::with_capacity(limit + 3);
    file.take((limit + 3) as u64).read_to_end(&mut bytes)?;
    let valid = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(e) if e.error_len().is_none() && offset + (bytes.len() as u64) < size => {
            e.valid_up_to()
        }
        Err(_) => bail!("Window is not UTF-8 or byte offset is inside a code point"),
    };
    let text = std::str::from_utf8(&bytes[..valid])?;
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 && offset < size {
        bail!("Byte count cannot fit the next UTF-8 character");
    }
    let text = &text[..end];
    if text.contains('\0') {
        bail!("NUL byte in text window; binary file unsupported");
    }
    Ok(text.to_owned())
}

/// Bounded full text for existing source parsing; never use metadata as the read cap.
pub(crate) fn source_text(path: &Path, cancelled: &AtomicBool) -> Result<String> {
    if cancelled.load(Ordering::Relaxed) {
        bail!("Read cancelled");
    }
    let (mut file, meta) = open(path)?;
    const CAP: u64 = 128_000;
    if meta.len() > CAP {
        bail!("Source exceeds 128000 bytes; use file windows/search instead");
    }
    let mut bytes = Vec::new();
    (&mut file).take(CAP + 1).read_to_end(&mut bytes)?;
    if bytes.len() > CAP as usize {
        bail!("Source grew beyond parse limit");
    }
    unchanged(&file, &meta)?;
    if cancelled.load(Ordering::Relaxed) {
        bail!("Read cancelled");
    }
    let text = String::from_utf8(bytes)?;
    if text.contains('\0') {
        bail!("Binary source unsupported");
    }
    Ok(text)
}

pub fn read(path: &Path, offset: u64, count: u64, cancelled: &AtomicBool) -> Result<Value> {
    let (mut file, meta) = open(path)?;
    let result = read_open(&mut file, meta.len(), offset, count, cancelled)?;
    unchanged(&file, &meta)?;
    Ok(result)
}

pub(crate) fn read_open(
    file: &mut File,
    size: u64,
    offset: u64,
    count: u64,
    cancelled: &AtomicBool,
) -> Result<Value> {
    if !(1..=WINDOW as u64).contains(&count) {
        bail!("byte_count must be 1..16384");
    }
    if cancelled.load(Ordering::Relaxed) {
        bail!("Read cancelled");
    }
    let text = chunk(file, offset, count as usize, size)?;
    let end = offset + text.len() as u64;
    Ok(
        json!({"content":text,"byte_offset":offset,"end_byte_offset":end,
        "file_bytes":size,"next_byte_offset":if end < size {Some(end)} else {None},
        "eof":end==size,"ends_with_newline":if end==size && (!text.is_empty() || size==0) {Some(text.ends_with('\n'))} else {None},"window_sha256":format!("{:x}",Sha256::digest(text.as_bytes())),
        "consistency":"live file; size/mtime checked during call, not an immutable snapshot",
        "utf8_scope":"returned window only; unseen bytes not validated"}),
    )
}

pub fn search(
    path: &Path,
    query: &str,
    offset: u64,
    budget: u64,
    cap: u64,
    cancelled: &AtomicBool,
) -> Result<Value> {
    let (mut file, meta) = open(path)?;
    let result = search_open(&mut file, meta.len(), query, offset, budget, cap, cancelled)?;
    unchanged(&file, &meta)?;
    Ok(result)
}

fn newline_count(bytes: &[u8]) -> u64 {
    bytes.iter().filter(|&&b| b == b'\n').count() as u64
}

// A nonzero search cursor can start in the middle of any line. Count only
// actual LF bytes from the beginning and keep prefix verification bounded.
fn first_search_line(file: &mut File, offset: u64, cancelled: &AtomicBool) -> Result<Option<u64>> {
    if offset > SCAN {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(0))?;
    let mut remaining = offset;
    let mut line = 1;
    let mut buf = [0; WINDOW];
    while remaining > 0 {
        if cancelled.load(Ordering::Relaxed) {
            bail!("Search cancelled");
        }
        let count = remaining.min(WINDOW as u64) as usize;
        file.read_exact(&mut buf[..count])?;
        line += newline_count(&buf[..count]);
        remaining -= count as u64;
    }
    Ok(Some(line))
}

// Only quote a complete source line. In particular, never label a clipped
// minified line or a suffix from a byte window as the exact line.
fn matching_line(file: &mut File, start: u64, size: u64) -> Result<Option<String>> {
    const LIMIT: usize = 512;
    let lo = start.saturating_sub(LIMIT as u64 + 1);
    let hi = size.min(start.saturating_add(LIMIT as u64 + 2));
    file.seek(SeekFrom::Start(lo))?;
    let mut buf = vec![0; (hi - lo) as usize];
    file.read_exact(&mut buf)?;
    let at = (start - lo) as usize;
    let left = match buf[..at].iter().rposition(|&b| b == b'\n') {
        Some(i) => i + 1,
        None if lo == 0 => 0,
        None => return Ok(None),
    };
    let right = match buf[at..].iter().position(|&b| b == b'\n') {
        Some(i) => at + i,
        None if hi == size => buf.len(),
        None => return Ok(None),
    };
    let bytes = &buf[left..right];
    let bytes = if buf.get(right) == Some(&b'\n') {
        bytes.strip_suffix(b"\r").unwrap_or(bytes)
    } else {
        bytes
    };
    Ok(if bytes.len() <= LIMIT {
        std::str::from_utf8(bytes).ok().map(str::to_owned)
    } else {
        None
    })
}

pub(crate) fn search_open(
    file: &mut File,
    size: u64,
    query: &str,
    offset: u64,
    budget: u64,
    cap: u64,
    cancelled: &AtomicBool,
) -> Result<Value> {
    if query.is_empty() || query.len() > 256 || query.contains('\0') {
        bail!("query must be 1..256 UTF-8 bytes without NUL");
    }
    if !(1024..=SCAN).contains(&budget) {
        bail!("scan_bytes must be 1024..8388608");
    }
    if !(1..=80).contains(&cap) {
        bail!("max_matches must be 1..80");
    }
    if cancelled.load(Ordering::Relaxed) {
        bail!("Search cancelled");
    }
    if offset > size {
        bail!("Byte offset exceeds file size");
    }
    let stop = offset.saturating_add(budget).min(size);
    let mut base_line = first_search_line(file, offset, cancelled)?;
    let mut pos = offset;
    let mut carry = String::new();
    let mut hits = Vec::new();
    let mut last_end = offset;
    let mut next = None;
    let mut reason = "eof";
    while pos < stop {
        if cancelled.load(Ordering::Relaxed) {
            bail!("Search cancelled");
        }
        let count = (stop - pos).min(WINDOW as u64) as usize;
        // The remaining budget can end inside a code point. Resume at its start.
        if count < 4 && stop < size {
            reason = "scan_limit";
            break;
        }
        let text = chunk(file, pos, count, size)?;
        let base = pos - carry.len() as u64;
        let joined = std::mem::take(&mut carry) + &text;
        pos += text.len() as u64;
        let begin = last_end.saturating_sub(base) as usize;
        let mut match_line = base_line;
        let mut counted = 0;
        for (index, _) in joined[begin..].match_indices(query) {
            let at = begin + index;
            if let Some(line) = &mut match_line {
                *line += newline_count(&joined.as_bytes()[counted..at]);
            }
            counted = at;
            let start = base + (begin + index) as u64;
            let end = start + query.len() as u64;
            let line_text = matching_line(file, start, size)?;
            hits.push(
                json!({"byte_offset":start,"end_byte_offset":end,"line":match_line,
                "line_complete":line_text.is_some(),"line_text":line_text}),
            );
            last_end = end;
            if hits.len() == cap as usize {
                next = Some(end);
                reason = "match_limit";
                break;
            }
        }
        if next.is_some() {
            break;
        }
        let mut tail = joined.len().saturating_sub(query.len() - 1);
        while !joined.is_char_boundary(tail) {
            tail -= 1;
        }
        if let Some(line) = &mut base_line {
            *line += newline_count(&joined.as_bytes()[..tail]);
        }
        carry = joined[tail..].to_owned();
    }
    if next.is_none() && pos < size {
        // Re-scan a bounded suffix to find a match crossing the scan boundary.
        next = Some((pos - carry.len() as u64).max(last_end));
        reason = "scan_limit";
    }
    Ok(
        json!({"matches":hits,"byte_offset":offset,"scanned_end_byte_offset":pos,
        "file_bytes":size,"next_byte_offset":next,"stop_reason":reason,
        "search_mode":"literal","regex_supported":false,"complete_from_offset":next.is_none(),"whole_file_searched":offset==0 && next.is_none(),
        "match_semantics":"exact, case-sensitive, non-overlapping UTF-8 matches; line is absolute and 1-based (null for cursors beyond the prefix verification limit); line_text is the complete starting line when available",
        "consistency":"live file; size/mtime checked during call, not an immutable snapshot"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_metadata_is_rejected() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "before").unwrap();
        let (file, meta) = open(temp.path()).unwrap();
        std::fs::write(temp.path(), "changed length").unwrap();
        assert!(unchanged(&file, &meta).is_err());
    }

    #[test]
    fn search_returns_absolute_lines_and_exact_text_after_resume() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let import = "import { formatQuote } from './format.js';";
        let call = "    byId('result').textContent = formatQuote(quote.total);";
        let filler = "// تفاصيل\r\n".repeat(14);
        std::fs::write(temp.path(), format!("{import}\r\n{filler}{call}")).unwrap();
        let cancelled = AtomicBool::new(false);
        let search_at = |offset, cap| {
            search(temp.path(), "formatQuote", offset, 1024, cap, &cancelled).unwrap()
        };
        let both = search_at(0, 80);
        assert_eq!(both["matches"][0]["line"], 1);
        assert_eq!(both["matches"][0]["line_text"], import);
        assert_eq!(both["matches"][1]["line"], 16);
        assert_eq!(both["matches"][1]["line_text"], call);
        let cursor = search_at(0, 1)["next_byte_offset"].as_u64().unwrap();
        assert_eq!(search_at(cursor, 80)["matches"][0], both["matches"][1]);
    }

    #[test]
    fn unicode_search_pages_equal_whole_string_oracle() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let text = "🐉éaaa\r\nababالعربية".repeat(3000);
        std::fs::write(temp.path(), &text).unwrap();
        let cancelled = AtomicBool::new(false);
        for query in ["🐉é", "aaa", "aba", "العربية🐉", "absent", "\r\n"] {
            let expected: Vec<u64> = text.match_indices(query).map(|(i, _)| i as u64).collect();
            for budget in [1024, 16384, 16385] {
                let mut hits = Vec::new();
                let mut offset = 0;
                loop {
                    let result =
                        search(temp.path(), query, offset, budget, 23, &cancelled).unwrap();
                    hits.extend(
                        result["matches"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|h| h["byte_offset"].as_u64().unwrap()),
                    );
                    match result["next_byte_offset"].as_u64() {
                        Some(next) => {
                            assert!(next > offset);
                            offset = next;
                        }
                        None => break,
                    }
                }
                assert_eq!(hits, expected, "query {query} budget {budget}");
            }
        }
    }
}
