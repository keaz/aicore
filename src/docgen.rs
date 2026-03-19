use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::ast::BinOp;
use crate::driver::FrontendOutput;
use crate::ir;
use crate::std_policy::find_deprecated_api;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocFormat {
    All,
    Html,
    Markdown,
    Json,
}

impl DocFormat {
    fn wants_markdown(self) -> bool {
        matches!(self, DocFormat::All | DocFormat::Markdown)
    }

    fn wants_html(self) -> bool {
        matches!(self, DocFormat::All | DocFormat::Html)
    }

    fn wants_json(self) -> bool {
        matches!(self, DocFormat::All | DocFormat::Json)
    }
}

#[derive(Debug, Clone)]
pub struct DocOutput {
    pub output_dir: PathBuf,
    pub index_path: Option<PathBuf>,
    pub html_path: Option<PathBuf>,
    pub api_json_path: Option<PathBuf>,
    pub primary_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct DocIndex {
    schema_version: u32,
    modules: Vec<DocModule>,
}

#[derive(Debug, Clone, Serialize)]
struct DocModule {
    module: String,
    items: Vec<DocItem>,
}

#[derive(Debug, Clone, Serialize)]
struct DocItem {
    kind: String,
    name: String,
    signature: String,
    summary: Option<String>,
    doc: Option<String>,
    examples: Vec<String>,
    effects: Vec<String>,
    requires: Option<String>,
    ensures: Option<String>,
    invariant: Option<String>,
    return_type: Option<String>,
    return_type_link: Option<String>,
    source_path: Option<String>,
    source_line: Option<usize>,
    deprecated: Option<DocDeprecated>,
}

#[derive(Debug, Clone, Serialize)]
struct DocDeprecated {
    replacement: String,
    since: String,
    note: String,
}

#[derive(Debug, Clone, Default)]
struct SourceDocMeta {
    doc: Option<String>,
    summary: Option<String>,
    examples: Vec<String>,
    source_path: String,
    source_line: usize,
}

pub fn generate_docs(
    front: &FrontendOutput,
    output_dir: &Path,
    input_path: &Path,
    format: DocFormat,
) -> anyhow::Result<DocOutput> {
    fs::create_dir_all(output_dir)?;

    let mut type_map = BTreeMap::new();
    for ty in &front.ir.types {
        type_map.insert(ty.id, ty.repr.clone());
    }
    let source_meta = collect_source_doc_meta(input_path)?;

    let mut modules = BTreeMap::<String, Vec<DocItem>>::new();

    for (idx, item) in front.ir.items.iter().enumerate() {
        let module = front
            .item_modules
            .get(idx)
            .and_then(|value| value.clone())
            .map(|parts| parts.join("."))
            .or_else(|| front.ir.module.as_ref().map(|module| module.join(".")))
            .unwrap_or_else(|| "<entry>".to_string());

        let doc_item = match item {
            ir::Item::Function(func) => {
                let ret_type = type_map
                    .get(&func.ret_type)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string());
                let effects = func.effects.clone();
                let requires = func.requires.as_ref().map(render_expr);
                let ensures = func.ensures.as_ref().map(render_expr);
                let deprecated =
                    find_deprecated_api(&module, &func.name).map(|entry| DocDeprecated {
                        replacement: entry.replacement.to_string(),
                        since: entry.since.to_string(),
                        note: entry.note.to_string(),
                    });
                let meta = source_meta
                    .get(&(module.clone(), "fn".to_string(), func.name.clone()))
                    .cloned()
                    .unwrap_or_default();
                DocItem {
                    kind: "fn".to_string(),
                    name: func.name.clone(),
                    signature: render_function_signature(func, &type_map),
                    summary: meta.summary,
                    doc: meta.doc,
                    examples: meta.examples,
                    effects,
                    requires,
                    ensures,
                    invariant: None,
                    return_type: Some(ret_type),
                    return_type_link: None,
                    source_path: if meta.source_path.is_empty() {
                        None
                    } else {
                        Some(meta.source_path)
                    },
                    source_line: if meta.source_line == 0 {
                        None
                    } else {
                        Some(meta.source_line)
                    },
                    deprecated,
                }
            }
            ir::Item::Struct(strukt) => build_non_function_doc_item(
                &source_meta,
                &module,
                "struct",
                &strukt.name,
                render_struct_signature(strukt, &type_map),
                strukt.invariant.as_ref().map(render_expr),
            ),
            ir::Item::Enum(enm) => build_non_function_doc_item(
                &source_meta,
                &module,
                "enum",
                &enm.name,
                render_enum_signature(enm, &type_map),
                None,
            ),
            ir::Item::Trait(trait_def) => build_non_function_doc_item(
                &source_meta,
                &module,
                "trait",
                &trait_def.name,
                render_trait_signature(trait_def, &type_map),
                None,
            ),
            ir::Item::Impl(impl_def) => build_non_function_doc_item(
                &source_meta,
                &module,
                "impl",
                &impl_def.trait_name,
                render_impl_signature(impl_def, &type_map),
                None,
            ),
        };

        modules.entry(module).or_default().push(doc_item);
    }

