use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Serialize;

use crate::ast;
use crate::parser;
use crate::span::Span;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Variant,
    Trait,
    Impl,
    Module,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Variant => "variant",
            SymbolKind::Trait => "trait",
            SymbolKind::Impl => "impl",
            SymbolKind::Module => "module",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SymbolLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub span_start: usize,
    pub span_end: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SymbolRecord {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub location: SymbolLocation,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub effects: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub generics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ensures: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invariant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct QueryFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<SymbolKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_pattern: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub has_invariant: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generic_over: Option<String>,
    #[serde(default)]
    pub has_requires: bool,
    #[serde(default)]
    pub has_ensures: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QueryReport {
    pub total_symbols: usize,
    pub matched_symbols: usize,
    pub filters: QueryFilters,
    pub symbols: Vec<SymbolRecord>,
}

pub fn list_symbols(entry_path: &Path) -> anyhow::Result<Vec<SymbolRecord>> {
    let root = symbol_index_root(entry_path);
    let mut files = Vec::new();
    collect_aic_files(&root, &mut files)?;
    files.sort();

    let mut records = Vec::new();
    for file in files {
        let source = match fs::read_to_string(&file) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let (program, diagnostics) = parser::parse(&source, &file.to_string_lossy());
        if diagnostics.iter().any(|diag| diag.is_error()) {
            continue;
        }
        let Some(program) = program else {
            continue;
        };

        let module_name = program.module.as_ref().map(|module| module.path.join("."));
        if let Some(module_decl) = &program.module {
            let module_value = module_name
                .clone()
                .unwrap_or_else(|| module_decl.path.join("."));
            records.push(SymbolRecord {
                name: module_value.clone(),
                kind: SymbolKind::Module,
                signature: format!("module {module_value}"),
                module: Some(module_value),
                location: location_for_span(&file, &source, module_decl.span),
                effects: Vec::new(),
                capabilities: Vec::new(),
                generics: Vec::new(),
                requires: None,
                ensures: None,
                invariant: None,
                container: None,
            });
        }

        for item in program.items {
            match item {
                ast::Item::Function(function) => {
                    records.push(function_symbol_record(
                        &file,
                        &source,
                        &module_name,
                        &function,
                        None,
                    ));
                }
                ast::Item::Struct(strukt) => {
                    records.push(struct_symbol_record(&file, &source, &module_name, &strukt));
                }
                ast::Item::Enum(enm) => {
                    records.push(enum_symbol_record(&file, &source, &module_name, &enm));
                    for variant in &enm.variants {
                        records.push(enum_variant_symbol_record(
                            &file,
                            &source,
                            &module_name,
                            &enm,
                            variant,
                        ));
                    }
                }
                ast::Item::Trait(trait_def) => {
                    records.push(trait_symbol_record(
                        &file,
                        &source,
                        &module_name,
                        &trait_def,
                    ));
                    for method in &trait_def.methods {
                        records.push(function_symbol_record(
                            &file,
                            &source,
                            &module_name,
                            method,
                            Some(trait_def.name.clone()),
                        ));
                    }
                }
                ast::Item::Impl(impl_def) => {
                    let impl_signature = render_impl_signature(&impl_def);
                    records.push(SymbolRecord {
                        name: impl_symbol_name(&impl_def),
                        kind: SymbolKind::Impl,
                        signature: impl_signature.clone(),
                        module: module_name.clone(),
                        location: location_for_span(&file, &source, impl_def.span),
                        effects: Vec::new(),
                        capabilities: Vec::new(),
                        generics: Vec::new(),
                        requires: None,
                        ensures: None,
                        invariant: None,
                        container: None,
                    });
                    for method in &impl_def.methods {
                        records.push(function_symbol_record(
                            &file,
                            &source,
                            &module_name,
                            method,
                            Some(impl_signature.clone()),
                        ));
                    }
                }
            }
        }
    }

    records.sort_by(|lhs, rhs| {
        lhs.name
            .cmp(&rhs.name)
            .then(lhs.kind.as_str().cmp(rhs.kind.as_str()))
            .then(lhs.module.cmp(&rhs.module))
            .then(lhs.location.file.cmp(&rhs.location.file))
            .then(lhs.location.span_start.cmp(&rhs.location.span_start))
    });

    Ok(records)
}

pub fn query_symbols(entry_path: &Path, filters: QueryFilters) -> anyhow::Result<QueryReport> {
    let all_symbols = list_symbols(entry_path)?;
    let total_symbols = all_symbols.len();

    let mut matched = all_symbols
        .into_iter()
        .filter(|symbol| symbol_matches_filters(symbol, &filters))
        .collect::<Vec<_>>();

    if let Some(limit) = filters.limit {
        matched.truncate(limit);
    }

    Ok(QueryReport {
        total_symbols,
        matched_symbols: matched.len(),
        filters,
        symbols: matched,
    })
}

fn function_symbol_record(
    file: &Path,
    source: &str,
    module: &Option<String>,
    function: &ast::Function,
    container: Option<String>,
) -> SymbolRecord {
    let mut effects = function.effects.clone();
    effects.sort();
    effects.dedup();

    let mut capabilities = function.capabilities.clone();
    capabilities.sort();
    capabilities.dedup();

    let requires = function
        .requires
        .as_ref()
        .and_then(|expr| snippet_for_span(source, expr.span));
    let ensures = function
        .ensures
        .as_ref()
        .and_then(|expr| snippet_for_span(source, expr.span));

    SymbolRecord {
        name: function.name.clone(),
        kind: SymbolKind::Function,
        signature: render_function_signature(function, requires.as_deref(), ensures.as_deref()),
        module: module.clone(),
        location: location_for_span(file, source, function.span),
        effects,
        capabilities,
        generics: function
            .generics
            .iter()
            .map(|param| param.name.clone())
            .collect(),
        requires,
        ensures,
        invariant: None,
        container,
    }
}

fn struct_symbol_record(
    file: &Path,
    source: &str,
    module: &Option<String>,
    strukt: &ast::StructDef,
) -> SymbolRecord {
    let invariant = strukt
        .invariant
        .as_ref()
        .and_then(|expr| snippet_for_span(source, expr.span));

    SymbolRecord {
        name: strukt.name.clone(),
        kind: SymbolKind::Struct,
        signature: render_struct_signature(strukt, invariant.as_deref()),
        module: module.clone(),
        location: location_for_span(file, source, strukt.span),
        effects: Vec::new(),
        capabilities: Vec::new(),
        generics: strukt
            .generics
            .iter()
            .map(|param| param.name.clone())
            .collect(),
        requires: None,
        ensures: None,
        invariant,
        container: None,
    }
}

fn enum_symbol_record(
    file: &Path,
    source: &str,
    module: &Option<String>,
    enm: &ast::EnumDef,
) -> SymbolRecord {
    SymbolRecord {
        name: enm.name.clone(),
        kind: SymbolKind::Enum,
        signature: render_enum_signature(enm),
        module: module.clone(),
        location: location_for_span(file, source, enm.span),
        effects: Vec::new(),
        capabilities: Vec::new(),
        generics: enm
            .generics
            .iter()
            .map(|param| param.name.clone())
            .collect(),
        requires: None,
        ensures: None,
        invariant: None,
        container: None,
    }
}

fn enum_variant_symbol_record(
    file: &Path,
    source: &str,
    module: &Option<String>,
    enm: &ast::EnumDef,
    variant: &ast::VariantDef,
) -> SymbolRecord {
    SymbolRecord {
        name: variant.name.clone(),
        kind: SymbolKind::Variant,
        signature: render_enum_variant_signature(&enm.name, variant),
        module: module.clone(),
        location: location_for_span(file, source, variant.span),
        effects: Vec::new(),
        capabilities: Vec::new(),
        generics: Vec::new(),
        requires: None,
        ensures: None,
        invariant: None,
        container: Some(enm.name.clone()),
    }
}

fn trait_symbol_record(
    file: &Path,
    source: &str,
    module: &Option<String>,
    trait_def: &ast::TraitDef,
) -> SymbolRecord {
    SymbolRecord {
        name: trait_def.name.clone(),
        kind: SymbolKind::Trait,
        signature: render_trait_signature(trait_def),
        module: module.clone(),
        location: location_for_span(file, source, trait_def.span),
        effects: Vec::new(),
        capabilities: Vec::new(),
        generics: trait_def
            .generics
            .iter()
            .map(|param| param.name.clone())
            .collect(),
        requires: None,
        ensures: None,
        invariant: None,
        container: None,
    }
}

fn symbol_matches_filters(symbol: &SymbolRecord, filters: &QueryFilters) -> bool {
    if let Some(kind) = filters.kind {
        if symbol.kind != kind {
            return false;
        }
    }

    if let Some(pattern) = &filters.name_pattern {
        if !name_matches_pattern(pattern, &symbol.name) {
            return false;
        }
    }

    if !filters.effects.is_empty() {
        let symbol_effects = symbol
            .effects
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !filters
            .effects
            .iter()
            .all(|effect| symbol_effects.iter().any(|value| value == &effect.as_str()))
        {
            return false;
        }
    }

    if filters.has_invariant && symbol.invariant.is_none() {
        return false;
    }

    if let Some(generic_name) = &filters.generic_over {
        if !symbol.generics.iter().any(|param| param == generic_name) {
            return false;
        }
    }

    if filters.has_requires && symbol.requires.is_none() {
        return false;
    }

    if filters.has_ensures && symbol.ensures.is_none() {
        return false;
    }

    true
}

fn name_matches_pattern(pattern: &str, value: &str) -> bool {
    if pattern.contains('*') || pattern.contains('?') {
        wildcard_match(pattern.as_bytes(), value.as_bytes())
    } else {
        value == pattern
    }
}

fn wildcard_match(pattern: &[u8], value: &[u8]) -> bool {
    let mut dp = vec![false; value.len() + 1];
    dp[0] = true;

    for token in pattern {
        if *token == b'*' {
            for idx in 1..=value.len() {
                dp[idx] = dp[idx] || dp[idx - 1];
            }
            continue;
        }

        for idx in (1..=value.len()).rev() {
            dp[idx] = dp[idx - 1] && (*token == b'?' || *token == value[idx - 1]);
        }
        dp[0] = false;
    }

    dp[value.len()]
}

fn location_for_span(file: &Path, source: &str, span: Span) -> SymbolLocation {
    let (line, column) = line_col_for_offset(source, span.start);
    SymbolLocation {
        file: file.to_string_lossy().to_string(),
        line,
        column,
        span_start: span.start,
        span_end: span.end,
    }
}

fn line_col_for_offset(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;

    for byte in source.as_bytes().iter().take(offset.min(source.len())) {
        if *byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

fn snippet_for_span(source: &str, span: Span) -> Option<String> {
    let end = span.end.min(source.len());
    let start = span.start.min(end);
    source
        .get(start..end)
        .map(str::trim)
        .filter(|snippet| !snippet.is_empty())
        .map(ToString::to_string)
}

fn render_function_signature(
    function: &ast::Function,
    requires: Option<&str>,
    ensures: Option<&str>,
) -> String {
    let params = function
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name, render_type_expr(&param.ty)))
        .collect::<Vec<_>>()
        .join(", ");

    let mut signature = format!(
        "fn {}{}({params}) -> {}",
        function.name,
        render_generic_params(&function.generics),
        render_type_expr(&function.ret_type)
    );

    if !function.effects.is_empty() {
        signature.push_str(" effects { ");
        signature.push_str(&function.effects.join(", "));
        signature.push_str(" }");
    }

    if !function.capabilities.is_empty() {
        signature.push_str(" capabilities { ");
        signature.push_str(&function.capabilities.join(", "));
        signature.push_str(" }");
    }

    if let Some(requires) = requires {
        signature.push_str(" requires ");
        signature.push_str(requires);
    }

    if let Some(ensures) = ensures {
        signature.push_str(" ensures ");
        signature.push_str(ensures);
    }

    signature
}

fn render_struct_signature(strukt: &ast::StructDef, invariant: Option<&str>) -> String {
    let fields = strukt
        .fields
        .iter()
        .map(|field| format!("{}: {}", field.name, render_type_expr(&field.ty)))
        .collect::<Vec<_>>()
        .join(", ");

    let mut signature = format!(
        "struct {}{} {{ {} }}",
        strukt.name,
        render_generic_params(&strukt.generics),
        fields
    );

    if let Some(invariant) = invariant {
        signature.push_str(" invariant ");
        signature.push_str(invariant);
    }

    signature
}

fn render_enum_signature(enm: &ast::EnumDef) -> String {
    let variants = enm
        .variants
        .iter()
        .map(|variant| {
            if let Some(payload) = &variant.payload {
                format!("{}({})", variant.name, render_type_expr(payload))
            } else {
                variant.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "enum {}{} {{ {} }}",
        enm.name,
        render_generic_params(&enm.generics),
        variants
    )
}

fn render_enum_variant_signature(enum_name: &str, variant: &ast::VariantDef) -> String {
    if let Some(payload) = &variant.payload {
        format!(
            "{}::{}({})",
            enum_name,
            variant.name,
            render_type_expr(payload)
        )
    } else {
        format!("{}::{}", enum_name, variant.name)
    }
}

fn render_trait_signature(trait_def: &ast::TraitDef) -> String {
    let methods = trait_def
        .methods
        .iter()
        .map(|method| format!("{};", render_function_signature(method, None, None)))
        .collect::<Vec<_>>()
        .join(" ");

    if methods.is_empty() {
        format!(
            "trait {}{};",
            trait_def.name,
            render_generic_params(&trait_def.generics)
        )
    } else {
        format!(
            "trait {}{} {{ {} }}",
            trait_def.name,
            render_generic_params(&trait_def.generics),
            methods
        )
    }
}

fn impl_symbol_name(impl_def: &ast::ImplDef) -> String {
    if impl_def.is_inherent {
        impl_def
            .target
            .as_ref()
            .map(render_type_expr)
            .unwrap_or_else(|| impl_def.trait_name.clone())
    } else {
        impl_def.trait_name.clone()
    }
}

fn render_impl_signature(impl_def: &ast::ImplDef) -> String {
    let methods = impl_def
        .methods
        .iter()
        .map(|method| format!("{};", render_function_signature(method, None, None)))
        .collect::<Vec<_>>()
        .join(" ");

    if impl_def.is_inherent {
        let target = impl_def
            .target
            .as_ref()
            .map(render_type_expr)
            .unwrap_or_else(|| impl_def.trait_name.clone());
        if methods.is_empty() {
            format!("impl {} {{}}", target)
        } else {
            format!("impl {} {{ {} }}", target, methods)
        }
    } else {
        let trait_args = if impl_def.trait_args.is_empty() {
            String::new()
        } else {
            format!(
                "[{}]",
                impl_def
                    .trait_args
                    .iter()
                    .map(render_type_expr)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let target = impl_def
            .target
            .as_ref()
            .map(render_type_expr)
            .map(|value| format!(" for {value}"))
            .unwrap_or_default();
        if methods.is_empty() {
            format!("impl {}{}{};", impl_def.trait_name, trait_args, target)
        } else {
            format!(
                "impl {}{}{} {{ {} }}",
                impl_def.trait_name, trait_args, target, methods
            )
        }
    }
}

fn render_generic_params(generics: &[ast::GenericParam]) -> String {
    if generics.is_empty() {
        return String::new();
    }

    format!(
        "[{}]",
        generics
            .iter()
            .map(|generic| {
                if generic.bounds.is_empty() {
                    generic.name.clone()
                } else {
                    format!("{}: {}", generic.name, generic.bounds.join(" + "))
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn render_type_expr(ty: &ast::TypeExpr) -> String {
    match &ty.kind {
        ast::TypeKind::Unit => "Unit".to_string(),
        ast::TypeKind::DynTrait { trait_name } => format!("dyn {trait_name}"),
        ast::TypeKind::Named { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}[{}]",
                    name,
                    args.iter()
                        .map(render_type_expr)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        ast::TypeKind::Hole => "_".to_string(),
    }
}

fn symbol_index_root(entry_path: &Path) -> PathBuf {
    if entry_path.is_dir() {
        entry_path.to_path_buf()
    } else {
        find_project_root(entry_path)
    }
}

fn find_project_root(entry: &Path) -> PathBuf {
    let mut cursor = entry
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    loop {
        if cursor.join("aic.toml").exists() {
            return cursor;
        }

        let Some(parent) = cursor.parent() else {
            return entry
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
        };
        cursor = parent.to_path_buf();
    }
}

fn collect_aic_files(root: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(root)
        .with_context(|| format!("failed to read {}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to list {}", root.display()))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();

        if path.is_dir() {
            if matches!(name, ".git" | "target" | ".aic-cache") {
                continue;
            }
            collect_aic_files(&path, out)?;
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) == Some("aic") {
            out.push(path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{query_symbols, QueryFilters, SymbolKind};

    #[test]
    fn wildcard_query_filters_are_applied_deterministically() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("src");
        fs::create_dir_all(&src).expect("mkdir src");

        fs::write(
            src.join("main.aic"),
            r#"module demo.main;
struct User {
    age: Int,
} invariant age >= 0

fn validate_user(user: User) -> Bool effects { io } requires user.age >= 0 ensures result == true {
    true
}

fn helper() -> Int {
    0
}
"#,
        )
        .expect("write source");

        let report = query_symbols(
            dir.path(),
            QueryFilters {
                kind: Some(SymbolKind::Function),
                name_pattern: Some("validate*".to_string()),
                effects: vec!["io".to_string()],
                has_invariant: false,
                generic_over: None,
                has_requires: true,
                has_ensures: true,
                limit: None,
            },
        )
        .expect("query symbols");

        assert_eq!(report.matched_symbols, 1);
        assert_eq!(report.symbols[0].name, "validate_user");
        assert_eq!(report.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn struct_invariant_filter_matches_only_structs_with_invariants() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("src");
        fs::create_dir_all(&src).expect("mkdir src");

        fs::write(
            src.join("main.aic"),
            r#"module demo.main;
struct User {
    age: Int,
} invariant age >= 0

struct Empty {
    value: Int,
}
"#,
        )
        .expect("write source");

        let report = query_symbols(
            dir.path(),
            QueryFilters {
                kind: Some(SymbolKind::Struct),
                name_pattern: None,
                effects: Vec::new(),
                has_invariant: true,
                generic_over: None,
                has_requires: false,
                has_ensures: false,
                limit: None,
            },
        )
        .expect("query symbols");

        assert_eq!(report.matched_symbols, 1);
        assert_eq!(report.symbols[0].name, "User");
    }
}
