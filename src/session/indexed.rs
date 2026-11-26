//! Local, on-demand search of saved Blue Dragon sessions.
//! Query tokenization, coverage threshold and relevance formula adapted from
//! AlphaCode v1.1.20 (MIT, see third_party/AlphaCode-LICENSE).
use super::semantic;
use super::{read_session, read_session_bounded, validate_id, Session, MAX_SESSION_BYTES};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::Path;

fn tokenize_session_search_query(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();

    for token in query.split(|c: char| !c.is_alphanumeric()) {
        if token.is_empty() {
            continue;
        }

        let token = token.to_lowercase();
        if is_session_search_stop_word(&token) {
            continue;
        }

        let keep = token.chars().count() >= 2 || token.chars().all(|c| c.is_ascii_digit());
        if keep && seen.insert(token.clone()) {
            terms.push(token);
        }
    }

    terms
}

fn is_session_search_stop_word(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "but"
            | "by"
            | "for"
            | "from"
            | "how"
            | "i"
            | "in"
            | "into"
            | "is"
            | "it"
            | "my"
            | "of"
            | "on"
            | "or"
            | "our"
            | "that"
            | "the"
            | "their"
            | "this"
            | "to"
            | "we"
            | "what"
            | "when"
            | "where"
            | "which"
            | "with"
            | "you"
            | "your"
    )
}

fn minimum_session_search_term_matches(term_count: usize) -> usize {
    match term_count {
        0 => 0,
        1 => 1,
        2 => 2,
        3..=5 => 2,
        _ => 3,
    }
}

// Index byte trigrams so a substring such as "quant" can find "quantity".
// Collisions only add candidates. Short terms always pass the filter.
const WORDS: usize = 128;
const CACHE: &str = ".session-search-v2.cache";
const CACHE_LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Entry {
    modified: std::time::SystemTime,
    size: u64,
    bits: Vec<u64>,
    concepts: u64,
}

fn bit(bytes: &[u8]) -> (usize, u64) {
    let hash = bytes.iter().fold(2166136261u32, |h, b| {
        (h ^ u32::from(*b)).wrapping_mul(16777619)
    });
    ((hash as usize >> 6) % WORDS, 1 << (hash & 63))
}

fn candidates(bits: &[u64], terms: &[String], minimum: usize) -> bool {
    terms
        .iter()
        .filter(|term| {
            term.as_bytes().windows(3).all(|gram| {
                let (word, mask) = bit(gram);
                bits[word] & mask != 0
            })
        })
        .take(minimum)
        .count()
        == minimum
}

type Index = BTreeMap<String, Entry>;

fn load_index(path: &Path) -> Option<Index> {
    let data = read_session_bounded(path, CACHE_LIMIT).ok()?;
    let (digest, body) = data.split_at_checked(64)?;
    if crate::intelligence::code::hash(body).as_bytes() != digest {
        return None;
    }
    let index: Index = serde_json::from_slice(body).ok()?;
    index
        .values()
        .all(|entry| entry.bits.len() == WORDS)
        .then_some(index)
}

fn save_index(dir: &Path, index: &Index) -> Result<()> {
    let mut writer = super::BoundedWriter {
        inner: Vec::new(),
        remaining: CACHE_LIMIT - 64,
    };
    serde_json::to_writer(&mut writer, index)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(crate::intelligence::code::hash(&writer.inner).as_bytes())?;
    file.write_all(&writer.inner)?;
    file.persist(dir.join(CACHE))?;
    Ok(())
}

struct Query {
    phrase: String,
    terms: Vec<String>,
    minimum: usize,
}

impl Query {
    fn score(&self, text: &str) -> Option<(f64, usize)> {
        let exact = text.find(&self.phrase);
        let mut count = 0;
        let mut hits = 0;
        let mut first = usize::MAX;
        for term in &self.terms {
            if let Some(pos) = text.find(term) {
                first = first.min(pos);
                count += 1;
                hits += text.matches(term).count();
            }
        }
        if exact.is_none() && count < self.minimum {
            return None;
        }
        let score = if exact.is_some() { 4.0 } else { 0.0 }
            + 3.0 * count as f64 / self.terms.len() as f64
            + 0.25 * count as f64
            + 200.0 * hits as f64 / (text.len() + 1) as f64;
        Some((score, exact.unwrap_or(first)))
    }
}

