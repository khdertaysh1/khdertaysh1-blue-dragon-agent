//! Static local ECMAScript module bindings. Oxc resolves lexical references
//! within each module. Link only module paths backed by indexed workspace files.
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
    Unsupported,
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
    stars: Vec<String>,
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
        for entry in &parsed.module_record.star_export_entries {
            if !entry.is_type && entry.import_name.is_all_but_default() {
                module
                    .stars
                    .push(entry.module_request.as_ref()?.name.to_string());
            }
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
                Export::Unsupported
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

// Ok(None) means no candidate exists. Ambiguous or excluded files must also
// block fallback to a lower-priority paths substitution.
fn candidate<'a>(index: &'a Index, importer: &str, name: String) -> Result<Option<&'a str>, ()> {
    let known = |candidate: &str| {
        index
            .files
            .get_key_value(candidate)
            .map(|(path, _)| path.as_str())
    };
    let typed = matches!(
        Path::new(importer).extension().and_then(|s| s.to_str()),
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
            let stem = name.rsplit_once('.').ok_or(())?.0;
            choices.extend(["ts", "tsx"].map(|ext| format!("{stem}.{ext}")));
        }
        _ => (),
    }
    let mut found = None;
    for candidate in choices {
        if let Some(path) = known(&candidate) {
            if found.replace(path).is_some() {
                return Err(());
            }
        } else {
            // An ignored, oversized or symlinked candidate cannot justify
            // falling through to a different module with the same stem.
            match std::fs::symlink_metadata(index.root.join(candidate)) {
                Ok(meta) if meta.is_dir() => (),
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound
                        || e.kind() == std::io::ErrorKind::NotADirectory => {}
                _ => return Err(()),
            }
        }
    }
    Ok(found)
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Options {
    base_url: Option<String>,
    #[serde(default)]
    paths: BTreeMap<String, Vec<String>>,
}

fn paths_config(body: &str) -> Option<Options> {
    // Bound the recursive JSONC parser even for adversarial project files.
    use jsonc_parser::tokens::Token;
    let body = body.trim_start_matches('\u{feff}');
    let mut scanner = jsonc_parser::Scanner::new(body);
    let mut depth = 0u8;
    while let Some(token) = scanner.scan().ok()? {
        match token {
            Token::OpenBrace | Token::OpenBracket if depth >= 32 => return None,
            Token::OpenBrace | Token::OpenBracket => depth += 1,
            Token::CloseBrace | Token::CloseBracket => depth = depth.checked_sub(1)?,
            _ => (),
        }
    }
    let config = jsonc_parser::parse_to_serde_value(
        body,
        &jsonc_parser::ParseOptions {
            allow_loose_object_property_names: false,
            ..Default::default()
        },
    )
    .ok()??;
    if config.get("extends").is_some() {
        return None;
    }
    serde_json::from_value(config.get("compilerOptions")?.clone()).ok()
}

type Configs = BTreeMap<String, Option<Options>>;

fn target<'a>(
    index: &'a Index,
    configs: &Configs,
    importer: &str,
    source: &str,
) -> Option<&'a str> {
    if let Some(name) = relative(importer, source) {
        return candidate(index, importer, name).ok().flatten();
    }
    if source.starts_with(['.', '/']) || source.contains([':', '?', '%', '\\']) {
        return None;
    }
    for dir in Path::new(importer).parent()?.ancestors() {
        let config_path = dir.join("tsconfig.json");
        let key = config_path.to_str()?.replace('\\', "/");
        let Some(config) = configs.get(&key) else {
            // A closer ignored or unreadable config shadows the parent.
            match std::fs::symlink_metadata(index.root.join(config_path)) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                _ => return None,
            }
        };
        let options = config.as_ref()?;
        let (values, capture) = if let Some(values) = options.paths.get(source) {
            (values, "")
        } else {
            let mut best = None;
            let mut tied = false;
            for (pattern, values) in &options.paths {
                let Some((prefix, suffix)) = pattern.split_once('*') else {
                    continue;
                };
                if suffix.contains('*') {
                    continue;
                }
                let Some(mid) = source
                    .strip_prefix(prefix)
                    .and_then(|s| s.strip_suffix(suffix))
                else {
                    continue;
                };
                match best {
                    Some((length, _, _)) if length > prefix.len() => continue,
                    Some((length, _, _)) if length == prefix.len() => tied = true,
                    _ => {
                        best = Some((prefix.len(), values, mid));
                        tied = false;
                    }
                }
            }
            if tied {
                return None;
            }
            let (_, values, mid) = best?;
            (values, mid)
        };
        let base = options.base_url.as_deref().unwrap_or(".");
        if base.starts_with('/') || base.contains([':', '\\']) {
            return None;
        }
        for value in values {
            if value.starts_with('/')
                || value.contains([':', '\\'])
                || value.matches('*').count() > 1
            {
                return None;
            }
            let mapped = value.replacen('*', capture, 1);
            let name = relative(&key, &format!("./{base}/{mapped}"))?;
            if let Some(path) = candidate(index, importer, name).ok()? {
                return Some(path);
            }
        }
        return None;
    }
    None
}