    for items in modules.values_mut() {
        items.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    }

    let mut module_docs = modules
        .into_iter()
        .map(|(module, items)| DocModule { module, items })
        .collect::<Vec<_>>();
    module_docs.sort_by(|a, b| a.module.cmp(&b.module));

    let known_types = module_docs
        .iter()
        .flat_map(|module| module.items.iter())
        .filter(|item| item.kind == "struct" || item.kind == "enum" || item.kind == "trait")
        .map(|item| item.name.clone())
        .collect::<BTreeSet<_>>();
    for module in &mut module_docs {
        for item in &mut module.items {
            if let Some(ret_ty) = &item.return_type {
                let base = base_type_name(ret_ty);
                if known_types.contains(base) {
                    item.return_type_link = Some(format!("#type-{}", anchor_token(base)));
                }
            }
        }
    }

    let doc_index = DocIndex {
        schema_version: 1,
        modules: module_docs,
    };

    let mut index_path = None;
    let mut html_path = None;
    let mut api_json_path = None;

    if format.wants_markdown() {
        let md_path = output_dir.join("index.md");
        fs::write(&md_path, render_markdown(&doc_index))?;
        index_path = Some(md_path);
    }
    if format.wants_html() {
        let html = output_dir.join("index.html");
        fs::write(&html, render_html(&doc_index))?;
        html_path = Some(html);
    }
    if format.wants_json() {
        let json_path = output_dir.join("api.json");
        fs::write(
            &json_path,
            format!("{}\n", serde_json::to_string_pretty(&doc_index)?),
        )?;
        api_json_path = Some(json_path);
    }

    let primary_path = match format {
        DocFormat::Html => html_path
            .clone()
            .unwrap_or_else(|| output_dir.join("index.html")),
        DocFormat::Markdown => index_path
            .clone()
            .unwrap_or_else(|| output_dir.join("index.md")),
        DocFormat::Json => api_json_path
            .clone()
            .unwrap_or_else(|| output_dir.join("api.json")),
        DocFormat::All => html_path
            .clone()
            .or_else(|| index_path.clone())
            .or_else(|| api_json_path.clone())
            .unwrap_or_else(|| output_dir.join("index.html")),
    };

    Ok(DocOutput {
        output_dir: output_dir.to_path_buf(),
        index_path,
        html_path,
        api_json_path,
        primary_path,
    })
}

fn render_markdown(index: &DocIndex) -> String {
    let mut out = String::new();
    out.push_str("# AIC API Documentation\n\n");
    for module in &index.modules {
        out.push_str(&format!("## {}\n\n", module.module));
        for item in &module.items {
            let anchor = item_anchor(item);
            out.push_str(&format!("<a id=\"{anchor}\"></a>\n"));
            out.push_str(&format!("### {} {}\n\n", item.kind, item.name));
            out.push_str("```aic\n");
            out.push_str(&item.signature);
            out.push_str("\n```\n\n");
            if let Some(summary) = &item.summary {
                out.push_str(summary);
                out.push_str("\n\n");
            }
            if let Some(doc) = &item.doc {
                out.push_str(doc);
                out.push_str("\n\n");
            }
            if let Some(source_path) = &item.source_path {
                if let Some(source_line) = item.source_line {
                    out.push_str(&format!("- Source: `{}:{}`\n", source_path, source_line));
                } else {
                    out.push_str(&format!("- Source: `{}`\n", source_path));
                }
            }
            if let Some(ret) = &item.return_type {
                if let Some(link) = &item.return_type_link {
                    out.push_str(&format!("- Returns: [{}]({})\n", ret, link));
                } else {
                    out.push_str(&format!("- Returns: `{}`\n", ret));
                }
            }
            if !item.effects.is_empty() {
                out.push_str(&format!("- Effects: `{}`\n", item.effects.join(", ")));
            }
            if let Some(requires) = &item.requires {
                out.push_str(&format!("- Requires: `{}`\n", requires));
            }
            if let Some(ensures) = &item.ensures {
                out.push_str(&format!("- Ensures: `{}`\n", ensures));
            }
            if let Some(invariant) = &item.invariant {
                out.push_str(&format!("- Invariant: `{}`\n", invariant));
            }
            if let Some(deprecated) = &item.deprecated {
                out.push_str(&format!(
                    "- Deprecated: since `{}`; use `{}` ({})\n",
                    deprecated.since, deprecated.replacement, deprecated.note
                ));
            }
            if !item.examples.is_empty() {
                out.push_str("- Examples:\n");
                for example in &item.examples {
                    out.push_str("```aic\n");
                    out.push_str(example);
                    out.push_str("\n```\n");
                }
            }
            out.push('\n');
        }
    }
    out
}

