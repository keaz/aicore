use std::fs;
use std::path::{Path, PathBuf};
use std::{error::Error, fmt};

use anyhow::Context;
use serde::Serialize;

use crate::ast;
use crate::parser;
use crate::span::Span;

const SCHEMA_VERSION: &str = "1.0";
const QUERY_MAX_LIMIT: usize = 500;
const QUERY_ERROR_UNSUPPORTED_FILTER_COMBINATION: &str = "unsupported_filter_combination";
const QUERY_ERROR_LIMIT_OUT_OF_RANGE: &str = "limit_out_of_range";
const QUERY_ERROR_INDEX_FAILED: &str = "symbol_index_failed";
const QUERY_ERROR_INDEX_PARTIAL: &str = "symbol_index_partial";
const INDEX_WARNING_CODE_IO_READ_FAILED: &str = "io_read_failed";

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct SymbolContracts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ensures: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invariant: Option<String>,
}

impl SymbolContracts {
    pub fn has_any(&self) -> bool {
        self.requires.is_some() || self.ensures.is_some() || self.invariant.is_some()
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
    pub module: Option<String>,
    pub location: SymbolLocation,
    pub effects: Vec<String>,
    #[serde(default)]
    pub contracts: SymbolContracts,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub generics: Vec<String>,
    #[serde(skip)]
    pub requires: Option<String>,
    #[serde(skip)]
    pub ensures: Option<String>,
    #[serde(skip)]
    pub invariant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct QueryFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<SymbolKind>,
    #[serde(rename = "name", skip_serializing_if = "Option::is_none")]
    pub name_pattern: Option<String>,
    #[serde(rename = "module", skip_serializing_if = "Option::is_none")]
    pub module_pattern: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub has_contract: bool,
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
    pub files_scanned: usize,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub skipped_files: Vec<SymbolIndexWarning>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QueryResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub project_root: String,
    pub total_symbols: usize,
    pub matched_symbols: usize,
    pub filters: QueryFilters,
    pub symbols: Vec<SymbolRecord>,
    pub files_scanned: usize,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub skipped_files: Vec<SymbolIndexWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<QueryError>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SymbolsResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub project_root: String,
    pub symbol_count: usize,
    pub symbols: Vec<SymbolRecord>,
    pub files_scanned: usize,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub skipped_files: Vec<SymbolIndexWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<QueryError>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SymbolIndexWarning {
    pub file: String,
    pub error_count: usize,
    pub code_count: usize,
    pub codes: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymbolIndexReport {
    symbols: Vec<SymbolRecord>,
    files_scanned: usize,
    files_indexed: usize,
    files_skipped: usize,
    skipped_files: Vec<SymbolIndexWarning>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QueryError {
    pub code: &'static str,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

impl QueryError {
    pub fn is_usage_error(&self) -> bool {
        matches!(
            self.code,
            QUERY_ERROR_UNSUPPORTED_FILTER_COMBINATION | QUERY_ERROR_LIMIT_OUT_OF_RANGE
        )
    }
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if !self.details.is_empty() {
            write!(f, ": {}", self.details.join("; "))?;
        }
        Ok(())
    }
}

impl Error for QueryError {}

pub fn list_symbols(entry_path: &Path) -> anyhow::Result<Vec<SymbolRecord>> {
    Ok(index_symbols(entry_path)?.symbols)
}

fn index_symbols(entry_path: &Path) -> anyhow::Result<SymbolIndexReport> {
    let root = symbol_index_root(entry_path);
    let mut files = Vec::new();
    collect_aic_files(&root, &mut files)?;
    files.sort();

    let mut symbols = Vec::new();
    let mut files_scanned = 0usize;
    let mut files_indexed = 0usize;
    let mut skipped_files = Vec::new();
    for file in files {
        files_scanned += 1;
        let source = match fs::read_to_string(&file) {
            Ok(value) => value,
            Err(err) => {
                skipped_files.push(io_read_warning(&file, &err));
                continue;
            }
        };

        let (program, diagnostics) = parser::parse(&source, &file.to_string_lossy());
        if diagnostics.iter().any(|diag| diag.is_error()) {
            skipped_files.push(parse_error_warning(&file, &diagnostics));
            continue;
        }
        let Some(program) = program else {
            skipped_files.push(SymbolIndexWarning {
                file: file.to_string_lossy().to_string(),
                error_count: 1,
                code_count: 0,
                codes: Vec::new(),
                summary: "parser produced no program".to_string(),
            });
            continue;
        };
        files_indexed += 1;

        let module_name = program.module.as_ref().map(|module| module.path.join("."));
        if let Some(module_decl) = &program.module {
            let module_value = module_name
                .clone()
                .unwrap_or_else(|| module_decl.path.join("."));
            symbols.push(SymbolRecord {
                name: module_value.clone(),
                kind: SymbolKind::Module,
                signature: format!("module {module_value}"),
                module: Some(module_value),
                location: location_for_span(&file, &source, module_decl.span),
                effects: Vec::new(),
                contracts: SymbolContracts::default(),
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
                    symbols.push(function_symbol_record(
                        &file,
                        &source,
                        &module_name,
                        &function,
                        None,
                    ));
                }
                ast::Item::Struct(strukt) => {
                    symbols.push(struct_symbol_record(&file, &source, &module_name, &strukt));
                }
                ast::Item::Enum(enm) => {
                    symbols.push(enum_symbol_record(&file, &source, &module_name, &enm));
                    for variant in &enm.variants {
                        symbols.push(enum_variant_symbol_record(
                            &file,
                            &source,
                            &module_name,
                            &enm,
                            variant,
                        ));
                    }
                }
                ast::Item::Trait(trait_def) => {
                    symbols.push(trait_symbol_record(
                        &file,
                        &source,
                        &module_name,
                        &trait_def,
                    ));
                    for method in &trait_def.methods {
                        symbols.push(function_symbol_record(
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
                    symbols.push(SymbolRecord {
                        name: impl_symbol_name(&impl_def),
                        kind: SymbolKind::Impl,
                        signature: impl_signature.clone(),
                        module: module_name.clone(),
                        location: location_for_span(&file, &source, impl_def.span),
                        effects: Vec::new(),
                        contracts: SymbolContracts::default(),
                        capabilities: Vec::new(),
                        generics: Vec::new(),
                        requires: None,
                        ensures: None,
                        invariant: None,
                        container: None,
                    });
                    for method in &impl_def.methods {
                        symbols.push(function_symbol_record(
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

    symbols.sort_by(|lhs, rhs| {
        lhs.name
            .cmp(&rhs.name)
            .then(lhs.kind.as_str().cmp(rhs.kind.as_str()))
            .then(lhs.module.cmp(&rhs.module))
            .then(lhs.location.file.cmp(&rhs.location.file))
            .then(lhs.location.span_start.cmp(&rhs.location.span_start))
    });

    let files_skipped = skipped_files.len();
    Ok(SymbolIndexReport {
        symbols,
        files_scanned,
        files_indexed,
        files_skipped,
        skipped_files,
    })
}

fn io_read_warning(file: &Path, err: &std::io::Error) -> SymbolIndexWarning {
    SymbolIndexWarning {
        file: file.to_string_lossy().to_string(),
        error_count: 1,
        code_count: 1,
        codes: vec![INDEX_WARNING_CODE_IO_READ_FAILED.to_string()],
        summary: format!("failed to read source file: {err}"),
    }
}

fn parse_error_warning(
    file: &Path,
    diagnostics: &[crate::diagnostics::Diagnostic],
) -> SymbolIndexWarning {
    let mut code_counts = std::collections::BTreeMap::<String, usize>::new();
    let mut summaries = Vec::new();
    for diagnostic in diagnostics.iter().filter(|diag| diag.is_error()) {
        *code_counts.entry(diagnostic.code.clone()).or_insert(0) += 1;
        summaries.push(format!("{}: {}", diagnostic.code, diagnostic.message));
    }
    summaries.sort();
    summaries.dedup();
    let summary = summaries
        .into_iter()
        .take(3)
        .collect::<Vec<_>>()
        .join(" | ");
    SymbolIndexWarning {
        file: file.to_string_lossy().to_string(),
        error_count: code_counts.values().sum(),
        code_count: code_counts.len(),
        codes: code_counts.keys().cloned().collect(),
        summary,
    }
}

pub fn query_symbols(entry_path: &Path, filters: QueryFilters) -> Result<QueryReport, QueryError> {
    validate_query_filters(&filters)?;
    let index = index_symbols(entry_path).map_err(|err| QueryError {
        code: QUERY_ERROR_INDEX_FAILED,
        message: format!("query: failed to index project symbols ({err})"),
        details: Vec::new(),
    })?;
    let total_symbols = index.symbols.len();

    let mut matched = index
        .symbols
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
        files_scanned: index.files_scanned,
        files_indexed: index.files_indexed,
        files_skipped: index.files_skipped,
        skipped_files: index.skipped_files,
    })
}

pub fn build_query_response(
    entry_path: &Path,
    filters: QueryFilters,
) -> anyhow::Result<QueryResponse> {
    build_query_response_with_options(entry_path, filters, false)
}

pub fn build_query_response_with_options(
    entry_path: &Path,
    filters: QueryFilters,
    strict_index: bool,
) -> anyhow::Result<QueryResponse> {
    let project_root = symbol_index_root(entry_path).to_string_lossy().to_string();
    let response = match query_symbols(entry_path, filters.clone()) {
        Ok(report) => {
            let strict_error = if strict_index {
                strict_index_error("query", report.files_skipped, &report.skipped_files)
            } else {
                None
            };
            QueryResponse {
                schema_version: SCHEMA_VERSION,
                command: "query",
                ok: strict_error.is_none(),
                project_root,
                total_symbols: report.total_symbols,
                matched_symbols: report.matched_symbols,
                filters: report.filters,
                symbols: report.symbols,
                files_scanned: report.files_scanned,
                files_indexed: report.files_indexed,
                files_skipped: report.files_skipped,
                skipped_files: report.skipped_files,
                error: strict_error,
            }
        }
        Err(error) => QueryResponse {
            schema_version: SCHEMA_VERSION,
            command: "query",
            ok: false,
            project_root,
            total_symbols: 0,
            matched_symbols: 0,
            filters,
            symbols: Vec::new(),
            files_scanned: 0,
            files_indexed: 0,
            files_skipped: 0,
            skipped_files: Vec::new(),
            error: Some(error),
        },
    };
    Ok(response)
}

pub fn build_symbols_response(entry_path: &Path) -> anyhow::Result<SymbolsResponse> {
    build_symbols_response_with_options(entry_path, false)
}

pub fn build_symbols_response_with_options(
    entry_path: &Path,
    strict_index: bool,
) -> anyhow::Result<SymbolsResponse> {
    let project_root = symbol_index_root(entry_path).to_string_lossy().to_string();
    let index = index_symbols(entry_path)?;
    let strict_error = if strict_index {
        strict_index_error("symbols", index.files_skipped, &index.skipped_files)
    } else {
        None
    };
    Ok(SymbolsResponse {
        schema_version: SCHEMA_VERSION,
        command: "symbols",
        ok: strict_error.is_none(),
        project_root,
        symbol_count: index.symbols.len(),
        symbols: index.symbols,
        files_scanned: index.files_scanned,
        files_indexed: index.files_indexed,
        files_skipped: index.files_skipped,
        skipped_files: index.skipped_files,
        error: strict_error,
    })
}

fn strict_index_error(
    command: &str,
    files_skipped: usize,
    skipped_files: &[SymbolIndexWarning],
) -> Option<QueryError> {
    if files_skipped == 0 {
        return None;
    }
    let mut details = skipped_files
        .iter()
        .map(|warning| format!("{}: {}", warning.file, warning.summary))
        .collect::<Vec<_>>();
    details.sort();
    Some(QueryError {
        code: QUERY_ERROR_INDEX_PARTIAL,
        message: format!(
            "{command}: strict index mode failed because {files_skipped} file(s) could not be indexed"
        ),
        details,
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
        contracts: SymbolContracts {
            requires: requires.clone(),
            ensures: ensures.clone(),
            invariant: None,
        },
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
        contracts: SymbolContracts {
            requires: None,
            ensures: None,
            invariant: invariant.clone(),
        },
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
        contracts: SymbolContracts::default(),
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
        contracts: SymbolContracts::default(),
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
        contracts: SymbolContracts::default(),
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

    if let Some(pattern) = &filters.module_pattern {
        let Some(module) = symbol.module.as_deref() else {
            return false;
        };
        if !name_matches_pattern(pattern, module) {
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

    if filters.has_contract && !symbol.contracts.has_any() {
        return false;
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

pub fn validate_query_filters(filters: &QueryFilters) -> Result<(), QueryError> {
    if let Some(limit) = filters.limit {
        if limit > QUERY_MAX_LIMIT {
            return Err(QueryError {
                code: QUERY_ERROR_LIMIT_OUT_OF_RANGE,
                message: format!(
                    "query: --limit must be between 1 and {QUERY_MAX_LIMIT} for deterministic pagination"
                ),
                details: vec![format!("received limit {limit}")],
            });
        }
    }

    let mut details = Vec::new();
    if filters.has_contract
        && (filters.has_requires || filters.has_ensures || filters.has_invariant)
    {
        details.push(
            "--has-contract cannot be combined with --has-requires, --has-ensures, or --has-invariant"
                .to_string(),
        );
    }

    if let Some(kind) = filters.kind {
        if filters.has_invariant && kind != SymbolKind::Struct {
            details.push("--has-invariant is only supported with --kind struct".to_string());
        }

        if (filters.has_requires || filters.has_ensures) && kind != SymbolKind::Function {
            details.push(
                "--has-requires and --has-ensures are only supported with --kind function"
                    .to_string(),
            );
        }
    }

    if details.is_empty() {
        Ok(())
    } else {
        Err(QueryError {
            code: QUERY_ERROR_UNSUPPORTED_FILTER_COMBINATION,
            message: "query: unsupported filter combination".to_string(),
            details,
        })
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
            if matches!(
                name,
                ".git"
                    | "target"
                    | ".aic"
                    | ".aic-cache"
                    | ".aic-checkpoints"
                    | ".aic-replay"
                    | ".aic-sessions"
            ) {
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

    use super::{
        build_query_response_with_options, build_symbols_response_with_options, query_symbols,
        validate_query_filters, QueryFilters, SymbolKind,
    };

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
                module_pattern: None,
                effects: vec!["io".to_string()],
                has_contract: false,
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
                module_pattern: None,
                effects: Vec::new(),
                has_contract: false,
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

    #[test]
    fn invalid_filter_combinations_are_rejected_stably() {
        let error = validate_query_filters(&QueryFilters {
            kind: Some(SymbolKind::Function),
            name_pattern: None,
            module_pattern: None,
            effects: Vec::new(),
            has_contract: false,
            has_invariant: true,
            generic_over: None,
            has_requires: false,
            has_ensures: false,
            limit: None,
        })
        .expect_err("expected invalid query filters");

        assert_eq!(error.code, "unsupported_filter_combination");
        assert!(error
            .details
            .iter()
            .any(|detail| detail == "--has-invariant is only supported with --kind struct"));
    }

    #[test]
    fn query_reports_partial_index_metadata_for_parse_failures() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("src");
        fs::create_dir_all(&src).expect("mkdir src");

        fs::write(
            src.join("main.aic"),
            r#"module demo.main;
fn main() -> Int { 0 }
"#,
        )
        .expect("write source");
        fs::write(
            src.join("broken.aic"),
            "module demo.broken;\nfn broken( -> Int { 0 }\n",
        )
        .expect("write broken source");

        let report = query_symbols(
            dir.path(),
            QueryFilters {
                kind: None,
                name_pattern: None,
                module_pattern: None,
                effects: Vec::new(),
                has_contract: false,
                has_invariant: false,
                generic_over: None,
                has_requires: false,
                has_ensures: false,
                limit: None,
            },
        )
        .expect("query symbols");

        assert_eq!(report.files_scanned, 2);
        assert_eq!(report.files_indexed, 1);
        assert_eq!(report.files_skipped, 1);
        assert_eq!(report.skipped_files.len(), 1);
        assert!(report.skipped_files[0].file.ends_with("broken.aic"));
        assert!(!report.skipped_files[0].summary.is_empty());
    }

    #[test]
    fn strict_index_mode_returns_stable_partial_index_error() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("src");
        fs::create_dir_all(&src).expect("mkdir src");

        fs::write(
            src.join("main.aic"),
            r#"module demo.main;
fn main() -> Int { 0 }
"#,
        )
        .expect("write source");
        fs::write(
            src.join("broken.aic"),
            "module demo.broken;\nfn broken( -> Int { 0 }\n",
        )
        .expect("write broken source");

        let query = build_query_response_with_options(
            dir.path(),
            QueryFilters {
                kind: None,
                name_pattern: None,
                module_pattern: None,
                effects: Vec::new(),
                has_contract: false,
                has_invariant: false,
                generic_over: None,
                has_requires: false,
                has_ensures: false,
                limit: None,
            },
            true,
        )
        .expect("query response");
        assert!(!query.ok);
        assert_eq!(
            query.error.as_ref().expect("query strict error").code,
            "symbol_index_partial"
        );

        let symbols =
            build_symbols_response_with_options(dir.path(), true).expect("symbols response");
        assert!(!symbols.ok);
        assert_eq!(
            symbols.error.as_ref().expect("symbols strict error").code,
            "symbol_index_partial"
        );
    }
}
