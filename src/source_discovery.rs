//! Conservative bounded HTML source references, not a DOM or loading oracle.
use serde_json::{json, Value};
/// Quoted and unquoted attributes; malformed/duplicate src stays inconclusive.
fn src_attribute(tag: &str) -> (Option<String>, bool) {
    let bytes = tag.as_bytes();
    let mut at = 0;
    let mut src = None;
    let mut src_seen = false;
    let mut limited = false;
    while at < bytes.len() {
        while at < bytes.len() && (bytes[at].is_ascii_whitespace() || bytes[at] == b'/') {
            at += 1;
        }
        let start = at;
        while at < bytes.len()
            && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'-' | b'_' | b':'))
        {
            at += 1;
        }
        if at == start {
            if at < bytes.len() {
                limited = true;
            }
            break;
        }
        let key = &tag[start..at];
        let is_src = key.eq_ignore_ascii_case("src");
        if is_src {
            if src_seen {
                return (None, true);
            }
            src_seen = true;
        }
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if bytes.get(at) != Some(&b'=') {
            limited |= is_src;
            continue;
        }
        at += 1;
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        let value = if matches!(bytes.get(at), Some(b'\'' | b'"')) {
            let quote = bytes[at];
            at += 1;
            let start = at;
            while at < bytes.len() && bytes[at] != quote {
                at += 1;
            }
            if at == bytes.len() {
                limited = true;
                break;
            }
            let value = &tag[start..at];
            at += 1;
            value
        } else {
            let start = at;
            while at < bytes.len() && !bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            &tag[start..at]
        };
        if key.eq_ignore_ascii_case("src") {
            if src.is_some() {
                return (None, true);
            }
            if value.len() > 2000 || value.is_empty() {
                return (None, true);
            }
            // Deliberately support only this common entity; never guess others.
            let decoded = value.replace("&amp;", "&");
            if value.replace("&amp;", "").contains('&') {
                return (None, true);
            }
            src = Some(decoded);
        }
    }
    (src, limited)
}
pub fn html(body: &str) -> Value {
    let lower = body.to_ascii_lowercase();
    let bytes = body.as_bytes();
    let mut at = 0;
    let mut refs = Vec::new();
    let mut limited = false;
    let mut base = false;
    let mut inline = 0;
    let mut incomplete_scripts = 0;
    while let Some(relative) = lower[at..].find('<') {
        let start = at + relative;
        at = start + 1;
        if lower[start..].starts_with("<!--") {
            if let Some(end) = lower[start + 4..].find("-->") {
                at = start + 4 + end + 3;
                continue;
            }
            limited = true;
            break;
        }
        let name_start = at;
        while at < bytes.len()
            && !bytes[at].is_ascii_whitespace()
            && !matches!(bytes[at], b'/' | b'>' | b'<')
        {
            at += 1;
        }
        if name_start == at {
            continue;
        }
        let name = &lower[name_start..at];
        let attributes_start = at;
        let mut quote = None;
        while at < bytes.len() {
            let b = bytes[at];
            if let Some(q) = quote {
                if b == q {
                    quote = None;
                }
            } else if matches!(b, b'\'' | b'"') {
                quote = Some(b);
            } else if b == b'>' {
                break;
            }
            at += 1;
            if at - attributes_start > 4096 {
                limited = true;
                break;
            }
        }
        if at == bytes.len() || at - attributes_start > 4096 {
            limited = true;
            break;
        }
        if name == "base" {
            base = true;
        }
        if name == "script" {
            let (src, partial) = src_attribute(&body[attributes_start..at]);
            limited |= partial;
            if partial {
                incomplete_scripts += 1;
            }
            if let Some(src) = src.filter(|_| !partial) {
                if refs.len() >= 80 {
                    limited = true;
                    break;
                }
                refs.push(json!({"specifier":src,"byte_offset":start,"kind":"static_html_script_candidate"}));
            } else if !partial {
                inline += 1;
            }
        }
        at += 1;
        if matches!(name, "script" | "style" | "textarea" | "title" | "xmp") {
            let closing = format!("</{name}");
            let mut search = at;
            let mut found = None;
            while let Some(offset) = lower[search..].find(&closing) {
                let end = search + offset + closing.len();
                if bytes
                    .get(end)
                    .is_none_or(|b| b.is_ascii_whitespace() || matches!(b, b'>' | b'/'))
                {
                    found = Some(end);
                    break;
                }
                search = end;
            }
            if let Some(end) = found {
                at = end;
            } else {
                limited = true;
                break;
            }
        }
    }
    json!({"references":refs,"limited":limited,"base_element_seen":base,"inline_script_blocks_not_analyzed":inline,"incomplete_script_tags":incomplete_scripts,"runtime_coverage_complete":false})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valueless_duplicate_src_is_not_a_fetch_candidate() {
        let scan = html("<script src src='/studio.js'></script>");
        assert_eq!(scan["references"].as_array().unwrap().len(), 0);
        assert_eq!(scan["limited"], true);
        assert_eq!(scan["incomplete_script_tags"], 1);
    }
    #[test]
    fn custom_element_prefixes_are_not_script_or_base() {
        let scan=html("<base-x href='/elsewhere/'><script-x src='/fake.js'></script-x><script src='/real.js'></script>");
        assert_eq!(scan["references"].as_array().unwrap().len(), 1);
        assert_eq!(scan["references"][0]["specifier"], "/real.js");
        assert_eq!(scan["base_element_seen"], false);
    }
    #[test]
    fn script_candidates_skip_comments_and_raw_text_and_keep_unknowns() {
        let scan = html(
            r#"<!-- <script src="fake.js"></script> --><script>const s='<script src="fake2.js">';</script><textarea><script src="fake3.js"></script></textarea><SCRIPT defer SRC='/real.js?a=1&amp;b=2'></SCRIPT>"#,
        );
        assert_eq!(scan["references"].as_array().unwrap().len(), 1);
        assert_eq!(scan["references"][0]["specifier"], "/real.js?a=1&b=2");
        assert_eq!(scan["limited"], false);
        let duplicate = html("<script src='a.js' src='b.js'></script>");
        assert_eq!(duplicate["limited"], true);
        assert_eq!(duplicate["incomplete_script_tags"], 1);
        assert_eq!(duplicate["inline_script_blocks_not_analyzed"], 0);
        assert_eq!(html("<script src='a&#46;js'></script>")["limited"], true);
        assert_eq!(
            html("<base href='/elsewhere/'><script src='app.js'></script>")["base_element_seen"],
            true
        );
        assert_eq!(
            html(&"<script src='/x.js'></script>".repeat(81))["limited"],
            true
        );
    }
}