fn build_non_function_doc_item(
    source_meta: &BTreeMap<(String, String, String), SourceDocMeta>,
    module: &str,
    kind: &str,
    name: &str,
    signature: String,
    invariant: Option<String>,
) -> DocItem {
    let meta = source_meta
        .get(&(module.to_string(), kind.to_string(), name.to_string()))
        .cloned()
        .unwrap_or_default();
    DocItem {
        kind: kind.to_string(),
        name: name.to_string(),
        signature,
        summary: meta.summary,
        doc: meta.doc,
        examples: meta.examples,
        effects: Vec::new(),
        requires: None,
        ensures: None,
        invariant,
        return_type: None,
        return_type_link: None,
        source_path: if meta.source_path.is_empty() {
            None
        } else {
            Some(meta.source_path)
        },
        source_line: if meta.source_line == 0 {
            None
        } else {
            Some(meta.source_line)
        },
        deprecated: None,
    }
}

fn collect_source_doc_meta(
    input_path: &Path,
) -> anyhow::Result<BTreeMap<(String, String, String), SourceDocMeta>> {
    let mut files = Vec::new();
    let root = doc_source_root(input_path);
    collect_aic_files(&root, &mut files)?;
    files.sort();

    let mut meta = BTreeMap::<(String, String, String), SourceDocMeta>::new();
    for file in files {
        let source = match fs::read_to_string(&file) {
            Ok(source) => source,
            Err(_) => continue,
        };
        let (program, diagnostics) = crate::parser::parse(&source, &file.to_string_lossy());
        if diagnostics.iter().any(|d| d.is_error()) {
            continue;
        }
        let Some(program) = program else {
            continue;
        };
        let module = program
            .module
            .as_ref()
            .map(|module| module.path.join("."))
            .unwrap_or_else(|| "<entry>".to_string());
        let docs_by_line = collect_decl_doc_map_by_line(&source);

        for item in program.items {
            let (kind, name, span_start) = match item {
                crate::ast::Item::Function(func) => ("fn".to_string(), func.name, func.span.start),
                crate::ast::Item::Struct(strukt) => {
                    ("struct".to_string(), strukt.name, strukt.span.start)
                }
                crate::ast::Item::Enum(enm) => ("enum".to_string(), enm.name, enm.span.start),
                crate::ast::Item::Trait(trait_def) => {
                    ("trait".to_string(), trait_def.name, trait_def.span.start)
                }
                crate::ast::Item::Impl(impl_def) => {
                    ("impl".to_string(), impl_def.trait_name, impl_def.span.start)
                }
            };
            let line = offset_to_line_number(&source, span_start);
            let doc = docs_by_line.get(&line).cloned();
            let summary = doc
                .as_ref()
                .and_then(|text| text.lines().find(|line| !line.trim().is_empty()))
                .map(|line| line.trim().to_string());
            let examples = doc
                .as_ref()
                .map(|text| extract_doc_examples(text))
                .unwrap_or_default();

            meta.insert(
                (module.clone(), kind, name),
                SourceDocMeta {
                    doc,
                    summary,
                    examples,
                    source_path: file.to_string_lossy().to_string(),
                    source_line: line,
                },
            );
        }
    }

    Ok(meta)
}

fn doc_source_root(input_path: &Path) -> PathBuf {
    if input_path.is_dir() {
        input_path.to_path_buf()
    } else {
        input_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| input_path.to_path_buf())
    }
}

