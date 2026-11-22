//! A quoted, single-file write request can supply its bytes directly.
//! Do not infer file content from examples in history or from tool output.
use crate::tools::ToolBox;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

pub struct ExactFile<'a> {
    path: &'a str,
    content: String,
}

impl<'a> ExactFile<'a> {
    pub fn parse(prompt: &'a str) -> Option<Self> {
        let (verb, rest) = prompt.trim_start().split_once(char::is_whitespace)?;
        if !["create", "write"]
            .iter()
            .any(|v| verb.eq_ignore_ascii_case(v))
        {
            return None;
        }
        let (path, rest) = rest.trim_start().split_once(char::is_whitespace)?;
        let path = path
            .strip_prefix('`')
            .and_then(|p| p.strip_suffix('`'))
            .unwrap_or(path);
        let (instruction, quoted) = rest.trim_start().split_once(':')?;
        let instruction = instruction.to_ascii_lowercase();
        // Only expand LF escapes when the user specifically requests that convention.
        let (instruction, expand_lf) =
            match instruction.strip_suffix(r", interpreting \n as newline characters") {
                Some(before) => (before, true),
                None => (instruction.as_str(), false),
            };
        if !matches!(
            instruction,
            "with exactly this content" | "with this exact content" | "with exact content"
        ) {
            return None;
        }
        let (literal, suffix) = quoted.trim_start().strip_prefix('"')?.split_once('"')?;
        let suffix = suffix.trim_start();
        let (suffix, trailing_lf) = suffix
            .strip_prefix("with a trailing newline")
            .map_or((suffix, false), |rest| (rest, true));
        if !(suffix.is_empty()
            || suffix == "."
            || suffix
                .strip_prefix('.')
                .is_some_and(|s| s.starts_with(char::is_whitespace)))
            || literal.ends_with('\\')
            || (expand_lf && literal.contains(r"\\"))
        {
            return None;
        }
        let mut content = if expand_lf {
            literal.replace(r"\n", "\n")
        } else {
            literal.to_owned()
        };
        if trailing_lf && !content.ends_with('\n') {
            content.push('\n');
        }
        if content.len() > crate::local_files::WINDOW || content.contains('\0') {
            return None;
        }
        Some(Self { path, content })
    }

    pub fn bind_write(&self, args: &mut Value, changes: &mut Value) {
        if args["path"].as_str() == Some(self.path)
            && args["content"].as_str().is_some_and(|s| s != self.content)
        {
            // Bind before preview/approval. Preserve both provider arguments and the
            // effective arguments in the existing tool history.
            args["content"] = json!(self.content);
            if let Some(changes) = changes.as_array_mut() {
                changes.push(json!({"field":"$.content","source":"current_user_quoted_literal","bytes":self.content.len()}));
            }
        }
    }

    pub fn verify(&self, tools: &ToolBox) -> Result<String> {
        let observed = tools
            .read_file_window(self.path, 0, crate::local_files::WINDOW as u64)
            .with_context(|| format!("Exact content of {} could not be verified", self.path))?;
        ensure!(observed["eof"] == true && observed["content"].as_str() == Some(&self.content),
            "Exact content mismatch for {}. Expected {} bytes (ends_with_newline={}), observed {} bytes (ends_with_newline={}).",
            self.path, self.content.len(), self.content.ends_with('\n'), observed["file_bytes"], observed["ends_with_newline"]);
        Ok(format!(
            "Verified {}: {} bytes, ends_with_newline={}.",
            self.path, observed["file_bytes"], observed["ends_with_newline"]
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_literal_and_explicit_newlines_without_touching_other_paths() {
        let escaped = ExactFile::parse(r#"Create note with exactly this content, interpreting \n as newline characters: "ع\n\n". Read it."#).unwrap();
        assert_eq!(escaped.content, "ع\n\n");
        let mut args = json!({"path":"other","content":"value"});
        escaped.bind_write(&mut args, &mut json!([]));
        assert_eq!(args["content"], "value");
        let explicit = ExactFile::parse(
            r#"Write note with exact content: "a" with a trailing newline. Read it."#,
        )
        .unwrap();
        assert_eq!(explicit.content, "a\n");
        let literal = ExactFile::parse(r#"Write note with exact content: "a\n"."#).unwrap();
        assert_eq!(literal.content, r"a\n");
        assert!(
            ExactFile::parse(r#"Explain this example: Create note with exact content: "a"."#)
                .is_none()
        );
        assert!(ExactFile::parse(r#"Create note with exact content: "a" or "b"."#).is_none());
    }
}
