//! Static local ECMAScript module bindings. Oxc resolves lexical references
//! within each module. The linker joins explicit relative imports to exports.
use super::Index;
use anyhow::{ensure, Result};
use oxc_allocator::Allocator;
use oxc_ast::{ast::Expression, AstKind};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_syntax::module_record::{ExportExportName, ExportImportName, ImportImportName};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

struct Import {
    source: String,
    exported: Option<String>, // None is a namespace import.
    offset: u32,
}
enum Export {
    Local(String),
    Forward(String, String),
}
struct Call {
    local: String,
    member: Option<String>,
    offset: u32,
}
#[derive(Default)]
pub(super) struct Module {
    definitions: BTreeMap<String, u32>,
    exports: BTreeMap<String, Export>,
    imports: BTreeMap<String, Import>,
    calls: Vec<Call>,
    lines: Vec<usize>,
}

impl Module {
    fn parse(path: &str, body: &str) -> Option<Self> {
        let arena = Allocator::default();
        let parsed = Parser::new(&arena, body, SourceType::from_path(path).ok()?).parse();
        if !parsed.errors.is_empty() || parsed.panicked {
            return None;
        }
        let analyzed = SemanticBuilder::new()
            .with_check_syntax_error(true)
            .build(&parsed.program);
        if !analyzed.errors.is_empty() {
            return None;
        }
        let sem = analyzed.semantic;
        let scopes = sem.scoping();
        let mut module = Self::default();
        // ECMAScript recognizes CR, CRLF, LF and both Unicode line separators.
        module.lines.push(0);
        let mut chars = body.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            match c {
                '\r' if chars.peek().is_some_and(|(_, next)| *next == '\n') => (),
                '\r' | '\n' | '\u{2028}' | '\u{2029}' => module.lines.push(i + c.len_utf8()),
                _ => (),
            }
        }
        for entry in &parsed.module_record.import_entries {
            if entry.is_type {
                continue;
            }
            let exported = match &entry.import_name {
                ImportImportName::Name(n) => Some(n.name.to_string()),
                ImportImportName::Default(_) => Some("default".into()),
                ImportImportName::NamespaceObject => None,
            };
            module.imports.insert(
                entry.local_name.name.to_string(),
                Import {
                    source: entry.module_request.name.to_string(),
                    exported,
                    offset: entry.local_name.span.start,
                },
            );
        }
        for entry in parsed
            .module_record
            .local_export_entries
            .iter()
            .chain(&parsed.module_record.indirect_export_entries)
        {
            if entry.is_type {
                continue;
            }
            let name = match &entry.export_name {
                ExportExportName::Name(n) => n.name.to_string(),
                ExportExportName::Default(_) => "default".into(),
                _ => continue,
            };
            let binding = if let (Some(src), ExportImportName::Name(n)) =
                (&entry.module_request, &entry.import_name)
            {
                Export::Forward(src.name.to_string(), n.name.to_string())
            } else if let Some(local) = entry.local_name.name() {
                Export::Local(local.to_string())
            } else {
                continue;
            };
            module.exports.insert(name, binding);
        }
        for id in scopes.iter_bindings_in(scopes.root_scope_id()) {
            let callable = match sem.symbol_declaration(id).kind() {
                AstKind::Function(f) => f.body.is_some(),
                AstKind::VariableDeclarator(d) => {
                    d.id.get_binding_identifier().is_some()
                        && matches!(
                            d.init.as_ref().map(Expression::get_inner_expression),
                            Some(
                                Expression::ArrowFunctionExpression(_)
                                    | Expression::FunctionExpression(_)
                            )
                        )
                }
                _ => false,
            };
            if callable && !scopes.symbol_is_mutated(id) {
                module
                    .definitions
                    .insert(scopes.symbol_name(id).into(), scopes.symbol_span(id).start);
            }
        }
        for node in sem.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let (ident, member, offset) = match call.callee.get_inner_expression() {
                Expression::Identifier(i) => (i.as_ref(), None, i.span.start),
                Expression::StaticMemberExpression(m) => {
                    let Some(i) = m.object.get_identifier_reference() else {
                        continue;
                    };
                    (i, Some(m.property.name.to_string()), m.property.span.start)
                }
                _ => continue,
            };
            let Some(reference) = ident.reference_id.get() else {
                continue;
            };
            let Some(binding) = scopes.get_reference(reference).symbol_id() else {
                continue;
            };
            if scopes.get_root_binding(ident.name.as_str()) == Some(binding) {
                module.calls.push(Call {
                    local: ident.name.to_string(),
                    member,
                    offset,
                });
            }
        }
        Some(module)
    }
}