fn collect_aic_files(root: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if root.is_file() {
        if root.extension().and_then(|ext| ext.to_str()) == Some("aic") {
            out.push(root.to_path_buf());
        }
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_aic_files(&path, out)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("aic") {
            out.push(path);
        }
    }
    Ok(())
}

fn collect_decl_doc_map_by_line(source: &str) -> BTreeMap<usize, String> {
    let mut map = BTreeMap::new();
    let mut pending = Vec::<String>::new();

    for (index, raw_line) in source.lines().enumerate() {
        let trimmed = raw_line.trim_start();
        if let Some(doc) = trimmed.strip_prefix("///") {
            pending.push(doc.trim_start().to_string());
            continue;
        }
        if trimmed.starts_with("#[") {
            continue;
        }
        if is_declaration_line(trimmed) {
            if !pending.is_empty() {
                map.insert(index + 1, pending.join("\n"));
                pending.clear();
            }
            continue;
        }
        pending.clear();
    }

    map
}

fn is_declaration_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let stripped = if let Some(rest) = trimmed.strip_prefix("pub(crate)") {
        rest.trim_start()
    } else if let Some(rest) = trimmed.strip_prefix("pub") {
        rest.trim_start()
    } else {
        trimmed
    };
    stripped.starts_with("fn ")
        || stripped.starts_with("async fn ")
        || stripped.starts_with("unsafe fn ")
        || stripped.starts_with("intrinsic fn ")
        || stripped.starts_with("extern ")
        || stripped.starts_with("struct ")
        || stripped.starts_with("enum ")
        || stripped.starts_with("trait ")
        || stripped.starts_with("impl ")
        || stripped.starts_with("module ")
        || stripped.starts_with("const ")
        || stripped.starts_with("type ")
}

fn offset_to_line_number(source: &str, offset: usize) -> usize {
    let upto = source.len().min(offset);
    source[..upto].bytes().filter(|byte| *byte == b'\n').count() + 1
}

fn extract_doc_examples(doc: &str) -> Vec<String> {
    let mut examples = Vec::new();
    let mut in_block = false;
    let mut current = Vec::<String>::new();
    for line in doc.lines() {
        let trimmed = line.trim_start();
        if !in_block && (trimmed.starts_with("```aic") || trimmed == "```") {
            in_block = true;
            current.clear();
            continue;
        }
        if in_block && trimmed.starts_with("```") {
            let example = current.join("\n").trim().to_string();
            if !example.is_empty() {
                examples.push(example);
            }
            in_block = false;
            current.clear();
            continue;
        }
        if in_block {
            current.push(line.to_string());
        }
    }
    examples
}

fn base_type_name(ty: &str) -> &str {
    ty.split_once('[')
        .map(|(base, _)| base.trim())
        .unwrap_or_else(|| ty.trim())
}