fn excerpt(text: &str, position: usize) -> String {
    let index = text
        .chars()
        .scan(0, |n, c| {
            let start = *n;
            *n += c.to_lowercase().map(char::len_utf8).sum::<usize>();
            Some(start)
        })
        .take_while(|n| *n < position)
        .count();
    text.chars()
        .skip(index.saturating_sub(36))
        .take(160)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn visit(session: &Session, mut row: impl FnMut(&str, Value)) {
    row(&session.workspace, json!({"role":"workspace"}));
    row(&session.id, json!({"role":"session"}));
    let turns: HashSet<_> = session.user_turns.iter().copied().collect();
    for (index, message) in session.messages.iter().enumerate().rev() {
        let Some(text) = message["content"].as_str() else {
            continue;
        };
        match message["role"].as_str() {
            Some("assistant") => (),
            Some("user") if (turns.is_empty() || turns.contains(&index))
                && !text.starts_with("The user requested inspection of the URL above. Blue Dragon automatically collected") => (),
            _ => continue,
        }
        // A retrieval answer refers to a different session; do not rank the lookup itself.
        if text.starts_with("Historical session references only:")
            || (message["role"] == "user" && semantic::past_question(text))
        {
            continue;
        }
        row(text, json!({"role":message["role"],"message_index":index}));
    }
    for obs in
        session.observations.iter().rev().filter(|o| {
            o.tool == "browser" && o.request_sha256.is_some() && o.result_sha256.is_some()
        })
    {
        let Ok((_, args, result)) = crate::audit::request_for(session, &obs.id) else {
            continue;
        };
        let Some(action) = args["action"].as_str() else {
            continue;
        };
        let count = semantic::observed_count(&args, &result);
        let selector = count
            .as_deref()
            .or_else(|| args["selector"].as_str())
            .unwrap_or("");
        let outcome = match result["execution_status"].as_str() {
            Some("unknown") => "unknown",
            Some("not_executed") => "not_executed",
            _ => match result["ok"].as_bool() {
                Some(true) => "ok",
                Some(false) => "failed",
                None => "unknown",
            },
        };
        row(
            &format!("browser {action} {selector} {outcome} {}", obs.id),
            json!({"role":"browser","message_index":obs.message_index,"evidence_id":obs.id,"outcome":outcome}),
        );
    }
}

pub fn search(dir: &Path, words: &str) -> Result<Value> {
    let phrase = words.trim().to_lowercase();
    anyhow::ensure!(phrase.len() <= 4096, "Search query exceeds 4096 bytes");
    let terms = tokenize_session_search_query(&phrase);
    anyhow::ensure!(!terms.is_empty(), "Enter a searchable word or number");
    let query = Query {
        minimum: minimum_session_search_term_matches(terms.len()),
        terms,
        phrase,
    };
    let wanted = semantic::features(&query.phrase);
    let cache = dir.join(CACHE);
    let mut previous = load_index(&cache).unwrap_or_default();
    let mut next = Index::new();
    let mut records = Vec::new();
    let mut rebuilt = 0;
    let mut unreadable = 0;
    for item in fs::read_dir(dir)? {
        let path = item?.path();
        let Some(id) = path.file_stem().and_then(|x| x.to_str()) else {
            continue;
        };
        if path.extension().and_then(|x| x.to_str()) != Some("json") || validate_id(id).is_err() {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            unreadable += 1;
            continue;
        };
        if !meta.is_file() || meta.len() > MAX_SESSION_BYTES {
            unreadable += 1;
            continue;
        }
        let modified = meta.modified().ok();
        let entry = previous
            .remove(id)
            .filter(|e| Some(e.modified) == modified && e.size == meta.len());
        if let Some(ref e) = entry {
            if !candidates(&e.bits, &query.terms, query.minimum)
                && semantic::similarity(wanted, e.concepts).is_none()
            {
                next.insert(id.to_owned(), entry.unwrap());
                continue;
            }
        }
        let session =
            read_session(&path).and_then(|data| Ok(serde_json::from_slice::<Session>(&data)?));
        let session = match session {
            Ok(s)
                if s.id == id && s.task_checks.checks.len() == s.task_checks.observations.len() =>
            {
                s
            }
            _ => {
                unreadable += 1;
                continue;
            }
        };
        let mut bits = entry.is_none().then(|| vec![0; WORDS]);
        let mut concepts = 0;
        let mut best: Option<(f64, Value)> = None;
        let mut evidence: Option<(f64, Value)> = None;
        visit(&session, |text, mut row| {
            let lower = text.to_lowercase();
            if let Some(bits) = bits.as_mut() {
                for gram in lower.as_bytes().windows(3) {
                    let (word, mask) = bit(gram);
                    bits[word] |= mask;
                }
            }
            let row_concepts = semantic::features(&lower);
            concepts |= row_concepts;
            let lexical = query.score(&lower);
            let related = semantic::similarity(wanted, row_concepts);
            if lexical.is_some() || related.is_some() {
                let (lexical_score, pos) = lexical.unwrap_or_else(|| {
                    let pos = semantic::first_hit(&lower, wanted);
                    (0.0, pos)
                });
                let score = lexical_score + related.unwrap_or(0.0);
                row["match_kind"] = json!(if lexical.is_some() {
                    "text"
                } else {
                    "concepts"
                });
                row["excerpt"] = json!(excerpt(text, pos));
                if row.get("evidence_id").is_some()
                    && evidence.as_ref().is_none_or(|(old, _)| score > *old)
                {
                    evidence = Some((score, row.clone()));
                }
                if best.as_ref().is_none_or(|(old, _)| score > *old) {
                    best = Some((score, row));
                }
            }
        });
        if let Some((score, hit)) = best {
            records.push((score,session.updated_at,session.id.clone(),json!({"id":id,"workspace":session.workspace,"updated_at":session.updated_at,"match":hit,"historical":true,"evidence":evidence.map(|x|x.1)})));
        }
        if let Some(e) = entry {
            next.insert(id.to_owned(), e);
        } else if let (Some(bits), Some(modified)) = (bits, modified) {
            rebuilt += 1;
            next.insert(
                id.to_owned(),
                Entry {
                    size: meta.len(),
                    modified,
                    bits,
                    concepts,
                },
            );
        }
    }
    let dirty = rebuilt > 0 || !previous.is_empty();
    let persistent = !dirty || save_index(dir, &next).is_ok();
    records.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    Ok(
        json!({"sessions":records.into_iter().map(|r|r.3).collect::<Vec<_>>(),"reindexed":rebuilt,"unreadable":unreadable,"cache_persisted":persistent}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path, ok: bool) -> Session {
        let mut s = super::super::new(root, "test");
        let args = json!({"action":"fill","selector":"#quantity","value":"never-index-this"});
        let result = json!({"ok":ok,"private":"private-result-token"});
        s.messages = vec![
            json!({"role":"user","content":"فحص الحقل"}),
            json!({"role":"assistant","tool_calls":[{"id":"a","function":{"name":"browser","arguments":args.to_string()}}]}),
            json!({"role":"tool","tool_call_id":"a","content":result.to_string()}),
        ];
        let mut observation = crate::reasoning::observe(2, "browser", &result);
        observation.request_sha256 =
            Some(crate::intelligence::code::hash(args.to_string().as_bytes()));
        s.observations.push(observation);
        s
    }

    #[test]
    fn verifies_session_scoped_evidence_and_refreshes_only_changes() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        let mut first = fixture(dir, true);
        let mut second = fixture(dir, false);
        for s in [&mut first, &mut second] {
            super::super::save(dir, s).unwrap();
        }
        let results = search(dir, "#quantity").unwrap();
        assert_eq!(results["reindexed"], 2);
        let rows = results["sessions"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for row in rows {
            assert_eq!(row["evidence"]["evidence_id"], "E000002");
            assert_eq!(
                row["evidence"]["outcome"],
                if row["id"] == first.id {
                    "ok"
                } else {
                    "failed"
                }
            );
        }
        let warm = search(dir, "QUANT").unwrap();
        assert_eq!(warm["reindexed"], 0);
        assert_eq!(warm["sessions"].as_array().unwrap().len(), 2);
        assert_eq!(
            search(dir, "الحقل").unwrap()["sessions"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        for query in ["private-result-token", "never-index-this"] {
            assert!(search(dir, query).unwrap()["sessions"]
                .as_array()
                .unwrap()
                .is_empty());
        }
        second.messages[2]["content"] = json!(r#"{"ok":true}"#);
        second
            .messages
            .push(json!({"role":"assistant","content":"new searchable marker"}));
        super::super::save(dir, &mut second).unwrap();
        let changed = search(dir, "quantity").unwrap();
        assert_eq!(changed["reindexed"], 1);
        assert_eq!(changed["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(changed["sessions"][0]["id"], first.id);
        assert_eq!(
            search(dir, "searchable").unwrap()["sessions"][0]["id"],
            second.id
        );
        fs::remove_file(dir.join(format!("{}.json", first.id))).unwrap();
        assert!(search(dir, "quantity").unwrap()["sessions"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cross_language_lookup_selects_verified_count_and_keeps_live_questions_separate() {
        let tmp = tempfile::tempdir().unwrap();
        let mut session = fixture(tmp.path(), true);
        let args =
            json!({"action":"evaluate","expression":"document.querySelector('#quantity').value"});
        let result = json!({"ok":true,"result":"1"});
        session.messages[1]["tool_calls"][0]["function"]["arguments"] = json!(args.to_string());
        session.messages[2]["content"] = json!(result.to_string());
        let mut obs = crate::reasoning::observe(2, "browser", &result);
        obs.request_sha256 = Some(crate::intelligence::code::hash(args.to_string().as_bytes()));
        session.observations = vec![obs];
        super::super::save(tmp.path(), &mut session).unwrap();
        for prompt in [
            "بأي جلسة ضلّ العدد واحد بعد ما حاولنا نغيّره؟",
            "Which session had the count stuck at one?",
        ] {
            assert!(semantic::past_question(prompt));
            let found = search(tmp.path(), prompt).unwrap();
            let row = &found["sessions"][0];
            assert_eq!(row["id"], session.id);
            assert_eq!(row["historical"], true);
            assert_eq!(row["evidence"]["evidence_id"], "E000002");
            assert!(row["evidence"]["excerpt"]
                .as_str()
                .unwrap()
                .contains("#quantity value 1"));
        }
        assert!(!semantic::past_question("وشو قيمته هلّق؟"));
        assert!(
            search(tmp.path(), "بأي جلسة ضل العدد ثلاثة؟").unwrap()["sessions"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let unrelated =
            json!({"action":"evaluate","expression":"document.querySelector('#pin').value"});
        assert!(semantic::observed_count(&unrelated, &result).is_none());
        session.messages[2]["content"] = json!(r#"{"ok":true,"result":"2"}"#);
        super::super::save(tmp.path(), &mut session).unwrap();
        assert!(search(tmp.path(), "العدد واحد").unwrap()["sessions"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn corrupt_or_unwritable_cache_still_returns_verified_matches() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        let mut s = fixture(dir, true);
        super::super::save(dir, &mut s).unwrap();
        let cache = dir.join(CACHE);
        fs::write(&cache, b"invalid").unwrap();
        assert_eq!(search(dir, "quantity").unwrap()["reindexed"], 1);
        fs::remove_file(&cache).unwrap();
        fs::create_dir(&cache).unwrap();
        let result = search(dir, "fill").unwrap();
        assert_eq!(result["cache_persisted"], false);
        assert_eq!(result["sessions"][0]["id"], s.id);
    }
}