#[derive(Clone)]
enum Resolution<'a> {
    Found(Symbol<'a>, Vec<&'a str>),
    Missing,
    Unknown,
    Ambiguous(Symbol<'a>, Symbol<'a>),
}
struct Graph<'a> {
    modules: Modules<'a>,
    targets: BTreeMap<(&'a str, &'a str), &'a str>,
    cache: std::cell::RefCell<BTreeMap<Symbol<'a>, Resolution<'a>>>,
}
impl<'a> Graph<'a> {
    fn walk(
        &self,
        path: &'a str,
        name: &'a str,
        stack: &mut Vec<Symbol<'a>>,
        budget: &mut usize,
    ) -> Resolution<'a> {
        use Resolution::*;
        if stack.contains(&(path, name)) {
            return Missing;
        }
        if stack.len() >= 32 || *budget == 0 {
            return Unknown;
        }
        *budget -= 1;
        let Some(m) = self.modules.get(path) else {
            return Unknown;
        };
        stack.push((path, name));
        let mut edge = |src: &'a str, remote: &'a str| {
            let Some(next) = self.targets.get(&(path, src)) else {
                return Unknown;
            };
            self.walk(next, remote, stack, budget)
        };
        let result = match m.exports.get(name) {
            Some(Export::Local(local)) if m.definitions.contains_key(local) => {
                Found((path, local), vec![])
            }
            Some(Export::Local(local)) => match m.imports.get(local) {
                Some(i) if i.exported.is_some() => edge(&i.source, i.exported.as_deref().unwrap()),
                _ => Unknown,
            },
            Some(Export::Forward(src, remote)) => edge(src, remote),
            Some(Export::Unsupported) => Unknown,
            None => {
                let mut result = Missing;
                if name != "default" {
                    for source in &m.stars {
                        match edge(source, name) {
                            Missing => (),
                            Found(sym, chain) => match &result {
                                Found(existing, _) if *existing == sym => (),
                                Found(existing, _) => {
                                    result = Ambiguous(*existing, sym);
                                    break;
                                }
                                Missing => result = Found(sym, chain),
                                _ => (),
                            },
                            other => {
                                result = other;
                                break;
                            }
                        }
                    }
                }
                result
            }
        };
        stack.pop();
        match result {
            Found(sym, mut chain) => {
                chain.insert(0, path);
                Found(sym, chain)
            }
            other => other,
        }
    }
    fn exported(&self, path: &'a str, name: &'a str) -> Resolution<'a> {
        if let Some(cached) = self.cache.borrow().get(&(path, name)) {
            return cached.clone();
        }
        let result = self.walk(path, name, &mut Vec::new(), &mut 256);
        self.cache.borrow_mut().insert((path, name), result.clone());
        result
    }
    fn imported(&self, path: &'a str, i: &'a Import, name: &'a str) -> Resolution<'a> {
        self.targets
            .get(&(path, i.source.as_str()))
            .map_or(Resolution::Unknown, |next| self.exported(next, name))
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
        cache: Default::default(),
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
    let configs: Configs = index
        .files
        .iter()
        .filter(|(p, _)| {
            Path::new(p)
                .file_name()
                .is_some_and(|n| n == "tsconfig.json")
        })
        .map(|(p, f)| (p.clone(), paths_config(&f.body)))
        .collect();
    for (&file, m) in &graph.modules {
        for src in m.imports.values().map(|i| &i.source).chain(&m.stars).chain(
            m.exports.values().filter_map(|e| {
                if let Export::Forward(src, _) = e {
                    Some(src)
                } else {
                    None
                }
            }),
        ) {
            if let Some(found) = target(index, &configs, file, src) {
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
            if let Resolution::Found(sym, _) = graph.exported(file, name) {
                selected.insert(sym);
            }
        }
        if let Some(i) = m.imports.get(query) {
            if let Some(exported) = &i.exported {
                if let Resolution::Found(sym, _) = graph.imported(file, i, exported) {
                    selected.insert(sym);
                }
            }
        }
    }
    selected
        .retain(|(file, _)| path.is_empty() || path == "." || Path::new(file).starts_with(path));
    let mut rows = Vec::new();
    let mut ambiguous = BTreeSet::new();
    for &(file, name) in &selected {
        rows.push((
            (file, name),
            0u8,
            file,
            graph.modules[file].definitions[name],
            name,
            vec![file],
        ));
    }
    for (&file, m) in &graph.modules {
        let mut namespaces = BTreeSet::new();
        for (alias, i) in &m.imports {
            if let Some(name) = &i.exported {
                match graph.imported(file, i, name) {
                    Resolution::Found(sym, chain) if selected.contains(&sym) => {
                        rows.push((sym, 1, file, i.offset, alias.as_str(), chain));
                    }
                    Resolution::Ambiguous(a, b) if query == alias || query == name => {
                        ambiguous.insert((file, i.source.as_str(), name.as_str(), a, b));
                    }
                    _ => (),
                }
            }
        }
        for call in &m.calls {
            let sym = if let Some(i) = m.imports.get(&call.local) {
                let remote = match (&i.exported, &call.member) {
                    (Some(n), None) | (None, Some(n)) => n,
                    _ => continue,
                };
                let (sym, chain) = match graph.imported(file, i, remote) {
                    Resolution::Found(sym, chain) => (sym, chain),
                    Resolution::Ambiguous(a, b) if query == remote => {
                        ambiguous.insert((file, i.source.as_str(), remote.as_str(), a, b));
                        continue;
                    }
                    _ => continue,
                };
                if selected.contains(&sym)
                    && i.exported.is_none()
                    && namespaces.insert((sym, i.offset))
                {
                    rows.push((sym, 1, file, i.offset, call.local.as_str(), chain.clone()));
                }
                Some((sym, chain))
            } else if call.member.is_none() {
                Some(((file, call.local.as_str()), Vec::new()))
            } else {
                None
            };
            if let Some((sym, chain)) = sym.filter(|(s, _)| selected.contains(s)) {
                rows.push((
                    sym,
                    2,
                    file,
                    call.offset,
                    call.member.as_ref().unwrap_or(&call.local).as_str(),
                    chain,
                ));
            }
        }
    }
    rows.sort_unstable();
    let total = rows.len();
    ensure!(offset <= total, "Offset exceeds results");
    let matches=rows.into_iter().skip(offset).take(limit).map(|((file,symbol),role,at,byte,local,chain)|{
        let file_data=&index.files[at];
        let starts=&graph.modules[at].lines;
        let line=starts.partition_point(|&p|p<=byte as usize);
        let text=file_data.body[starts[line-1]..starts.get(line).copied().unwrap_or(file_data.body.len())].trim_end_matches(['\r','\n','\u{2028}','\u{2029}']);
        let module_chain:Vec<_>=std::iter::once(at).chain(chain.into_iter().filter(|p|*p!=at)).collect();
        json!({"module_chain":module_chain,"role":(["definition","import","call"][role as usize]),"definition_path":file,"symbol":symbol,"local_name":local,"path":at,"line":line,"byte_offset":byte,"text":text.chars().take(512).collect::<String>(),"text_truncated":text.chars().count()>512,"sha256":file_data.hash})
    }).collect::<Vec<_>>();
    Ok(
        json!({"ok":true,"kind":"bindings","query":query,"matches":matches,"total_matches":total,"next_offset":(offset+matches.len()<total).then_some(offset+matches.len()),"workspace_fingerprint":index.fingerprint,"coverage":index.coverage,"skipped_semantic_files":skipped,"untrusted_source_data":true,"ambiguous_imports":ambiguous.iter().take(40).map(|(file,source,name,a,b)| json!({"path":file,"source":source,"symbol":name,"conflicting_definitions":[a,b]})).collect::<Vec<_>>(),"ambiguous_import_count":ambiguous.len(),"limits":"Static top-level function bindings in indexed JS/TS modules. Local ESM imports, named and star re-exports, and exact/single-wildcard paths from the nearest indexed tsconfig.json. Inherited configs, external packages, CommonJS and dynamic dispatch are outside coverage. Unknown star branches do not establish unique resolution. Ambiguous extensionless targets are omitted."}),
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
    fn config_alias_to_star_barrel_reports_exact_lines_and_module_chain() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src/pricing")).unwrap();
        fs::write(root.join("tsconfig.json"), r#"{// local paths
          "compilerOptions":{"baseUrl":"src","paths":{"@/*":["missing/*","*"],"@/pricing/*":["pricing/*"],},},}"#).unwrap();
        fs::write(
            root.join("src/pricing/index.ts"),
            "export * from './quote';",
        )
        .unwrap();
        fs::write(
            root.join("src/pricing/quote.ts"),
            "// source\nexport function formatQuote(){}",
        )
        .unwrap();
        fs::write(
            root.join("ui.ts"),
            "import {formatQuote as show} from '@/pricing';\n\nshow();",
        )
        .unwrap();
        let mut index = Index::default();
        let res = trace(&mut index, root, "formatQuote", "");
        assert_eq!(res["total_matches"], 3, "{res}");
        assert_eq!(res["matches"][0]["line"], 2);
        let call = &res["matches"][2];
        assert_eq!(call["role"], "call");
        assert_eq!(call["line"], 3);
        assert_eq!(
            call["module_chain"],
            json!(["ui.ts", "src/pricing/index.ts", "src/pricing/quote.ts"])
        );
        fs::write(
            root.join("tsconfig.json"),
            r#"{"compilerOptions":{"paths":{"@/pricing":["missing"]}}}"#,
        )
        .unwrap();
        assert_eq!(
            trace(&mut index, root, "formatQuote", "")["total_matches"],
            1
        );
    }

    #[test]
    fn stars_resolve_cycles_and_diamonds_and_refuse_conflicts_or_unknown_branches() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (file, body) in [
            ("ui.js", "import {f} from './index.js'; f();"),
            ("one.js", "export function f(){}"),
            ("two.js", "export function f(){}"),
            (
                "left.js",
                "export * from './index.js'; export * from './one.js';",
            ),
            ("right.js", "export {f} from './one.js';"),
            (
                "index.js",
                "export * from './left.js'; export * from './right.js';",
            ),
        ] {
            fs::write(root.join(file), body).unwrap();
        }
        let mut index = Index::default();
        assert_eq!(trace(&mut index, root, "f", "one.js")["total_matches"], 3);
        fs::write(root.join("right.js"), "export * from './two.js';").unwrap();
        let conflict = trace(&mut index, root, "f", "");
        assert_eq!(conflict["total_matches"], 2);
        assert_eq!(conflict["ambiguous_imports"][0]["path"], "ui.js");
        fs::write(
            root.join("index.js"),
            "export * from './left.js'; export {f} from './two.js';",
        )
        .unwrap();
        assert_eq!(trace(&mut index, root, "f", "two.js")["total_matches"], 3);
        fs::write(
            root.join("index.js"),
            "export * from './one.js'; export * from 'unindexed';",
        )
        .unwrap();
        assert_eq!(trace(&mut index, root, "f", "one.js")["total_matches"], 1);
        fs::write(root.join("one.js"), "export default function f(){}").unwrap();
        fs::write(root.join("ui.js"), "import f from './index.js'; f();").unwrap();
        fs::write(root.join("index.js"), "export * from './one.js';").unwrap();
        assert_eq!(trace(&mut index, root, "f", "one.js")["total_matches"], 1);
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