type Modules<'a> = BTreeMap<&'a str, &'a Module>;
type Symbol<'a> = (&'a str, &'a str);

fn relative(importer: &str, source: &str) -> Option<String> {
    if !(source.starts_with("./") || source.starts_with("../"))
        || source.contains(['\\', '?', '#', '%'])
    {
        return None;
    }
    let mut path = Path::new(importer).parent()?.to_path_buf();
    for part in Path::new(source).components() {
        match part {
            Component::CurDir => (),
            Component::ParentDir => {
                if !path.pop() {
                    return None;
                }
            }
            Component::Normal(part) => path.push(part),
            _ => return None,
        }
    }
    Some(path.to_str()?.replace('\\', "/"))
}

fn target<'a>(index: &'a Index, importer: &str, source: &str) -> Option<&'a str> {
    let name = relative(importer, source)?;
    let known = |candidate: &str| {
        index
            .files
            .get_key_value(candidate)
            .map(|(path, _)| path.as_str())
    };
    let typed = matches!(
        Path::new(importer).extension()?.to_str(),
        Some("ts" | "tsx")
    );
    let ext = Path::new(&name).extension().and_then(|s| s.to_str());
    let mut choices = vec![name.clone()];
    match ext {
        None => {
            for ext in ["js", "jsx", "mjs", "ts", "tsx"] {
                choices.push(format!("{name}.{ext}"));
                choices.push(format!("{name}/index.{ext}"));
            }
        }
        Some("js" | "jsx") if typed => {
            let stem = name.rsplit_once('.')?.0;
            choices.extend(["ts", "tsx"].map(|ext| format!("{stem}.{ext}")));
        }
        _ => return known(&name),
    }
    let mut found = None;
    for candidate in choices {
        if let Some(path) = known(&candidate) {
            if found.replace(path).is_some() {
                return None;
            }
        } else {
            // An ignored, oversized or symlinked candidate cannot justify
            // falling through to a different module with the same stem.
            match std::fs::symlink_metadata(index.root.join(candidate)) {
                Ok(meta) if meta.is_dir() => (),
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound
                        || e.kind() == std::io::ErrorKind::NotADirectory => {}
                _ => return None,
            }
        }
    }
    found
}

struct Graph<'a> {
    modules: Modules<'a>,
    targets: BTreeMap<(&'a str, &'a str), &'a str>,
}
impl<'a> Graph<'a> {
    fn exported(&self, mut path: &'a str, mut name: &'a str) -> Option<Symbol<'a>> {
        for _ in 0..32 {
            let module = self.modules.get(path)?;
            let (src, remote) = match module.exports.get(name)? {
                Export::Local(local) if module.definitions.contains_key(local) => {
                    return Some((path, local))
                }
                Export::Local(local) => {
                    let import = module.imports.get(local)?;
                    (&import.source, import.exported.as_ref()?)
                }
                Export::Forward(src, remote) => (src, remote),
            };
            path = self.targets.get(&(path, src.as_str()))?;
            name = remote;
        }
        None
    }
    fn imported(&self, path: &'a str, i: &'a Import, name: &'a str) -> Option<Symbol<'a>> {
        self.exported(self.targets.get(&(path, i.source.as_str()))?, name)
    }
}

