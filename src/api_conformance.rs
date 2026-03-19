use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Serialize;

use crate::ast::{self, TypeExpr};
use crate::diagnostics::Diagnostic;
use crate::effects::{
    normalize_capability_declarations_with_context, normalize_effect_declarations_with_context,
};
use crate::ir;
use crate::ir_builder;
use crate::package_loader::{self, LoadOptions};
use crate::package_workflow::read_manifest;
use crate::parser;
use crate::resolver::{self, FunctionInfo, Resolution};
use crate::span::Span;
use crate::symbol_query::{self, SymbolKind, SymbolLocation, SymbolRecord};
use crate::typecheck::{self, PreflightCallable};

const SCHEMA_VERSION: &str = "1.0";
const TYPE_PARSE_LABEL: &str = "<validate-type>";
const ARG_TYPE_PARSE_LABEL: &str = "<validate-call-arg-type>";
const SUGGEST_DEFAULT_LIMIT: usize = 8;
const TYPE_PROBE_PREFIX: &str = "fn probe(value: ";
const TYPE_PROBE_SUFFIX: &str = ") -> Int { 0 }";
const TYPE_ALIAS_PROBE_NAME: &str = "Probe";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ValidateCallResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub fast_path: bool,
    pub project_root: String,
    pub target: String,
    pub arg_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<ResolvedCallable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<SuggestCandidate>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ValidateTypeResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub fast_path: bool,
    pub project_root: String,
    pub type_expr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub named_types: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SuggestResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub fast_path: bool,
    pub project_root: String,
    pub partial: String,
    pub candidate_count: usize,
    pub candidates: Vec<SuggestCandidate>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedCallable {
    pub qualified_name: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub signature: String,
    pub location: SymbolLocation,
    pub arity: usize,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub is_extern: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extern_abi: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generics: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub generic_bindings: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ensures: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SuggestCandidate {
    pub qualified_name: String,
    pub name: String,
    pub kind: SymbolKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub signature: String,
    pub match_kind: SuggestMatchKind,
    pub distance: usize,
    pub score: i64,
    pub location: SymbolLocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ensures: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestMatchKind {
    Exact,
    CaseInsensitiveExact,
    Prefix,
    Substring,
    Wildcard,
    Fuzzy,
}

#[derive(Debug, Clone)]
struct ParsedTypeExpr {
    rendered: String,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
struct FastContext {
    project_root: PathBuf,
    file_label: String,
    ir: ir::Program,
    resolution: Resolution,
    symbol_records: Vec<SymbolRecord>,
    type_strings: BTreeMap<ir::TypeId, String>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
struct SuggestSymbol {
    qualified_name: String,
    name: String,
    kind: SymbolKind,
    module: Option<String>,
    signature: String,
    location: SymbolLocation,
    effects: Vec<String>,
    capabilities: Vec<String>,
    generics: Vec<String>,
    requires: Option<String>,
    ensures: Option<String>,
    container: Option<String>,
}

#[derive(Debug, Clone)]
struct MatchScore {
    match_kind: SuggestMatchKind,
    bucket: u8,
    distance: usize,
    len_delta: usize,
    score: i64,
}

impl FastContext {
    fn load(project_root: &Path, offline: bool) -> anyhow::Result<Self> {
        let project_root = normalize_project_root(project_root);
        let entry_path = default_entry_file(&project_root)?;
        let file_label = entry_path.to_string_lossy().to_string();
        let load = package_loader::load_entry_with_options(&project_root, LoadOptions { offline })?;
        let mut diagnostics = load.diagnostics;

        let program = load.program.unwrap_or_else(empty_ast_program);
        let mut ir = ir_builder::build(&program);
        diagnostics.extend(normalize_effect_declarations_with_context(
            &mut ir,
            &file_label,
            Some(&load.item_modules),
            Some(&load.module_files),
        ));
        diagnostics.extend(normalize_capability_declarations_with_context(
            &mut ir,
            &file_label,
            Some(&load.item_modules),
            Some(&load.module_files),
        ));
        let (resolution, resolve_diags) = resolver::resolve_with_item_modules_imports_and_files(
            &ir,
            &file_label,
            Some(&load.item_modules),
            Some(&load.module_imports),
            Some(&load.module_files),
        );
        diagnostics.extend(resolve_diags);
        sort_diagnostics(&mut diagnostics);

        let type_strings = ir
            .types
            .iter()
            .map(|ty| (ty.id, ty.repr.clone()))
            .collect::<BTreeMap<_, _>>();
        let symbol_records = symbol_query::list_symbols(&project_root).unwrap_or_default();
        Ok(Self {
            project_root,
            file_label,
            ir,
            resolution,
            symbol_records,
            type_strings,
            diagnostics,
        })
    }

    fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    fn find_symbol_record(&self, module: Option<&str>, name: &str) -> Option<&SymbolRecord> {
        self.symbol_records.iter().find(|symbol| {
            symbol.kind == SymbolKind::Function
                && symbol.name == name
                && symbol.container.is_none()
                && match (module, symbol.module.as_deref()) {
                    (None, None) => true,
                    (Some(expected), Some(found)) => expected == found,
                    _ => false,
                }
        })
    }

    fn all_suggest_symbols(&self) -> Vec<SuggestSymbol> {
        let mut out = BTreeMap::<(String, String, Option<String>), SuggestSymbol>::new();

        for symbol in &self.symbol_records {
            out.entry((
                symbol.kind.as_str().to_string(),
                symbol.name.clone(),
                symbol.module.clone(),
            ))
            .or_insert_with(|| SuggestSymbol {
                qualified_name: qualified_symbol_name(symbol.module.as_deref(), &symbol.name),
                name: symbol.name.clone(),
                kind: symbol.kind,
                module: symbol.module.clone(),
                signature: symbol.signature.clone(),
                location: symbol.location.clone(),
                effects: symbol.effects.clone(),
                capabilities: symbol.capabilities.clone(),
                generics: symbol.generics.clone(),
                requires: symbol.requires.clone(),
                ensures: symbol.ensures.clone(),
                container: symbol.container.clone(),
            });
        }

        for ((module, name), info) in &self.resolution.module_function_infos {
            out.entry((
                SymbolKind::Function.as_str().to_string(),
                name.clone(),
                Some(module.clone()),
            ))
            .or_insert_with(|| SuggestSymbol {
                qualified_name: qualified_symbol_name(Some(module.as_str()), name),
                name: name.clone(),
                kind: SymbolKind::Function,
                module: Some(module.clone()),
                signature: render_function_signature(
                    Some(module.as_str()),
                    name,
                    info,
                    &self.type_strings,
                ),
                location: external_location(),
                effects: sorted_strings(info.effects.iter().cloned().collect()),
                capabilities: sorted_strings(info.capabilities.iter().cloned().collect()),
                generics: info.generics.clone(),
                requires: None,
                ensures: None,
                container: None,
            });
        }

        out.into_values().collect()
    }
}

pub fn validate_call(
    project_root: &Path,
    target: &str,
    arg_types: &[String],
    offline: bool,
) -> anyhow::Result<ValidateCallResponse> {
    let context = FastContext::load(project_root, offline)?;
    if context.has_errors() {
        return Ok(ValidateCallResponse {
            schema_version: SCHEMA_VERSION,
            command: "validate-call",
            ok: false,
            fast_path: true,
            project_root: context.project_root.display().to_string(),
            target: target.to_string(),
            arg_types: arg_types.to_vec(),
            resolved: None,
            suggestions: Vec::new(),
            diagnostics: context.diagnostics,
        });
    }

    let mut parsed_arg_types = Vec::with_capacity(arg_types.len());
    let mut diagnostics = Vec::new();
    for raw in arg_types {
        let parsed = parse_cli_type_expr(
            raw,
            ARG_TYPE_PARSE_LABEL,
            TYPE_PROBE_PREFIX,
            TYPE_PROBE_SUFFIX,
        )?;
        if parsed.diagnostics.iter().any(Diagnostic::is_error) {
            diagnostics.extend(parsed.diagnostics);
        } else {
            parsed_arg_types.push(parsed.rendered);
        }
    }
    sort_diagnostics(&mut diagnostics);
    if diagnostics.iter().any(Diagnostic::is_error) {
        return Ok(ValidateCallResponse {
            schema_version: SCHEMA_VERSION,
            command: "validate-call",
            ok: false,
            fast_path: true,
            project_root: context.project_root.display().to_string(),
            target: target.to_string(),
            arg_types: arg_types.to_vec(),
            resolved: None,
            suggestions: Vec::new(),
            diagnostics,
        });
    }

    let normalized_target = normalize_preflight_target(&context.resolution, target);
    let preflight = typecheck::validate_call_fast(
        &context.ir,
        &context.resolution,
        &context.file_label,
        &normalized_target,
        &parsed_arg_types,
    );
    let resolved = preflight
        .callable
        .map(|callable| resolved_callable_from_preflight(&context, callable));
    let suggestions = if resolved.is_none() {
        let partial = target
            .rsplit('.')
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(target);
        suggest_candidates(&context.all_suggest_symbols(), partial, Some(5))
    } else {
        Vec::new()
    };
    let mut all_diagnostics = preflight.diagnostics;
    all_diagnostics.extend(diagnostics);
    sort_diagnostics(&mut all_diagnostics);

    Ok(ValidateCallResponse {
        schema_version: SCHEMA_VERSION,
        command: "validate-call",
        ok: resolved.is_some() && !all_diagnostics.iter().any(Diagnostic::is_error),
        fast_path: true,
        project_root: context.project_root.display().to_string(),
        target: target.to_string(),
        arg_types: parsed_arg_types,
        resolved,
        suggestions,
        diagnostics: all_diagnostics,
    })
}

pub fn validate_type(
    project_root: &Path,
    type_expr: &str,
    offline: bool,
) -> anyhow::Result<ValidateTypeResponse> {
    let parsed = parse_cli_type_expr(
        type_expr,
        TYPE_PARSE_LABEL,
        TYPE_PROBE_PREFIX,
        TYPE_PROBE_SUFFIX,
    )?;
    if parsed.diagnostics.iter().any(Diagnostic::is_error) {
        return Ok(ValidateTypeResponse {
            schema_version: SCHEMA_VERSION,
            command: "validate-type",
            ok: false,
            fast_path: true,
            project_root: normalize_project_root(project_root).display().to_string(),
            type_expr: type_expr.to_string(),
            canonical: None,
            kind: None,
            named_types: Vec::new(),
            diagnostics: parsed.diagnostics,
        });
    }

    let context = FastContext::load(project_root, offline)?;
    if context.has_errors() {
        return Ok(ValidateTypeResponse {
            schema_version: SCHEMA_VERSION,
            command: "validate-type",
            ok: false,
            fast_path: true,
            project_root: context.project_root.display().to_string(),
            type_expr: type_expr.to_string(),
            canonical: None,
            kind: None,
            named_types: Vec::new(),
            diagnostics: context.diagnostics,
        });
    }

    let probe_source = render_validate_type_probe(type_expr);
    let (probe_program, probe_diagnostics) = parser::parse(&probe_source, TYPE_PARSE_LABEL);
    let mut diagnostics = rebase_type_diagnostics(
        probe_diagnostics,
        format!("type {TYPE_ALIAS_PROBE_NAME} = ").len(),
        type_expr.len(),
        TYPE_PARSE_LABEL,
    );
    let mut canonical = Some(parsed.rendered.clone());
    let mut kind = None;
    let mut named_types = Vec::new();
    if let Some(probe_program) = probe_program.as_ref() {
        if let Some(type_expr) = extract_probe_type_expr(probe_program) {
            kind = Some(type_expr_kind(type_expr));
            named_types = collect_named_types(type_expr);
        }
    }
    let preflight = typecheck::validate_type_fast(
        &context.ir,
        &context.resolution,
        &context.file_label,
        &parsed.rendered,
    );
    if let Some(normalized_type) = preflight.normalized_type {
        canonical = Some(normalized_type);
    }
    diagnostics.extend(preflight.diagnostics);
    sort_diagnostics(&mut diagnostics);

    Ok(ValidateTypeResponse {
        schema_version: SCHEMA_VERSION,
        command: "validate-type",
        ok: canonical.is_some() && !diagnostics.iter().any(Diagnostic::is_error),
        fast_path: true,
        project_root: context.project_root.display().to_string(),
        type_expr: type_expr.to_string(),
        canonical,
        kind,
        named_types,
        diagnostics,
    })
}

pub fn suggest_partial(
    project_root: &Path,
    partial: &str,
    limit: Option<usize>,
) -> anyhow::Result<SuggestResponse> {
    let context = FastContext::load(project_root, false)?;
    if context.has_errors() {
        return Ok(SuggestResponse {
            schema_version: SCHEMA_VERSION,
            command: "suggest",
            ok: false,
            fast_path: true,
            project_root: context.project_root.display().to_string(),
            partial: partial.to_string(),
            candidate_count: 0,
            candidates: Vec::new(),
            diagnostics: context.diagnostics,
        });
    }

    let candidates = suggest_candidates(&context.all_suggest_symbols(), partial, limit);
    Ok(SuggestResponse {
        schema_version: SCHEMA_VERSION,
        command: "suggest",
        ok: true,
        fast_path: true,
        project_root: context.project_root.display().to_string(),
        partial: partial.to_string(),
        candidate_count: candidates.len(),
        candidates,
        diagnostics: Vec::new(),
    })
}

fn normalize_project_root(project_root: &Path) -> PathBuf {
    if project_root.is_dir() {
        project_root.to_path_buf()
    } else {
        project_root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

fn normalize_preflight_target(resolution: &Resolution, target: &str) -> String {
    let normalized = target.replace("::", ".");
    let segments = normalized
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.len() <= 1 {
        return normalized;
    }

    let current_module = resolution
        .entry_module
        .clone()
        .unwrap_or_else(|| "<root>".to_string());
    if segments.len() == 2 {
        let alias = segments[0];
        let alias_is_ambiguous = resolution
            .module_ambiguous_import_aliases
            .get(&current_module)
            .map(|set| set.contains(alias))
            .unwrap_or_else(|| resolution.ambiguous_import_aliases.contains(alias));
        if !alias_is_ambiguous {
            if let Some(module) = resolution
                .module_import_aliases
                .get(&current_module)
                .and_then(|aliases| aliases.get(alias))
                .or_else(|| resolution.import_aliases.get(alias))
            {
                return format!("{module}.{}", segments[1]);
            }
        }
    }

    normalized
}

fn default_entry_file(project_root: &Path) -> anyhow::Result<PathBuf> {
    if project_root.is_dir() {
        if let Some(manifest) = read_manifest(project_root)? {
            return Ok(project_root.join(manifest.main));
        }
        return Ok(project_root.join("src/main.aic"));
    }
    Ok(project_root.to_path_buf())
}

fn parse_cli_type_expr(
    raw: &str,
    label: &str,
    prefix: &str,
    suffix: &str,
) -> anyhow::Result<ParsedTypeExpr> {
    let source = format!("{prefix}{raw}{suffix}");
    let (program, diagnostics) = parser::parse(&source, label);
    let mut diagnostics = rebase_type_diagnostics(diagnostics, prefix.len(), raw.len(), label);
    if diagnostics.iter().any(Diagnostic::is_error) {
        sort_diagnostics(&mut diagnostics);
        return Ok(ParsedTypeExpr {
            rendered: raw.to_string(),
            diagnostics,
        });
    }

    let program = program.context("missing parser program for type expression")?;
    let function = match program.items.first() {
        Some(ast::Item::Function(function)) => function,
        _ => anyhow::bail!("type wrapper did not produce a function item"),
    };
    let ty = function
        .params
        .first()
        .map(|param| &param.ty)
        .context("type wrapper did not produce a parameter type")?;
    Ok(ParsedTypeExpr {
        rendered: render_type_expr(ty),
        diagnostics,
    })
}

fn rebase_type_diagnostics(
    mut diagnostics: Vec<Diagnostic>,
    prefix_len: usize,
    raw_len: usize,
    label: &str,
) -> Vec<Diagnostic> {
    for diagnostic in &mut diagnostics {
        for span in &mut diagnostic.spans {
            span.file = label.to_string();
            span.start = span.start.saturating_sub(prefix_len).min(raw_len);
            span.end = span.end.saturating_sub(prefix_len).min(raw_len);
        }
        for fix in &mut diagnostic.suggested_fixes {
            if let Some(start) = fix.start.as_mut() {
                *start = start.saturating_sub(prefix_len).min(raw_len);
            }
            if let Some(end) = fix.end.as_mut() {
                *end = end.saturating_sub(prefix_len).min(raw_len);
            }
        }
    }
    diagnostics
}

fn render_validate_type_probe(type_expr: &str) -> String {
    format!("type {TYPE_ALIAS_PROBE_NAME} = {type_expr};\n")
}

fn extract_probe_type_expr(program: &ast::Program) -> Option<&TypeExpr> {
    program.items.iter().find_map(|item| match item {
        ast::Item::Function(function)
            if function.name == ast::encode_internal_type_alias(TYPE_ALIAS_PROBE_NAME) =>
        {
            Some(&function.ret_type)
        }
        _ => None,
    })
}

fn render_type_expr(ty: &TypeExpr) -> String {
    match &ty.kind {
        ast::TypeKind::Unit => "()".to_string(),
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

fn type_expr_kind(type_expr: &TypeExpr) -> &'static str {
    match &type_expr.kind {
        ast::TypeKind::Unit => "unit",
        ast::TypeKind::Named { .. } => "named",
        ast::TypeKind::DynTrait { .. } => "dyn_trait",
        ast::TypeKind::Hole => "hole",
    }
}

fn collect_named_types(type_expr: &TypeExpr) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    collect_named_types_inner(type_expr, &mut names);
    names.into_iter().collect()
}

fn collect_named_types_inner(type_expr: &TypeExpr, out: &mut std::collections::BTreeSet<String>) {
    match &type_expr.kind {
        ast::TypeKind::Unit | ast::TypeKind::Hole => {}
        ast::TypeKind::DynTrait { trait_name } => {
            out.insert(trait_name.clone());
        }
        ast::TypeKind::Named { name, args } => {
            out.insert(name.clone());
            for arg in args {
                collect_named_types_inner(arg, out);
            }
        }
    }
}

fn resolved_callable_from_preflight(
    context: &FastContext,
    callable: PreflightCallable,
) -> ResolvedCallable {
    let symbol = context.find_symbol_record(callable.module.as_deref(), &callable.name);
    let location = symbol
        .map(|symbol| symbol.location.clone())
        .unwrap_or_else(external_location);
    ResolvedCallable {
        qualified_name: qualified_symbol_name(callable.module.as_deref(), &callable.name),
        name: callable.name,
        module: callable.module,
        signature: callable.signature,
        location,
        arity: callable.parameters.len(),
        is_async: callable.is_async,
        is_unsafe: callable.is_unsafe,
        is_extern: callable.is_extern,
        extern_abi: callable.extern_abi,
        effects: callable.effects,
        capabilities: callable.capabilities,
        generics: callable.generics,
        generic_bindings: callable.generic_bindings,
        requires: symbol.and_then(|symbol| symbol.requires.clone()),
        ensures: symbol.and_then(|symbol| symbol.ensures.clone()),
    }
}

fn suggest_candidates(
    symbols: &[SuggestSymbol],
    partial: &str,
    limit: Option<usize>,
) -> Vec<SuggestCandidate> {
    let mut ranked = symbols
        .iter()
        .filter_map(|symbol| {
            score_candidate(partial, symbol).map(|score| SuggestCandidateWithScore {
                candidate: SuggestCandidate {
                    qualified_name: symbol.qualified_name.clone(),
                    name: symbol.name.clone(),
                    kind: symbol.kind,
                    module: symbol.module.clone(),
                    signature: symbol.signature.clone(),
                    match_kind: score.match_kind,
                    distance: score.distance,
                    score: score.score,
                    location: symbol.location.clone(),
                    effects: symbol.effects.clone(),
                    capabilities: symbol.capabilities.clone(),
                    generics: symbol.generics.clone(),
                    requires: symbol.requires.clone(),
                    ensures: symbol.ensures.clone(),
                    container: symbol.container.clone(),
                },
                bucket: score.bucket,
                len_delta: score.len_delta,
            })
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|lhs, rhs| {
        lhs.bucket
            .cmp(&rhs.bucket)
            .then(lhs.candidate.distance.cmp(&rhs.candidate.distance))
            .then(lhs.len_delta.cmp(&rhs.len_delta))
            .then(kind_priority(lhs.candidate.kind).cmp(&kind_priority(rhs.candidate.kind)))
            .then(lhs.candidate.module.cmp(&rhs.candidate.module))
            .then(lhs.candidate.name.cmp(&rhs.candidate.name))
            .then(
                lhs.candidate
                    .location
                    .file
                    .cmp(&rhs.candidate.location.file),
            )
            .then(
                lhs.candidate
                    .location
                    .span_start
                    .cmp(&rhs.candidate.location.span_start),
            )
    });

    let mut candidates = ranked
        .into_iter()
        .map(|entry| entry.candidate)
        .collect::<Vec<_>>();
    if let Some(limit) = limit.or(Some(SUGGEST_DEFAULT_LIMIT)) {
        candidates.truncate(limit);
    }
    candidates
}

struct SuggestCandidateWithScore {
    candidate: SuggestCandidate,
    bucket: u8,
    len_delta: usize,
}

fn score_candidate(partial: &str, symbol: &SuggestSymbol) -> Option<MatchScore> {
    let query = partial.trim();
    if query.is_empty() {
        return None;
    }

    let query_lower = query.to_ascii_lowercase();
    let name_lower = symbol.name.to_ascii_lowercase();
    let qualified_lower = symbol.qualified_name.to_ascii_lowercase();
    let len_delta = symbol.name.len().abs_diff(query.len());

    if symbol.name == query || symbol.qualified_name == query {
        return Some(MatchScore {
            match_kind: SuggestMatchKind::Exact,
            bucket: 0,
            distance: 0,
            len_delta,
            score: 1000,
        });
    }

    if name_lower == query_lower || qualified_lower == query_lower {
        return Some(MatchScore {
            match_kind: SuggestMatchKind::CaseInsensitiveExact,
            bucket: 1,
            distance: 0,
            len_delta,
            score: 900,
        });
    }

    if symbol.name.starts_with(query) || symbol.qualified_name.starts_with(query) {
        return Some(MatchScore {
            match_kind: SuggestMatchKind::Prefix,
            bucket: 2,
            distance: 0,
            len_delta,
            score: 800 - len_delta as i64,
        });
    }

    if name_lower.contains(&query_lower) || qualified_lower.contains(&query_lower) {
        return Some(MatchScore {
            match_kind: SuggestMatchKind::Substring,
            bucket: 3,
            distance: 0,
            len_delta,
            score: 700 - len_delta as i64,
        });
    }

    if query.contains('*') || query.contains('?') {
        if wildcard_match(query.as_bytes(), symbol.name.as_bytes())
            || wildcard_match(query.as_bytes(), symbol.qualified_name.as_bytes())
        {
            return Some(MatchScore {
                match_kind: SuggestMatchKind::Wildcard,
                bucket: 4,
                distance: 0,
                len_delta,
                score: 600 - len_delta as i64,
            });
        }
    }

    let distance = levenshtein_distance(&query_lower, &name_lower)
        .min(levenshtein_distance(&query_lower, &qualified_lower));
    let threshold = fuzzy_threshold(query.len(), symbol.name.len());
    if distance <= threshold {
        return Some(MatchScore {
            match_kind: SuggestMatchKind::Fuzzy,
            bucket: 5,
            distance,
            len_delta,
            score: 500 - (distance as i64 * 10) - len_delta as i64,
        });
    }

    None
}

fn render_function_signature(
    module: Option<&str>,
    name: &str,
    info: &FunctionInfo,
    type_strings: &BTreeMap<ir::TypeId, String>,
) -> String {
    let generics = if info.generics.is_empty() {
        String::new()
    } else {
        format!("[{}]", info.generics.join(", "))
    };
    let params = info
        .param_names
        .iter()
        .zip(info.param_types.iter())
        .map(|(param_name, param_ty)| {
            let ty = type_strings
                .get(param_ty)
                .cloned()
                .unwrap_or_else(|| "<?>".to_string());
            format!("{param_name}: {ty}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let return_type = type_strings
        .get(&info.ret_type)
        .cloned()
        .unwrap_or_else(|| "<?>".to_string());
    let rendered_name = qualified_symbol_name(module, name);
    let mut prefix = String::new();
    if info.is_extern {
        prefix.push_str("extern ");
        if let Some(abi) = &info.extern_abi {
            prefix.push_str(abi);
            prefix.push(' ');
        }
    }
    if info.is_unsafe {
        prefix.push_str("unsafe ");
    }
    if info.is_async {
        prefix.push_str("async ");
    }
    format!("{prefix}fn {rendered_name}{generics}({params}) -> {return_type}")
}

fn qualified_symbol_name(module: Option<&str>, name: &str) -> String {
    match module {
        Some(module) if !module.is_empty() && module != "<root>" => format!("{module}.{name}"),
        _ => name.to_string(),
    }
}

fn empty_ast_program() -> ast::Program {
    ast::Program {
        module: None,
        imports: Vec::new(),
        items: Vec::new(),
        span: Span::new(0, 0),
    }
}

fn external_location() -> SymbolLocation {
    SymbolLocation {
        file: "<external>".to_string(),
        line: 1,
        column: 1,
        span_start: 0,
        span_end: 0,
    }
}

fn sorted_strings(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn sort_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.sort_by(|lhs, rhs| {
        lhs.code
            .cmp(&rhs.code)
            .then(lhs.message.cmp(&rhs.message))
            .then_with(|| {
                lhs.spans
                    .first()
                    .map(|span| (&span.file, span.start, span.end))
                    .cmp(
                        &rhs.spans
                            .first()
                            .map(|span| (&span.file, span.start, span.end)),
                    )
            })
    });
}

fn kind_priority(kind: SymbolKind) -> u8 {
    match kind {
        SymbolKind::Function => 0,
        SymbolKind::Struct => 1,
        SymbolKind::Enum => 2,
        SymbolKind::Trait => 3,
        SymbolKind::Variant => 4,
        SymbolKind::Module => 5,
        SymbolKind::Impl => 6,
    }
}

fn fuzzy_threshold(query_len: usize, candidate_len: usize) -> usize {
    let max_len = query_len.max(candidate_len);
    if max_len <= 4 {
        2
    } else {
        3
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

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars = a.chars().collect::<Vec<_>>();
    let b_chars = b.chars().collect::<Vec<_>>();
    let mut prev = (0..=b_chars.len()).collect::<Vec<_>>();
    let mut curr = vec![0usize; b_chars.len() + 1];

    for (i, ca) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_chars.len()]
}

#[cfg(test)]
mod tests {
    use super::{
        collect_named_types, parse_cli_type_expr, render_type_expr, score_candidate,
        suggest_candidates, SuggestMatchKind, SuggestSymbol, TYPE_PROBE_PREFIX, TYPE_PROBE_SUFFIX,
    };
    use crate::ast::{TypeExpr, TypeKind};
    use crate::span::Span;
    use crate::symbol_query::{SymbolKind, SymbolLocation};

    fn suggest_symbol(name: &str, kind: SymbolKind, module: Option<&str>) -> SuggestSymbol {
        SuggestSymbol {
            qualified_name: module
                .map(|module| format!("{module}.{name}"))
                .unwrap_or_else(|| name.to_string()),
            name: name.to_string(),
            kind,
            module: module.map(str::to_string),
            signature: format!("fn {name}() -> ()"),
            location: SymbolLocation {
                file: "src/main.aic".to_string(),
                line: 1,
                column: 1,
                span_start: 0,
                span_end: 1,
            },
            effects: Vec::new(),
            capabilities: Vec::new(),
            generics: Vec::new(),
            requires: None,
            ensures: None,
            container: None,
        }
    }

    #[test]
    fn suggest_candidates_include_prefix_matches_deterministically() {
        let symbols = vec![
            suggest_symbol("handle_result", SymbolKind::Function, Some("app.main")),
            suggest_symbol("hard_reset", SymbolKind::Function, Some("app.main")),
            suggest_symbol("ResultValue", SymbolKind::Struct, Some("app.types")),
        ];

        let ranked = suggest_candidates(&symbols, "hand", Some(10));
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].name, "handle_result");
        assert_eq!(ranked[0].match_kind, SuggestMatchKind::Prefix);
    }

    #[test]
    fn score_candidate_accepts_wildcard_and_exact() {
        let exact = suggest_symbol("render", SymbolKind::Function, Some("app.main"));
        let wildcard = suggest_symbol("reader", SymbolKind::Function, Some("app.main"));

        let exact_score = score_candidate("render", &exact).expect("exact score");
        assert_eq!(exact_score.match_kind, SuggestMatchKind::Exact);

        let wildcard_score = score_candidate("re*er", &wildcard).expect("wildcard score");
        assert_eq!(wildcard_score.match_kind, SuggestMatchKind::Wildcard);
    }

    #[test]
    fn wrapped_type_parser_reports_malformed_input_against_type_label() {
        let parsed = parse_cli_type_expr(
            "Option[Int",
            "<validate-test>",
            TYPE_PROBE_PREFIX,
            TYPE_PROBE_SUFFIX,
        )
        .expect("parse malformed type expr");
        assert!(parsed
            .diagnostics
            .iter()
            .any(crate::diagnostics::Diagnostic::is_error));
        assert!(parsed
            .diagnostics
            .iter()
            .all(|diag| diag.spans.iter().all(|span| span.file == "<validate-test>")));
    }

    #[test]
    fn collect_named_types_is_deterministic() {
        let ty = TypeExpr {
            kind: TypeKind::Named {
                name: "Result".to_string(),
                args: vec![
                    TypeExpr {
                        kind: TypeKind::Named {
                            name: "Vec".to_string(),
                            args: vec![TypeExpr {
                                kind: TypeKind::Named {
                                    name: "User".to_string(),
                                    args: Vec::new(),
                                },
                                span: Span::new(0, 0),
                            }],
                        },
                        span: Span::new(0, 0),
                    },
                    TypeExpr {
                        kind: TypeKind::Named {
                            name: "AppError".to_string(),
                            args: Vec::new(),
                        },
                        span: Span::new(0, 0),
                    },
                ],
            },
            span: Span::new(0, 0),
        };

        assert_eq!(render_type_expr(&ty), "Result[Vec[User], AppError]");
        assert_eq!(
            collect_named_types(&ty),
            vec![
                "AppError".to_string(),
                "Result".to_string(),
                "User".to_string(),
                "Vec".to_string()
            ]
        );
    }

    #[test]
    fn score_candidate_keeps_prefix_ranked_ahead_of_fuzzy() {
        let prefix = score_candidate(
            "vali",
            &suggest_symbol("validate_call", SymbolKind::Function, Some("app.api")),
        )
        .expect("prefix score");
        let fuzzy = score_candidate(
            "validate_call",
            &suggest_symbol("vaildate_call", SymbolKind::Function, Some("app.api")),
        )
        .expect("fuzzy score");
        assert_eq!(prefix.match_kind, SuggestMatchKind::Prefix);
        assert_eq!(fuzzy.match_kind, SuggestMatchKind::Fuzzy);
        assert!(prefix.bucket < fuzzy.bucket);
        assert!(prefix.score > fuzzy.score);
    }
}