fn anchor_token(raw: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn item_anchor(item: &DocItem) -> String {
    if item.kind == "struct" || item.kind == "enum" || item.kind == "trait" {
        return format!("type-{}", anchor_token(&item.name));
    }
    format!(
        "item-{}-{}",
        anchor_token(&item.kind),
        anchor_token(&item.name)
    )
}

fn render_html(index: &DocIndex) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html><html><head><meta charset=\"utf-8\"/>");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/>");
    out.push_str("<title>AIC API Documentation</title>");
    out.push_str("<style>body{font-family:ui-sans-serif,system-ui,sans-serif;margin:2rem;line-height:1.4;}code,pre{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;}pre{background:#f6f8fa;padding:0.75rem;border-radius:6px;overflow:auto;}article{border:1px solid #e5e7eb;border-radius:8px;padding:1rem;margin:0.75rem 0;}input{width:100%;max-width:32rem;padding:0.5rem;margin:0.75rem 0;}h2{margin-top:2rem;}</style>");
    out.push_str("</head><body>");
    out.push_str("<h1>AIC API Documentation</h1>");
    out.push_str("<p><input id=\"searchBox\" type=\"search\" placeholder=\"Search modules, symbols, signatures, docs...\" /></p>");
    for module in &index.modules {
        out.push_str(&format!(
            "<section><h2>{}</h2>",
            escape_html(&module.module)
        ));
        for item in &module.items {
            let search_blob = format!(
                "{} {} {} {}",
                item.kind,
                item.name,
                item.signature,
                item.doc.clone().unwrap_or_default()
            );
            let anchor = item_anchor(item);
            out.push_str(&format!(
                "<article id=\"{}\" class=\"doc-item\" data-search=\"{}\">",
                escape_html_attr(&anchor),
                escape_html_attr(&search_blob.to_ascii_lowercase())
            ));
            out.push_str(&format!(
                "<h3>{} {}</h3>",
                escape_html(&item.kind),
                escape_html(&item.name)
            ));
            out.push_str("<pre><code>");
            out.push_str(&escape_html(&item.signature));
            out.push_str("</code></pre>");
            if let Some(summary) = &item.summary {
                out.push_str(&format!("<p>{}</p>", escape_html(summary)));
            }
            if let Some(doc) = &item.doc {
                out.push_str("<pre><code>");
                out.push_str(&escape_html(doc));
                out.push_str("</code></pre>");
            }
            if let Some(source_path) = &item.source_path {
                if let Some(source_line) = item.source_line {
                    out.push_str(&format!(
                        "<p>Source: <a href=\"{}#L{}\">{}:{}</a></p>",
                        escape_html_attr(source_path),
                        source_line,
                        escape_html(source_path),
                        source_line
                    ));
                } else {
                    out.push_str(&format!(
                        "<p>Source: <a href=\"{}\">{}</a></p>",
                        escape_html_attr(source_path),
                        escape_html(source_path)
                    ));
                }
            }
            if let Some(ret) = &item.return_type {
                if let Some(link) = &item.return_type_link {
                    out.push_str(&format!(
                        "<p>Returns: <a href=\"{}\">{}</a></p>",
                        escape_html_attr(link),
                        escape_html(ret)
                    ));
                } else {
                    out.push_str(&format!(
                        "<p>Returns: <code>{}</code></p>",
                        escape_html(ret)
                    ));
                }
            }
            if !item.effects.is_empty() {
                out.push_str(&format!(
                    "<p>Effects: <code>{}</code></p>",
                    escape_html(&item.effects.join(", "))
                ));
            }
            if let Some(requires) = &item.requires {
                out.push_str(&format!(
                    "<p>Requires: <code>{}</code></p>",
                    escape_html(requires)
                ));
            }
            if let Some(ensures) = &item.ensures {
                out.push_str(&format!(
                    "<p>Ensures: <code>{}</code></p>",
                    escape_html(ensures)
                ));
            }
            if let Some(invariant) = &item.invariant {
                out.push_str(&format!(
                    "<p>Invariant: <code>{}</code></p>",
                    escape_html(invariant)
                ));
            }
            if !item.examples.is_empty() {
                out.push_str("<h4>Examples</h4>");
                for example in &item.examples {
                    out.push_str("<pre><code class=\"language-aic\">");
                    out.push_str(&escape_html(example));
                    out.push_str("</code></pre>");
                }
            }
            out.push_str("</article>");
        }
        out.push_str("</section>");
    }
    out.push_str("<script>const box=document.getElementById('searchBox');const items=[...document.querySelectorAll('.doc-item')];box?.addEventListener('input',()=>{const q=(box.value||'').toLowerCase().trim();for(const item of items){const hay=(item.dataset.search||'');item.style.display=(!q||hay.includes(q))?'block':'none';}});</script>");
    out.push_str("</body></html>");
    out
}