pub(super) fn search(
    index: &Index,
    query: &str,
    path: &str,
    limit: usize,
    args: &Value,
) -> Result<Value> {
    ensure!(
        !query.is_empty() && query.len() <= 256 && !query.chars().any(char::is_whitespace),
        "Supply one exact symbol name"
    );
    let offset = usize::try_from(
        args.get("offset")
            .map(|v| v.as_u64().ok_or_else(|| anyhow::anyhow!("Invalid offset")))
            .transpose()?
            .unwrap_or(0),
    )?;
    ensure!(
        offset == 0 || args["expected_fingerprint"] == index.fingerprint,
        "Index changed or missing fingerprint; restart at offset zero"
    );
    let mut graph = Graph {
        modules: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let mut skipped = 0;
    for (name, f) in &index.files {
        if !matches!(f.language.as_str(), "JavaScript" | "TypeScript" | "TSX") {
            continue;
        }
        if !f.parse_errors && !f.structure_limited && f.trace_depth_ok {
            if let Some(module) = f.bindings.get_or_init(|| Module::parse(name, &f.body)) {
                graph.modules.insert(name, module);
                continue;
            }
        }
        skipped += 1;
    }
    for (&file, m) in &graph.modules {
        for src in m
            .imports
            .values()
            .map(|i| &i.source)
            .chain(m.exports.values().filter_map(|e| {
                if let Export::Forward(src, _) = e {
                    Some(src)
                } else {
                    None
                }
            }))
        {
            if let Some(found) = target(index, file, src) {
                graph.targets.insert((file, src.as_str()), found);
            }
        }
    }
    let mut selected = BTreeSet::new();
    for (&file, m) in &graph.modules {
        if let Some((local, _)) = m.definitions.get_key_value(query) {
            selected.insert((file, local.as_str()));
        }
        if let Some((name, _)) = m.exports.get_key_value(query) {
            selected.extend(graph.exported(file, name));
        }
        if let Some(i) = m.imports.get(query) {
            if let Some(exported) = &i.exported {
                selected.extend(graph.imported(file, i, exported));
            }
        }
    }
    selected
        .retain(|(file, _)| path.is_empty() || path == "." || Path::new(file).starts_with(path));
    let mut rows = Vec::new();
    for &(file, name) in &selected {
        rows.push((
            (file, name),
            0u8,
            file,
            graph.modules[file].definitions[name],
            name,
        ));
    }
    for (&file, m) in &graph.modules {
        let mut namespaces = BTreeSet::new();
        for (alias, i) in &m.imports {
            if let Some(sym) = i
                .exported
                .as_ref()
                .and_then(|n| graph.imported(file, i, n))
                .filter(|s| selected.contains(s))
            {
                rows.push((sym, 1, file, i.offset, alias.as_str()));
            }
        }
        for call in &m.calls {
            let sym = if let Some(i) = m.imports.get(&call.local) {
                let remote = match (&i.exported, &call.member) {
                    (Some(n), None) | (None, Some(n)) => n,
                    _ => continue,
                };
                let sym = graph.imported(file, i, remote);
                if let Some(sym) = sym.filter(|s| selected.contains(s)) {
                    if i.exported.is_none() && namespaces.insert((sym, i.offset)) {
                        rows.push((sym, 1, file, i.offset, call.local.as_str()));
                    }
                }
                sym
            } else if call.member.is_none() {
                Some((file, call.local.as_str()))
            } else {
                None
            };
            if let Some(sym) = sym.filter(|s| selected.contains(s)) {
                rows.push((
                    sym,
                    2,
                    file,
                    call.offset,
                    call.member.as_ref().unwrap_or(&call.local).as_str(),
                ));
            }
        }
    }
    rows.sort_unstable();
    let total = rows.len();
    ensure!(offset <= total, "Offset exceeds results");
    let matches=rows.into_iter().skip(offset).take(limit).map(|((file,symbol),role,at,byte,local)|{
        let file_data=&index.files[at];
        let starts=&graph.modules[at].lines;
        let line=starts.partition_point(|&p|p<=byte as usize);
        let text=file_data.body[starts[line-1]..starts.get(line).copied().unwrap_or(file_data.body.len())].trim_end_matches(['\r','\n','\u{2028}','\u{2029}']);
        json!({"role":(["definition","import","call"][role as usize]),"definition_path":file,"symbol":symbol,"local_name":local,"path":at,"line":line,"byte_offset":byte,"text":text.chars().take(512).collect::<String>(),"text_truncated":text.chars().count()>512,"sha256":file_data.hash})
    }).collect::<Vec<_>>();
    Ok(
        json!({"ok":true,"kind":"bindings","query":query,"matches":matches,"total_matches":total,"next_offset":(offset+matches.len()<total).then_some(offset+matches.len()),"workspace_fingerprint":index.fingerprint,"coverage":index.coverage,"skipped_semantic_files":skipped,"untrusted_source_data":true,"limits":"Static top-level function bindings in indexed JS/TS modules. Explicit local ESM imports and named re-exports only. External packages, tsconfig paths, export-star, CommonJS and dynamic dispatch are outside coverage. Ambiguous extensionless targets are omitted."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn trace(index: &mut Index, root: &Path, name: &str, path: &str) -> Value {
        index
            .search(
                root,
                &json!({"kind":"bindings","query":name,"path":path,"limit":20}),
            )
            .unwrap()
    }

    #[test]
    fn quote_lines_are_file_relative_and_refresh_after_an_edit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let assets = root.join("public/assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(
            assets.join("format.js"),
            "export function formatQuote(total) {\n  return total;\n}",
        )
        .unwrap();
        let call = r#"    byId("result").textContent = formatQuote(quote.total);"#;
        let src = format!(
            "import {{ formatQuote }} from './format.js';\r\n{}{}\r\n",
            "// 🔵 فاصل\r\n".repeat(14),
            call
        );
        fs::write(assets.join("bench.js"), &src).unwrap();
        let mut index = Index::default();
        let result = trace(&mut index, root, "formatQuote", "");
        let rows = result["matches"].as_array().unwrap();
        assert_eq!(rows.len(), 3, "{result}");
        assert_eq!(
            (rows[0]["role"].as_str(), rows[0]["line"].as_u64()),
            (Some("definition"), Some(1))
        );
        assert_eq!(
            (rows[1]["role"].as_str(), rows[1]["line"].as_u64()),
            (Some("import"), Some(1))
        );
        assert_eq!(
            (rows[2]["role"].as_str(), rows[2]["line"].as_u64()),
            (Some("call"), Some(16))
        );
        assert_eq!(rows[2]["text"], call);
        let page = index
            .search(
                root,
                &json!({"query":"formatQuote","kind":"bindings","limit":2}),
            )
            .unwrap();
        assert_eq!(page["next_offset"], 2);
        let request = json!({"query":"formatQuote","kind":"bindings","offset":2,"expected_fingerprint":page["workspace_fingerprint"]});
        assert_eq!(index.search(root, &request).unwrap()["matches"][0], rows[2]);
        fs::write(assets.join("bench.js"), format!("\n{src}")).unwrap();
        assert!(index.search(root, &request).is_err());
        assert_eq!(
            trace(&mut index, root, "formatQuote", "")["matches"][2]["line"],
            17
        );
    }

    #[test]
    fn aliases_reexports_and_namespace_calls_resolve_to_the_right_definition() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (file,text) in [
            ("one.ts","export const formatQuote = (v:number) => v;"),
            ("two.ts","export function formatQuote(v:number){return v*2;}"),
            ("bridge.ts","export {formatQuote as renamed} from './one.js';"),
            ("ui.tsx","import {renamed as show} from './bridge';\nimport {formatQuote as other} from './two';\nimport * as ns from './one';\nshow(1);\nns.formatQuote(2);\nother(3);\nfunction shadow(show:Function){show(4);}\nconst fake='show(5)';\n// show(6)\nconst jsx=<span>{show(7)}</span>;")
        ]{fs::write(root.join(file),text).unwrap();}
        let mut index = Index::default();
        let result = trace(&mut index, root, "show", "");
        let rows = result["matches"].as_array().unwrap();
        assert_eq!(rows.len(), 6, "{result}");
        assert!(rows.iter().all(|r| r["definition_path"] == "one.ts"));
        let lines: Vec<_> = rows
            .iter()
            .filter(|r| r["role"] == "call")
            .map(|r| r["line"].as_u64().unwrap())
            .collect();
        assert_eq!(lines, vec![4, 5, 10]);
        let result = trace(&mut index, root, "formatQuote", "two.ts");
        assert_eq!(result["matches"][2]["line"], 6);
        assert_eq!(result["matches"][2]["local_name"], "other");
    }

    #[test]
    fn omits_type_only_shadowed_and_ambiguous_calls() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (name,body) in [
            ("a.ts","export default function f(){}"),
            ("a.js","export default function f(){}"),
            ("use.ts","import typed from './a';\nimport type typeOnly from './a.js';\ntyped();\ntypeOnly();"),
            ("use.js","import actual from './a.js';\nfunction outer(actual){actual();}\nactual();"),
        ]{fs::write(root.join(name),body).unwrap();}
        let result = trace(&mut Index::default(), root, "f", "a.js");
        let rows = result["matches"].as_array().unwrap();
        assert_eq!(rows.len(), 3, "{result}");
        assert_eq!(rows[2]["path"], "use.js");
        assert_eq!(rows[2]["line"], 3);
    }
}
