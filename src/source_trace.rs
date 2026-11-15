//! Bounded syntactic flow candidates, not resolved taint or sanitizer proofs.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
fn bindings(node: tree_sitter::Node<'_>, bytes: &[u8]) -> (Vec<Value>, bool) {
    let mut pending = vec![node];
    let mut rows = Vec::new();
    let mut visited = 0;
    while let Some(current) = pending.pop() {
        visited += 1;
        if visited > 128 || rows.len() >= 32 {
            return (rows, true);
        }
        let text = |n: tree_sitter::Node<'_>| {
            n.utf8_text(bytes)
                .unwrap_or("")
                .chars()
                .take(160)
                .collect::<String>()
        };
        match current.kind() {
            "import_specifier" | "export_specifier" => {
                if let Some(name) = current.child_by_field_name("name") {
                    let alias = current.child_by_field_name("alias").unwrap_or(name);
                    rows.push(json!({"imported":text(name),"local_or_exported":text(alias),"kind":current.kind()}));
                }
            }
            "identifier"
                if current
                    .parent()
                    .is_some_and(|p| p.kind() == "import_clause") =>
            {
                rows.push(json!({"imported":"default","local_or_exported":text(current),"kind":"default_import"}));
            }
            "identifier"
                if current
                    .parent()
                    .is_some_and(|p| p.kind() == "namespace_import") =>
            {
                rows.push(json!({"imported":"*","local_or_exported":text(current),"kind":"namespace_import"}));
            }
            _ => {}
        }
        let mut cursor = current.walk();
        pending.extend(current.named_children(&mut cursor));
    }
    (rows, false)
}
fn function_scope(mut node: tree_sitter::Node<'_>) -> (usize, usize, String) {
    loop {
        if matches!(
            node.kind(),
            "function_declaration"
                | "function_expression"
                | "arrow_function"
                | "method_definition"
                | "generator_function_declaration"
                | "generator_function"
        ) || node.parent().is_none()
        {
            return (node.start_byte(), node.end_byte(), node.kind().to_owned());
        }
        node = node.parent().unwrap();
    }
}
fn references_symbol(root: tree_sitter::Node<'_>, bytes: &[u8], symbol: &str) -> (bool, bool) {
    let mut stack = vec![root];
    let mut count = 0;
    while let Some(node) = stack.pop() {
        count += 1;
        if count > 1024 {
            return (false, true);
        }
        if matches!(
            node.kind(),
            "function_declaration"
                | "function_expression"
                | "arrow_function"
                | "method_definition"
                | "generator_function_declaration"
                | "generator_function"
        ) {
            continue;
        }
        if matches!(
            node.kind(),
            "identifier" | "shorthand_property_identifier" | "member_expression"
        ) && node.utf8_text(bytes).ok() == Some(symbol)
        {
            return (true, false);
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    (false, false)
}
fn simple_names(node: Option<tree_sitter::Node<'_>>, bytes: &[u8]) -> Value {
    let Some(node) = node else {
        return Value::Null;
    };
    let mut cursor = node.walk();
    let items = node.named_children(&mut cursor).collect::<Vec<_>>();
    let complete = items.len() <= 8
        && items
            .iter()
            .all(|n| n.kind() == "identifier" && n.end_byte() - n.start_byte() <= 100);
    json!({"simple_identifiers_only":complete,"count":items.len(),"names":if complete {Some(items.iter().map(|n|n.utf8_text(bytes).unwrap_or("")).collect::<Vec<_>>())}else{None}})
}
pub fn inspect_scoped(body: &str, symbol: &str, anchor: usize) -> Result<Value> {
    inspect_inner(body, symbol, Some(anchor))
}
pub fn inspect(body: &str, symbol: &str) -> Result<Value> {
    inspect_inner(body, symbol, None)
}
fn inspect_inner(body: &str, symbol: &str, anchor: Option<usize>) -> Result<Value> {
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
    let selected_scope = if let Some(at) = anchor {
        if at >= bytes.len() || !body.is_char_boundary(at) {
            anyhow::bail!("Scope anchor must be a valid UTF-8 byte offset inside the source");
        }
        Some(function_scope(
            tree.root_node()
                .named_descendant_for_byte_range(at, at)
                .context("No syntax node at scope anchor")?,
        ))
    } else {
        None
    };
    let mut stack = vec![tree.root_node()];
    let mut candidates = std::collections::BTreeMap::new();
    let mut imports = Vec::new();
    let mut dynamic_imports = Vec::new();
    let mut count = 0;
    let mut limited = false;
    while let Some(node) = stack.pop() {
        count += 1;
        if count > 60_000
            || candidates.len() >= 80
            || imports.len() + dynamic_imports.len() >= 80
            || started.elapsed() > Duration::from_millis(200)
        {
            limited = true;
            break;
        }
        let text = node.utf8_text(bytes).unwrap_or("");
        if matches!(node.kind(), "import_statement" | "export_statement") {
            if let Some(source) = node.child_by_field_name("source") {
                let (names, bindings_limited) = bindings(node, bytes);
                limited |= bindings_limited;
                imports.push(json!({"edge_kind":node.kind(),"bindings":names,"bindings_limited":bindings_limited,"dataflow_proven":false,"specifier":source.utf8_text(bytes).unwrap_or("").trim_matches(['\'', '"']),"byte_offset":node.start_byte(),"binding_excerpt":text.chars().take(200).collect::<String>()}));
            }
        }
        if node.kind() == "call_expression"
            && node
                .child_by_field_name("function")
                .is_some_and(|function| function.kind() == "import")
        {
            let literal = node.child_by_field_name("arguments").and_then(|arguments| {
                let mut cursor = arguments.walk();
                let children = arguments.named_children(&mut cursor).collect::<Vec<_>>();
                if children.len() != 1 || children[0].kind() != "string" {
                    return None;
                }
                let raw = children[0].utf8_text(bytes).ok()?;
                if raw.len() < 3 || raw.len() > 2002 || raw.contains('\\') {
                    return None;
                }
                let first = raw.as_bytes()[0];
                if !matches!(first, b'\'' | b'"') || raw.as_bytes().last() != Some(&first) {
                    return None;
                }
                Some(&raw[1..raw.len() - 1])
            });
            limited |= literal.is_none();
            dynamic_imports.push(json!({"specifier":literal,"byte_offset":node.start_byte(),
                "kind":if literal.is_some(){"literal_dynamic_import_candidate"}else{"unresolved_dynamic_import"},
                "runtime_loading_verified":false}));
        }
        // Match syntax references, never substrings in comments/string literals.
        if matches!(
            node.kind(),
            "identifier" | "shorthand_property_identifier" | "member_expression"
        ) && text == symbol
        {
            let mut ancestor = Some(node);
            while let Some(candidate) = ancestor {
                count += 1;
                if count > 60_000
                    || candidates.len() >= 80
                    || started.elapsed() > Duration::from_millis(200)
                {
                    limited = true;
                    break;
                }
                if matches!(
                    candidate.kind(),
                    "variable_declarator"
                        | "function_declaration"
                        | "assignment_expression"
                        | "augmented_assignment_expression"
                        | "update_expression"
                        | "return_statement"
                        | "call_expression"
                ) {
                    candidates.insert(
                        (
                            candidate.start_byte(),
                            candidate.end_byte(),
                            candidate.kind(),
                        ),
                        candidate,
                    );
                }
                ancestor = candidate.parent();
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    let mut rows = Vec::new();
    for node in candidates.values() {
        let scope = function_scope(*node);
        if selected_scope
            .as_ref()
            .is_some_and(|selected| *selected != scope)
        {
            continue;
        }
        let text = node.utf8_text(bytes).unwrap_or("");
        let left = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("left"))
            .or_else(|| node.child_by_field_name("function"))
            .or_else(|| {
                (node.kind() == "update_expression")
                    .then(|| node.child_by_field_name("argument"))
                    .flatten()
            });
        let right = node
            .child_by_field_name("value")
            .or_else(|| node.child_by_field_name("right"))
            .or_else(|| node.child_by_field_name("arguments"))
            .or_else(|| {
                (node.kind() == "return_statement")
                    .then(|| node.named_child(0))
                    .flatten()
            });
        let (query_in_input, input_scan_limited) = right
            .map(|n| references_symbol(n, bytes, symbol))
            .unwrap_or((false, false));
        limited |= input_scan_limited;
        let label = match node.kind() {
            "function_declaration" => "function_declaration_candidate_not_resolved_call_target",
            "call_expression" => "call_candidate_not_a_validation_guarantee",
            "return_statement" => "return_candidate",
            "augmented_assignment_expression" | "update_expression" => "syntactic_update",
            _ => "syntactic_assignment",
        };
        let direct_named_module_export = node.kind() == "function_declaration"
            && node.parent().is_some_and(|parent| {
                let mut cursor = parent.walk();
                parent.kind() == "export_statement"
                    && parent
                        .parent()
                        .is_some_and(|module| module.kind() == "program")
                    && !parent
                        .children(&mut cursor)
                        .any(|child| child.kind() == "default")
            });
        rows.push(json!({"direct_named_module_export":direct_named_module_export,"kind":label,"call_arguments":if node.kind()=="call_expression"{simple_names(node.child_by_field_name("arguments"),bytes)}else{Value::Null},"declaration_parameters":if node.kind()=="function_declaration"{simple_names(node.child_by_field_name("parameters"),bytes)}else{Value::Null},"query_in_input":query_in_input,"input_scan_limited":input_scan_limited,"target_is_identifier":left.is_some_and(|n|n.kind()=="identifier"),"function_scope":{"start_byte":scope.0,"end_byte":scope.1,"kind":scope.2},"target":left.and_then(|n|n.utf8_text(bytes).ok()).map(|s|s.chars().take(160).collect::<String>()),"input_expression":right.and_then(|n|n.utf8_text(bytes).ok()).map(|s|s.chars().take(240).collect::<String>()),"line":node.start_position().row+1,"byte_offset":node.start_byte(),"end_byte":node.end_byte(),"excerpt":text.chars().take(320).collect::<String>()}));
    }
    Ok(
        json!({"selected_function_scope":selected_scope.map(|s|json!({"start_byte":s.0,"end_byte":s.1,"kind":s.2})),"candidates":rows,"imports":imports,"dynamic_imports":dynamic_imports,"parse_errors":tree.root_node().has_error(),"limited":limited,"limits":"AST syntax candidates only. Identifiers can shadow/alias; calls may validate, transform or do neither. No type resolution, execution order, interprocedural taint, sanitizer correctness or exploitability proof. Import links indicate only cached URL resolution."}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn module_edges_expose_aliases_defaults_namespaces_and_reexports() {
        let source = "import clean, {decode as transform} from './parse.js'; import * as helpers from './helpers.js'; export {transform as sanitize} from './bridge.js'; const out=transform(input);";
        let result = inspect(source, "transform").unwrap();
        let imports = result["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 3);
        let parse = imports
            .iter()
            .find(|row| row["specifier"] == "./parse.js")
            .unwrap();
        assert!(parse["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["imported"] == "decode" && b["local_or_exported"] == "transform"));
        assert!(parse["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["imported"] == "default" && b["local_or_exported"] == "clean"));
        let namespace = imports
            .iter()
            .find(|row| row["specifier"] == "./helpers.js")
            .unwrap();
        assert_eq!(namespace["bindings"][0]["imported"], "*");
        let export = imports
            .iter()
            .find(|row| row["specifier"] == "./bridge.js")
            .unwrap();
        assert_eq!(export["edge_kind"], "export_statement");
        assert_eq!(export["bindings"][0]["local_or_exported"], "sanitize");
        assert!(imports.iter().all(|row| row["dataflow_proven"] == false));
    }
    #[test]
    fn retains_nested_calls_with_same_start_position() {
        let result = inspect("factory(input)(other)", "input").unwrap();
        let calls = result["candidates"].as_array().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["byte_offset"], calls[1]["byte_offset"]);
        assert_ne!(calls[0]["end_byte"], calls[1]["end_byte"]);
    }
    #[test]
    fn excludes_substrings_literals_and_property_names() {
        let body = "const inputExtra = 1; const quoted = 'input'; const obj = {input: 2}; const real = transform(input);";
        let result = inspect(body, "input").unwrap();
        let rows = result["candidates"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .all(|row| row["target"] == "real" || row["target"] == "transform"));
        let member = inspect(
            "const a = location.hash; const b = 'location.hash';",
            "location.hash",
        )
        .unwrap();
        assert_eq!(member["candidates"].as_array().unwrap().len(), 1);
        assert_eq!(member["candidates"][0]["target"], "a");
    }
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