fn escape_html(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attr(raw: &str) -> String {
    escape_html(raw).replace('"', "&quot;")
}

fn render_function_signature(func: &ir::Function, types: &BTreeMap<ir::TypeId, String>) -> String {
    let generics = render_generic_params(&func.generics);

    let params = func
        .params
        .iter()
        .map(|param| {
            let ty = types
                .get(&param.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            format!("{}: {}", param.name, ty)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let ret = types
        .get(&func.ret_type)
        .cloned()
        .unwrap_or_else(|| "<?>".to_string());

    let effects = if func.effects.is_empty() {
        String::new()
    } else {
        format!(" effects {{ {} }}", func.effects.join(", "))
    };
    let capabilities = if func.capabilities.is_empty() {
        String::new()
    } else {
        format!(" capabilities {{ {} }}", func.capabilities.join(", "))
    };

    let async_prefix = if func.is_async { "async " } else { "" };
    format!(
        "{}fn {}{}({}) -> {}{}{}",
        async_prefix, func.name, generics, params, ret, effects, capabilities
    )
}

fn render_struct_signature(strukt: &ir::StructDef, types: &BTreeMap<ir::TypeId, String>) -> String {
    let generics = render_generic_params(&strukt.generics);

    let fields = strukt
        .fields
        .iter()
        .map(|field| {
            let ty = types
                .get(&field.ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            format!("{}: {}", field.name, ty)
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("struct {}{} {{ {} }}", strukt.name, generics, fields)
}

fn render_enum_signature(enm: &ir::EnumDef, types: &BTreeMap<ir::TypeId, String>) -> String {
    let generics = render_generic_params(&enm.generics);

    let variants = enm
        .variants
        .iter()
        .map(|variant| {
            if let Some(payload) = variant.payload {
                let ty = types
                    .get(&payload)
                    .cloned()
                    .unwrap_or_else(|| "<?>".to_string());
                format!("{}({})", variant.name, ty)
            } else {
                variant.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("enum {}{} {{ {} }}", enm.name, generics, variants)
}

fn render_trait_signature(
    trait_def: &ir::TraitDef,
    types: &BTreeMap<ir::TypeId, String>,
) -> String {
    let generics = render_generic_params(&trait_def.generics);
    if trait_def.methods.is_empty() {
        return format!("trait {}{};", trait_def.name, generics);
    }
    let methods = trait_def
        .methods
        .iter()
        .map(|method| format!("{};", render_function_signature(method, types)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("trait {}{} {{ {} }}", trait_def.name, generics, methods)
}

fn render_impl_signature(impl_def: &ir::ImplDef, types: &BTreeMap<ir::TypeId, String>) -> String {
    let methods = impl_def
        .methods
        .iter()
        .map(|method| format!("{};", render_function_signature(method, types)))
        .collect::<Vec<_>>()
        .join(" ");
    if impl_def.is_inherent {
        let target = impl_def
            .target
            .and_then(|ty| types.get(&ty).cloned())
            .unwrap_or_else(|| impl_def.trait_name.clone());
        if methods.is_empty() {
            return format!("impl {} {{}}", target);
        }
        return format!("impl {} {{ {} }}", target, methods);
    }

    let args = impl_def
        .trait_args
        .iter()
        .map(|ty| types.get(ty).cloned().unwrap_or_else(|| "<?>".to_string()))
        .collect::<Vec<_>>()
        .join(", ");
    if methods.is_empty() {
        return format!("impl {}[{}];", impl_def.trait_name, args);
    }
    format!("impl {}[{}] {{ {} }}", impl_def.trait_name, args, methods)
}

fn render_generic_params(generics: &[ir::GenericParam]) -> String {
    if generics.is_empty() {
        return String::new();
    }
    format!(
        "[{}]",
        generics
            .iter()
            .map(|g| {
                if g.bounds.is_empty() {
                    g.name.clone()
                } else {
                    format!("{}: {}", g.name, g.bounds.join(" + "))
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn render_expr(expr: &ir::Expr) -> String {
    match &expr.kind {
        ir::ExprKind::Int(v) => v.to_string(),
        ir::ExprKind::Float(v) => render_float_literal(*v),
        ir::ExprKind::Bool(v) => v.to_string(),
        ir::ExprKind::Char(v) => format!("{:?}", v),
        ir::ExprKind::String(s) => format!("\"{}\"", s),
        ir::ExprKind::Unit => "()".to_string(),
        ir::ExprKind::Var(name) => name.clone(),
        ir::ExprKind::Call {
            callee,
            args,
            arg_names,
        } => {
            let rendered_args = args
                .iter()
                .enumerate()
                .map(|(idx, arg)| {
                    if let Some(name) = arg_names.get(idx).and_then(|name| name.as_deref()) {
                        format!("{}: {}", name, render_expr(arg))
                    } else {
                        render_expr(arg)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", render_expr(callee), rendered_args)
        }
        ir::ExprKind::TemplateLiteral { template, args } => render_template_literal(template, args),
        ir::ExprKind::Closure { params, .. } => {
            let rendered = params
                .iter()
                .map(|param| param.name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            format!("|{}| -> ... {{ ... }}", rendered)
        }
        ir::ExprKind::If { cond, .. } => {
            format!("if {} {{ ... }} else {{ ... }}", render_expr(cond))
        }
        ir::ExprKind::While { cond, .. } => format!("while {} {{ ... }}", render_expr(cond)),
        ir::ExprKind::Loop { .. } => "loop { ... }".to_string(),
        ir::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                format!("break {}", render_expr(expr))
            } else {
                "break".to_string()
            }
        }
        ir::ExprKind::Continue => "continue".to_string(),
        ir::ExprKind::Match { expr, .. } => format!("match {} {{ ... }}", render_expr(expr)),
        ir::ExprKind::Binary { op, lhs, rhs } => {
            format!(
                "({} {} {})",
                render_expr(lhs),
                render_binop(*op),
                render_expr(rhs)
            )
        }
        ir::ExprKind::Unary { op, expr } => {
            let op = match op {
                crate::ast::UnaryOp::Neg => "-",
                crate::ast::UnaryOp::Not => "!",
                crate::ast::UnaryOp::BitNot => "~",
            };
            format!("{}{}", op, render_expr(expr))
        }
        ir::ExprKind::Borrow { mutable, expr } => {
            if *mutable {
                format!("&mut {}", render_expr(expr))
            } else {
                format!("&{}", render_expr(expr))
            }
        }
        ir::ExprKind::Await { expr } => format!("await {}", render_expr(expr)),
        ir::ExprKind::Try { expr } => format!("{}?", render_expr(expr)),
        ir::ExprKind::StructInit { name, fields } => {
            let rendered = fields
                .iter()
                .map(|(field, value, _)| format!("{}: {}", field, render_expr(value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {{ {} }}", name, rendered)
        }
        ir::ExprKind::FieldAccess { base, field } => format!("{}.{}", render_expr(base), field),
        ir::ExprKind::UnsafeBlock { .. } => "unsafe { ... }".to_string(),
    }
}

fn render_template_literal(template: &str, args: &[ir::Expr]) -> String {
    let mut out = String::from("f\"");
    let mut cursor = 0usize;
    for (idx, arg) in args.iter().enumerate() {
        let placeholder = format!("{{{idx}}}");
        let Some(rel) = template[cursor..].find(&placeholder) else {
            break;
        };
        let split_at = cursor + rel;
        push_escaped_template_segment(&mut out, &template[cursor..split_at]);
        out.push('{');
        out.push_str(&render_expr(arg));
        out.push('}');
        cursor = split_at + placeholder.len();
    }
    push_escaped_template_segment(&mut out, &template[cursor..]);
    out.push('"');
    out
}

fn push_escaped_template_segment(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            _ => out.push(ch),
        }
    }
}

fn render_float_literal(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f64::INFINITY {
        "inf".to_string()
    } else if value == f64::NEG_INFINITY {
        "-inf".to_string()
    } else {
        let mut text = format!("{value}");
        if !text.contains('.') && !text.contains('e') && !text.contains('E') {
            text.push_str(".0");
        }
        text
    }
}

fn render_binop(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Ushr => ">>>",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::driver::{has_errors, run_frontend};

    use super::{generate_docs, DocFormat};

    #[test]
    fn docgen_emits_signatures_effects_and_contracts() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("main.aic");
        fs::write(
            &src,
            r#"module app.main;
import std.time;

/// Absolute value with contract guarantees.
///
/// ## Example
/// ```aic
/// abs(1)
/// ```
fn abs(x: Int) -> Int effects { time } capabilities { time } requires x >= 0 ensures result >= 0 {
    x
}

fn main() -> Int effects { time } capabilities { time } {
    now();
    abs(1)
}
"#,
        )
        .expect("write source");

        let front = run_frontend(&src).expect("frontend");
        assert!(
            !has_errors(&front.diagnostics),
            "diagnostics={:#?}",
            front.diagnostics
        );

        let out = generate_docs(&front, &dir.path().join("docs/api"), &src, DocFormat::All)
            .expect("docgen");
        let index = fs::read_to_string(out.index_path.expect("index path")).expect("read index");
        assert!(index.contains("fn abs(x: Int) -> Int effects { time } capabilities { time }"));
        assert!(index.contains("Requires: `(x >= 0)`"));
        assert!(index.contains("Ensures: `(result >= 0)`"));
        assert!(index.contains("Absolute value with contract guarantees."));

        let html = fs::read_to_string(out.html_path.expect("html path")).expect("read html");
        assert!(html.contains("searchBox"));

        let json = fs::read_to_string(out.api_json_path.expect("json path")).expect("read json");
        assert!(json.contains("\"summary\""));
        assert!(json.contains("\"examples\""));
    }
}
