//! Bounded syntactic flow candidates, not resolved taint or sanitizer proofs.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
pub fn inspect(body: &str, symbol: &str) -> Result<Value> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_javascript::LANGUAGE.into())?;
    let started = Instant::now();
    let mut cancel = |_: &tree_sitter::ParseState| started.elapsed() > Duration::from_millis(150);
    let bytes = body.as_bytes();
    let tree = parser
        .parse_with_options(
            &mut |at, _| bytes.get(at..).unwrap_or(&[]),
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut cancel)),
        )
        .context("JavaScript parse deadline exceeded")?;
    let mut stack = vec![tree.root_node()];
    let mut rows = Vec::new();
    let mut imports = Vec::new();
    let mut count = 0;
    let mut limited = false;
    while let Some(node) = stack.pop() {
        count += 1;
        if count > 60_000 || rows.len() >= 80 || started.elapsed() > Duration::from_millis(200) {
            limited = true;
            break;
        }
        let text = node.utf8_text(bytes).unwrap_or("");
        if node.kind() == "import_statement" {
            if let Some(source) = node.child_by_field_name("source") {
                imports.push(json!({"specifier":source.utf8_text(bytes).unwrap_or("").trim_matches(['\'', '"']),"byte_offset":node.start_byte(),"binding_excerpt":text.chars().take(200).collect::<String>()}));
            }
        }
        let relevant = text.contains(symbol);
        if relevant
            && matches!(
                node.kind(),
                "variable_declarator"
                    | "assignment_expression"
                    | "return_statement"
                    | "call_expression"
            )
        {
            let left = node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("left"))
                .or_else(|| node.child_by_field_name("function"));
            let right = node
                .child_by_field_name("value")
                .or_else(|| node.child_by_field_name("right"));
            let label = match node.kind() {
                "call_expression" => "call_candidate_not_a_validation_guarantee",
                "return_statement" => "return_candidate",
                _ => "syntactic_assignment",
            };
            rows.push(json!({"kind":label,"target":left.and_then(|n|n.utf8_text(bytes).ok()).map(|s|s.chars().take(160).collect::<String>()),"input_expression":right.and_then(|n|n.utf8_text(bytes).ok()).map(|s|s.chars().take(240).collect::<String>()),"line":node.start_position().row+1,"byte_offset":node.start_byte(),"end_byte":node.end_byte(),"excerpt":text.chars().take(320).collect::<String>()}));
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    Ok(
        json!({"candidates":rows,"imports":imports,"parse_errors":tree.root_node().has_error(),"limited":limited,"limits":"AST syntax candidates only. Identifiers can shadow/alias; calls may validate, transform or do neither. No type resolution, execution order, interprocedural taint, sanitizer correctness or exploitability proof. Import links indicate only cached URL resolution."}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traces_source_transform_and_sink_as_unverified_candidates_not_comment_code() {
        let body="// const fake=location.hash;\nimport {clean} from './clean.js';\nconst input=location.hash; const transformed=clean(input); node.innerHTML=transformed;";
        let input = inspect(body, "input").unwrap();
        let sink = inspect(body, "transformed").unwrap();
        assert!(input["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["target"] == "input"));
        assert!(!input["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["target"] == "fake"));
        assert!(sink["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["target"] == "node.innerHTML"));
        assert_eq!(input["imports"][0]["specifier"], "./clean.js");
        assert!(input["limits"]
            .as_str()
            .unwrap()
            .contains("No type resolution"));
    }
}
