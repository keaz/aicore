use std::collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context};
use serde::Serialize;

use crate::ast::{self, BinOp, Expr, ExprKind, TypeExpr, TypeKind};
use crate::parser;
use crate::span::Span;
use crate::symbol_query::{self, SymbolKind, SymbolRecord};

const TESTGEN_PROTOCOL_VERSION: &str = "1.0";
const TESTGEN_PHASE: &str = "testgen";
const EFFECT_DIAGNOSTIC_CODE: &str = "E2001";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestgenStrategy {
    Boundary,
    InvariantViolation,
    ExhaustiveMatch,
    EffectCoverage,
}

impl TestgenStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boundary => "boundary",
            Self::InvariantViolation => "invariant-violation",
            Self::ExhaustiveMatch => "exhaustive-match",
            Self::EffectCoverage => "effect-coverage",
        }
    }
}

pub type TestStrategy = TestgenStrategy;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TestgenTarget {
    pub name: String,
    pub kind: SymbolKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TestgenArtifact {
    pub kind: String,
    pub name: String,
    pub path_hint: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TestgenResponse {
    pub protocol_version: String,
    pub phase: String,
    pub strategy: String,
    pub seed: u64,
    pub target: TestgenTarget,
    pub artifacts: Vec<TestgenArtifact>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct TargetSelector {
    kind: Option<SymbolKind>,
    name: String,
    module: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedFile {
    source: String,
    program: ast::Program,
}

#[derive(Debug, Clone)]
struct ParsedContext {
    source: String,
    program: ast::Program,
    imports: Vec<String>,
}

#[derive(Debug, Clone)]
struct StructContext {
    def: ast::StructDef,
    snippet: String,
}

#[derive(Debug, Clone)]
struct EnumContext {
    def: ast::EnumDef,
    snippet: String,
}

#[derive(Debug, Clone, Default)]
struct TypeIndex {
    structs: BTreeMap<String, StructContext>,
    enums: BTreeMap<String, EnumContext>,
}

#[derive(Debug, Clone)]
struct BoundaryCase {
    param: String,
    positive_value: String,
    negative_value: String,
    label: String,
}

pub fn generate_tests(
    project_root: &Path,
    strategy: TestgenStrategy,
    target_tokens: &[String],
    seed: u64,
) -> anyhow::Result<TestgenResponse> {
    let selector = parse_target_selector(target_tokens)?;
    let symbols = symbol_query::list_symbols(project_root)?;
    let target = select_target_symbol(&symbols, &selector)?;

    let mut parsed_cache = BTreeMap::<PathBuf, ParsedFile>::new();
    let type_index = build_type_index(&symbols, &mut parsed_cache)?;
    let mut notes = Vec::new();
    let artifacts = match strategy {
        TestgenStrategy::Boundary => build_boundary_artifacts(
            project_root,
            &target,
            &type_index,
            &mut parsed_cache,
            &mut notes,
            seed,
        )?,
        TestgenStrategy::InvariantViolation => build_invariant_artifacts(
            project_root,
            &target,
            &type_index,
            &mut parsed_cache,
            &mut notes,
            seed,
        )?,
        TestgenStrategy::ExhaustiveMatch => build_exhaustive_artifacts(
            project_root,
            &target,
            &type_index,
            &mut parsed_cache,
            &mut notes,
            seed,
        )?,
        TestgenStrategy::EffectCoverage => build_effect_artifacts(
            project_root,
            &target,
            &type_index,
            &mut parsed_cache,
            &mut notes,
            seed,
        )?,
    };

    Ok(TestgenResponse {
        protocol_version: TESTGEN_PROTOCOL_VERSION.to_string(),
        phase: TESTGEN_PHASE.to_string(),
        strategy: strategy.as_str().to_string(),
        seed,
        target: TestgenTarget {
            name: target.name.clone(),
            kind: target.kind,
            module: target.module.clone(),
        },
        artifacts,
        notes,
    })
}

pub fn generate(
    project_root: &Path,
    strategy: TestStrategy,
    target_tokens: &[String],
    seed: u64,
    emit_dir: Option<&Path>,
) -> anyhow::Result<TestgenResponse> {
    let mut response = generate_tests(project_root, strategy, target_tokens, seed)?;
    if let Some(dir) = emit_dir {
        let resolved_emit_dir = if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            project_root.join(dir)
        };
        materialize_artifacts(&mut response.artifacts, project_root, &resolved_emit_dir)?;
    }
    Ok(response)
}

pub fn format_text(response: &TestgenResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "testgen: {} {}",
        response.strategy, response.target.name
    ));
    lines.push(format!("seed: {}", response.seed));
    if response.artifacts.is_empty() {
        lines.push("artifacts: none".to_string());
    } else {
        lines.push(format!("artifacts ({}):", response.artifacts.len()));
        for artifact in &response.artifacts {
            let mut line = format!(
                "  [{}] {} -> {}",
                artifact.kind, artifact.name, artifact.path_hint
            );
            if let Some(written_path) = &artifact.written_path {
                line.push_str(&format!(" (written {written_path})"));
            }
            lines.push(line);
        }
    }
    if !response.notes.is_empty() {
        lines.push("notes:".to_string());
        for note in &response.notes {
            lines.push(format!("  - {note}"));
        }
    }
    lines.join("\n")
}

fn build_boundary_artifacts(
    _project_root: &Path,
    target: &SymbolRecord,
    type_index: &TypeIndex,
    parsed_cache: &mut BTreeMap<PathBuf, ParsedFile>,
    notes: &mut Vec<String>,
    seed: u64,
) -> anyhow::Result<Vec<TestgenArtifact>> {
    if target.kind != SymbolKind::Function {
        bail!(
            "strategy `boundary` requires a function target; got {} `{}`",
            target.kind.as_str(),
            target.name
        );
    }

    let context = load_context(target, parsed_cache)?;
    let function = find_function(&context.program, target)?;
    let function_snippet = snippet_for_span(&context.source, function.span)?;
    let boundaries = function
        .requires
        .as_ref()
        .map(derive_boundary_cases)
        .unwrap_or_default();
    if boundaries.is_empty() {
        bail!(
            "boundary strategy currently supports integer comparison requires clauses like `x >= 0`"
        );
    }
    let ensure_assert = function
        .ensures
        .as_ref()
        .and_then(|expr| snippet_for_span(&context.source, expr.span).ok())
        .map(|expr| format!("assert({});", expr.replace("result", "out")));

    let mut type_names = BTreeSet::new();
    for param in &function.params {
        collect_named_types(&param.ty, &mut type_names);
    }
    collect_named_types(&function.ret_type, &mut type_names);
    let dependency_snippets = collect_dependency_snippets(type_index, &type_names);

    let mut tests = Vec::new();
    for case in &boundaries {
        if case.param.contains('.') {
            bail!(
                "boundary strategy currently supports direct parameter comparisons, not field paths like `{}`",
                case.param
            );
        }
        let mut positive_overrides = BTreeMap::new();
        positive_overrides.insert(case.param.clone(), case.positive_value.clone());
        let positive_args =
            render_function_args(&function.params, type_index, &positive_overrides, seed)?;
        let positive_assert = ensure_assert
            .clone()
            .unwrap_or_else(|| "assert(true);".to_string());
        tests.push(format!(
            "#[test]\nfn test_{}_boundary_accepts_{}() -> () {{\n    let out = {}({});\n    {}\n}}",
            sanitize_identifier(&target.name),
            sanitize_identifier(&case.label),
            function.name,
            positive_args,
            positive_assert
        ));

        let mut negative_overrides = BTreeMap::new();
        negative_overrides.insert(case.param.clone(), case.negative_value.clone());
        let negative_args =
            render_function_args(&function.params, type_index, &negative_overrides, seed)?;
        tests.push(format!(
            "#[test]\n#[should_panic]\nfn test_{}_boundary_rejects_{}() -> () {{\n    let rejected = {}({});\n    rejected;\n}}",
            sanitize_identifier(&target.name),
            sanitize_identifier(&case.label),
            function.name,
            negative_args
        ));
    }

    if function.ensures.is_none() {
        notes.push("generated boundary tests without an ensures-derived assertion because the target does not declare `ensures`".to_string());
    }

    let content = join_sections(&[
        render_module_and_imports(
            &format!(
                "generated.testgen.boundary.{}",
                sanitize_identifier(&target.name)
            ),
            &context.imports,
            false,
        ),
        render_snippet_block(&dependency_snippets),
        function_snippet,
        tests.join("\n\n"),
    ]);

    Ok(vec![TestgenArtifact {
        kind: "attribute-test-fixture".to_string(),
        name: format!("{}_boundary", target.name),
        path_hint: format!(
            "tests/generated/boundary_{}.aic",
            sanitize_identifier(&target.name)
        ),
        content,
        written_path: None,
        reason: Some(
            "self-contained boundary tests generated from supported requires/ensures clauses"
                .to_string(),
        ),
    }])
}

fn build_invariant_artifacts(
    _project_root: &Path,
    target: &SymbolRecord,
    type_index: &TypeIndex,
    parsed_cache: &mut BTreeMap<PathBuf, ParsedFile>,
    _notes: &mut Vec<String>,
    seed: u64,
) -> anyhow::Result<Vec<TestgenArtifact>> {
    if target.kind != SymbolKind::Struct {
        bail!(
            "strategy `invariant-violation` requires a struct target; got {} `{}`",
            target.kind.as_str(),
            target.name
        );
    }

    let context = load_context(target, parsed_cache)?;
    let strukt = find_struct(&context.program, target)?;
    let struct_snippet = snippet_for_span(&context.source, strukt.span)?;
    let invariant = strukt
        .invariant
        .as_ref()
        .ok_or_else(|| anyhow!("struct `{}` does not declare an invariant", strukt.name))?;
    let invariant_text = snippet_for_span(&context.source, invariant.span)?;
    let struct_with_invariant = format!("{struct_snippet} invariant {invariant_text}");
    let boundaries = derive_boundary_cases(invariant);
    let boundary = boundaries.first().cloned().ok_or_else(|| {
        anyhow!(
            "invariant strategy currently supports integer comparison invariants like `age >= 0`"
        )
    })?;
    if boundary.param.contains('.') {
        bail!(
            "invariant strategy currently supports direct field comparisons, not field paths like `{}`",
            boundary.param
        );
    }

    let mut valid_fields = Vec::new();
    let mut invalid_fields = Vec::new();
    for field in &strukt.fields {
        let default = render_value_for_type(&field.ty, type_index, None, seed, &field.name)?;
        let positive = if field.name == boundary.param {
            boundary.positive_value.clone()
        } else {
            default.clone()
        };
        let negative = if field.name == boundary.param {
            boundary.negative_value.clone()
        } else {
            default
        };
        valid_fields.push(format!("{}: {}", field.name, positive));
        invalid_fields.push(format!("{}: {}", field.name, negative));
    }

    let valid_content = join_sections(&[
        format!("// expect: 1\n{}", render_module_and_imports(
            &format!("generated.testgen.inv.{}_valid", sanitize_identifier(&target.name)),
            &context.imports,
            true,
        )),
        struct_with_invariant.clone(),
        format!(
            "fn main() -> Int effects {{ io }} capabilities {{ io }} {{\n    let valid_value = {} {{ {} }};\n    valid_value.age;\n    print_int(1);\n    0\n}}",
            strukt.name,
            valid_fields.join(", ")
        ),
    ]);

    let invalid_content = join_sections(&[
        render_module_and_imports(
            &format!(
                "generated.testgen.inv.{}_invalid",
                sanitize_identifier(&target.name)
            ),
            &context.imports,
            false,
        ),
        struct_with_invariant,
        format!(
            "#[test]\n#[should_panic]\nfn test_{}_invariant_rejects_invalid_construction() -> () {{\n    let invalid_value = {} {{ {} }};\n    invalid_value.age;\n}}",
            sanitize_identifier(&target.name),
            strukt.name,
            invalid_fields.join(", ")
        ),
    ]);

    Ok(vec![
        TestgenArtifact {
            kind: "run-pass-fixture".to_string(),
            name: format!("{}_invariant_valid", target.name),
            path_hint: format!(
                "tests/generated/run-pass/invariant_{}_valid.aic",
                sanitize_identifier(&target.name)
            ),
            content: valid_content,
            written_path: None,
            reason: Some("valid invariant-preserving construction case".to_string()),
        },
        TestgenArtifact {
            kind: "attribute-test-fixture".to_string(),
            name: format!("{}_invariant_invalid", target.name),
            path_hint: format!(
                "tests/generated/invariant_{}_invalid.aic",
                sanitize_identifier(&target.name)
            ),
            content: invalid_content,
            written_path: None,
            reason: Some(
                "invalid construction case expected to panic through the lowered invariant helper"
                    .to_string(),
            ),
        },
    ])
}

fn build_exhaustive_artifacts(
    _project_root: &Path,
    target: &SymbolRecord,
    type_index: &TypeIndex,
    parsed_cache: &mut BTreeMap<PathBuf, ParsedFile>,
    _notes: &mut Vec<String>,
    seed: u64,
) -> anyhow::Result<Vec<TestgenArtifact>> {
    if target.kind != SymbolKind::Enum {
        bail!(
            "strategy `exhaustive-match` requires an enum target; got {} `{}`",
            target.kind.as_str(),
            target.name
        );
    }

    let context = load_context(target, parsed_cache)?;
    let enm = find_enum(&context.program, target)?;
    let enum_snippet = snippet_for_span(&context.source, enm.span)?;
    if enm.variants.is_empty() {
        bail!(
            "enum `{}` has no variants to generate exhaustive tests for",
            enm.name
        );
    }

    let mut tests = Vec::new();
    for (index, variant) in enm.variants.iter().enumerate() {
        let value = if let Some(payload) = &variant.payload {
            format!(
                "{}({})",
                variant.name,
                render_value_for_type(payload, type_index, None, seed, &variant.name)?
            )
        } else {
            format!("{}()", variant.name)
        };
        let match_arms = enm
            .variants
            .iter()
            .enumerate()
            .map(|(arm_index, entry)| {
                if entry.payload.is_some() {
                    format!(
                        "        {}(_) => {},",
                        entry.name,
                        if arm_index == index { 1 } else { 0 }
                    )
                } else {
                    format!(
                        "        {} => {},",
                        entry.name,
                        if arm_index == index { 1 } else { 0 }
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        tests.push(format!(
            "#[test]\nfn test_{}_variant_{}_is_covered() -> () {{\n    let value: {} = {};\n    let out = match value {{\n{}\n    }};\n    assert(out == 1);\n}}",
            sanitize_identifier(&target.name),
            sanitize_identifier(&variant.name),
            target.name,
            value,
            match_arms
        ));
    }

    let content = join_sections(&[
        render_module_and_imports(
            &format!(
                "generated.testgen.exhaustive.{}",
                sanitize_identifier(&target.name)
            ),
            &context.imports,
            false,
        ),
        enum_snippet,
        tests.join("\n\n"),
    ]);

    Ok(vec![TestgenArtifact {
        kind: "attribute-test-fixture".to_string(),
        name: format!("{}_exhaustive", target.name),
        path_hint: format!(
            "tests/generated/exhaustive_{}.aic",
            sanitize_identifier(&target.name)
        ),
        content,
        written_path: None,
        reason: Some(
            "one generated attribute test per enum variant with an exhaustive match body"
                .to_string(),
        ),
    }])
}

fn build_effect_artifacts(
    _project_root: &Path,
    target: &SymbolRecord,
    type_index: &TypeIndex,
    parsed_cache: &mut BTreeMap<PathBuf, ParsedFile>,
    notes: &mut Vec<String>,
    seed: u64,
) -> anyhow::Result<Vec<TestgenArtifact>> {
    if target.kind != SymbolKind::Function {
        bail!(
            "strategy `effect-coverage` requires a function target; got {} `{}`",
            target.kind.as_str(),
            target.name
        );
    }

    let context = load_context(target, parsed_cache)?;
    let function = find_function(&context.program, target)?;
    let function_snippet = snippet_for_span(&context.source, function.span)?;
    let args = render_function_args(&function.params, type_index, &BTreeMap::new(), seed)?;

    let mut type_names = BTreeSet::new();
    for param in &function.params {
        collect_named_types(&param.ty, &mut type_names);
    }
    collect_named_types(&function.ret_type, &mut type_names);
    let dependency_snippets = collect_dependency_snippets(type_index, &type_names);

    let mut declared_effects = function.effects.clone();
    let mut declared_capabilities = function.capabilities.clone();
    if !declared_effects.contains(&"io".to_string()) {
        declared_effects.push("io".to_string());
    }
    if !declared_capabilities.contains(&"io".to_string()) {
        declared_capabilities.push("io".to_string());
    }
    declared_effects.sort();
    declared_effects.dedup();
    declared_capabilities.sort();
    declared_capabilities.dedup();

    let run_pass = join_sections(&[
        format!(
            "// expect: 1\n{}",
            render_module_and_imports(
                &format!(
                    "generated.testgen.effect.{}_declared",
                    sanitize_identifier(&target.name)
                ),
                &context.imports,
                true,
            )
        ),
        render_snippet_block(&dependency_snippets),
        function_snippet.clone(),
        format!(
            "fn main() -> Int{}{} {{\n    let observed = {}({});\n    observed;\n    print_int(1);\n    0\n}}",
            render_effects_clause(&declared_effects),
            render_capabilities_clause(&declared_capabilities),
            function.name,
            args
        ),
    ]);

    let mut artifacts = vec![TestgenArtifact {
        kind: "run-pass-fixture".to_string(),
        name: format!("{}_effect_declared", target.name),
        path_hint: format!(
            "tests/generated/run-pass/effect_{}_declared.aic",
            sanitize_identifier(&target.name)
        ),
        content: run_pass,
        written_path: None,
        reason: Some("effectful wrapper declares the required effect/capability set".to_string()),
    }];

    if function.effects.is_empty() && function.capabilities.is_empty() {
        notes.push(
            "target is pure, so the compile-fail missing-effect fixture was skipped".to_string(),
        );
        return Ok(artifacts);
    }

    let compile_fail = join_sections(&[
        format!(
            "// expect-error: {}\n{}",
            EFFECT_DIAGNOSTIC_CODE,
            render_module_and_imports(
                &format!(
                    "generated.testgen.effect.{}_missing_effect",
                    sanitize_identifier(&target.name)
                ),
                &context.imports,
                false,
            )
        ),
        render_snippet_block(&dependency_snippets),
        function_snippet,
        format!(
            "fn main() -> Int {{\n    let observed = {}({});\n    observed\n}}",
            function.name, args
        ),
    ]);
    artifacts.push(TestgenArtifact {
        kind: "compile-fail-fixture".to_string(),
        name: format!("{}_effect_missing_effect", target.name),
        path_hint: format!(
            "tests/generated/compile-fail/effect_{}_missing_effect.aic",
            sanitize_identifier(&target.name)
        ),
        content: compile_fail,
        written_path: None,
        reason: Some(
            "missing-effect coverage fixture expected to fail with the canonical diagnostic"
                .to_string(),
        ),
    });

    Ok(artifacts)
}

fn parse_target_selector(tokens: &[String]) -> anyhow::Result<TargetSelector> {
    if tokens.is_empty() {
        bail!("--for requires a target selector");
    }

    let joined = tokens.join(" ");
    let parts = joined
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        bail!("--for requires a non-empty target selector");
    }

    let (kind, raw_name) = if let Some(kind) = parse_kind_label(&parts[0]) {
        if parts.len() < 2 {
            bail!("--for requires a symbol name after `{}`", parts[0]);
        }
        (Some(kind), parts[1..].join(" "))
    } else {
        (None, parts.join(" "))
    };

    let raw_name = raw_name.trim();
    if raw_name.is_empty() {
        bail!("--for requires a non-empty symbol name");
    }

    let (module, name) = split_module_and_name(raw_name);
    Ok(TargetSelector { kind, name, module })
}

fn parse_kind_label(raw: &str) -> Option<SymbolKind> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "function" | "fn" => Some(SymbolKind::Function),
        "struct" => Some(SymbolKind::Struct),
        "enum" => Some(SymbolKind::Enum),
        _ => None,
    }
}

fn split_module_and_name(raw: &str) -> (Option<String>, String) {
    if let Some((module, name)) = raw.rsplit_once('.') {
        if !module.trim().is_empty() && !name.trim().is_empty() {
            return (Some(module.trim().to_string()), name.trim().to_string());
        }
    }
    (None, raw.trim().to_string())
}

fn select_target_symbol<'a>(
    symbols: &'a [SymbolRecord],
    selector: &TargetSelector,
) -> anyhow::Result<&'a SymbolRecord> {
    let mut candidates = symbols
        .iter()
        .filter(|symbol| symbol.name == selector.name)
        .filter(|symbol| selector.kind.is_none_or(|kind| symbol.kind == kind))
        .filter(|symbol| {
            selector
                .module
                .as_ref()
                .is_none_or(|module| symbol.module.as_ref() == Some(module))
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|lhs, rhs| {
        lhs.kind
            .as_str()
            .cmp(rhs.kind.as_str())
            .then(lhs.module.cmp(&rhs.module))
            .then(lhs.location.file.cmp(&rhs.location.file))
            .then(lhs.location.span_start.cmp(&rhs.location.span_start))
    });

    match candidates.as_slice() {
        [] => bail!("unknown testgen target `{}`", selector.name),
        [symbol] => Ok(*symbol),
        many => {
            let rendered = many
                .iter()
                .map(|symbol| {
                    let module = symbol
                        .module
                        .clone()
                        .unwrap_or_else(|| "<root>".to_string());
                    format!("{} {} ({module})", symbol.kind.as_str(), symbol.name)
                })
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "ambiguous testgen target `{}`; candidates: {rendered}",
                selector.name
            )
        }
    }
}

fn build_type_index(
    symbols: &[SymbolRecord],
    parsed_cache: &mut BTreeMap<PathBuf, ParsedFile>,
) -> anyhow::Result<TypeIndex> {
    let mut index = TypeIndex::default();
    for symbol in symbols
        .iter()
        .filter(|symbol| matches!(symbol.kind, SymbolKind::Struct | SymbolKind::Enum))
    {
        let path = PathBuf::from(&symbol.location.file);
        let parsed = load_parsed_file(parsed_cache, &path)?;
        match symbol.kind {
            SymbolKind::Struct => {
                if let Some(strukt) = parsed.program.items.iter().find_map(|item| match item {
                    ast::Item::Struct(strukt)
                        if strukt.name == symbol.name && span_matches(symbol, strukt.span) =>
                    {
                        Some(strukt)
                    }
                    _ => None,
                }) {
                    index
                        .structs
                        .entry(strukt.name.clone())
                        .or_insert(StructContext {
                            def: strukt.clone(),
                            snippet: snippet_for_span(&parsed.source, strukt.span)?,
                        });
                }
            }
            SymbolKind::Enum => {
                if let Some(enm) = parsed.program.items.iter().find_map(|item| match item {
                    ast::Item::Enum(enm)
                        if enm.name == symbol.name && span_matches(symbol, enm.span) =>
                    {
                        Some(enm)
                    }
                    _ => None,
                }) {
                    index.enums.entry(enm.name.clone()).or_insert(EnumContext {
                        def: enm.clone(),
                        snippet: snippet_for_span(&parsed.source, enm.span)?,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(index)
}

fn load_context(
    target: &SymbolRecord,
    parsed_cache: &mut BTreeMap<PathBuf, ParsedFile>,
) -> anyhow::Result<ParsedContext> {
    let path = PathBuf::from(&target.location.file);
    let parsed = load_parsed_file(parsed_cache, &path)?;
    let imports = parsed
        .program
        .imports
        .iter()
        .map(|import| format!("import {};", import.path.join(".")))
        .collect::<Vec<_>>();
    Ok(ParsedContext {
        source: parsed.source.clone(),
        program: parsed.program.clone(),
        imports,
    })
}

fn load_parsed_file<'a>(
    cache: &'a mut BTreeMap<PathBuf, ParsedFile>,
    path: &Path,
) -> anyhow::Result<&'a ParsedFile> {
    if !cache.contains_key(path) {
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let (program, diagnostics) = parser::parse(&source, &path.to_string_lossy());
        if diagnostics.iter().any(|diag| diag.is_error()) {
            bail!(
                "failed to parse {} for testgen context: {diagnostics:#?}",
                path.display()
            );
        }
        let program =
            program.ok_or_else(|| anyhow!("parser returned no AST for {}", path.display()))?;
        cache.insert(path.to_path_buf(), ParsedFile { source, program });
    }
    cache
        .get(path)
        .ok_or_else(|| anyhow!("missing parsed file for {}", path.display()))
}

fn find_function<'a>(
    program: &'a ast::Program,
    target: &SymbolRecord,
) -> anyhow::Result<&'a ast::Function> {
    program
        .items
        .iter()
        .find_map(|item| match item {
            ast::Item::Function(function)
                if function.name == target.name && span_matches(target, function.span) =>
            {
                Some(function)
            }
            _ => None,
        })
        .ok_or_else(|| anyhow!("failed to resolve function `{}` from source", target.name))
}

fn find_struct<'a>(
    program: &'a ast::Program,
    target: &SymbolRecord,
) -> anyhow::Result<&'a ast::StructDef> {
    program
        .items
        .iter()
        .find_map(|item| match item {
            ast::Item::Struct(strukt)
                if strukt.name == target.name && span_matches(target, strukt.span) =>
            {
                Some(strukt)
            }
            _ => None,
        })
        .ok_or_else(|| anyhow!("failed to resolve struct `{}` from source", target.name))
}

fn find_enum<'a>(
    program: &'a ast::Program,
    target: &SymbolRecord,
) -> anyhow::Result<&'a ast::EnumDef> {
    program
        .items
        .iter()
        .find_map(|item| match item {
            ast::Item::Enum(enm) if enm.name == target.name && span_matches(target, enm.span) => {
                Some(enm)
            }
            _ => None,
        })
        .ok_or_else(|| anyhow!("failed to resolve enum `{}` from source", target.name))
}

fn render_module_and_imports(
    module_name: &str,
    imports: &[String],
    include_std_io: bool,
) -> String {
    let mut rendered_imports = imports.iter().cloned().collect::<BTreeSet<_>>();
    if include_std_io {
        rendered_imports.insert("import std.io;".to_string());
    }

    let mut lines = vec![format!("module {module_name};")];
    lines.extend(rendered_imports);
    lines.join("\n")
}

fn render_snippet_block(snippets: &[String]) -> String {
    snippets.join("\n\n")
}

fn collect_dependency_snippets(type_index: &TypeIndex, names: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut pending = names.iter().cloned().collect::<Vec<_>>();
    pending.sort();

    while let Some(name) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(strukt) = type_index.structs.get(&name) {
            out.push(strukt.snippet.clone());
            let mut nested = BTreeSet::new();
            for field in &strukt.def.fields {
                collect_named_types(&field.ty, &mut nested);
            }
            for item in nested.into_iter().rev() {
                pending.push(item);
            }
            continue;
        }
        if let Some(enm) = type_index.enums.get(&name) {
            out.push(enm.snippet.clone());
            let mut nested = BTreeSet::new();
            for variant in &enm.def.variants {
                if let Some(payload) = &variant.payload {
                    collect_named_types(payload, &mut nested);
                }
            }
            for item in nested.into_iter().rev() {
                pending.push(item);
            }
        }
    }

    out.sort();
    out.dedup();
    out
}

fn collect_named_types(ty: &TypeExpr, out: &mut BTreeSet<String>) {
    match &ty.kind {
        TypeKind::Named { name, args } => {
            if !is_builtin_type(name) {
                out.insert(name.clone());
            }
            for arg in args {
                collect_named_types(arg, out);
            }
        }
        TypeKind::DynTrait { .. } | TypeKind::Unit | TypeKind::Hole => {}
    }
}

fn materialize_artifacts(
    artifacts: &mut [TestgenArtifact],
    project_root: &Path,
    emit_dir: &Path,
) -> anyhow::Result<()> {
    let emit_dir_rel = emit_dir
        .strip_prefix(project_root)
        .ok()
        .filter(|path| !path.as_os_str().is_empty());
    for artifact in artifacts {
        let artifact_hint = Path::new(&artifact.path_hint);
        let artifact_tail = emit_dir_rel
            .and_then(|prefix| artifact_hint.strip_prefix(prefix).ok())
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(artifact_hint);
        let destination = emit_dir.join(artifact_tail);
        let parent = destination.parent().ok_or_else(|| {
            anyhow!(
                "cannot materialize artifact `{}` without a parent directory",
                artifact.path_hint
            )
        })?;
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create testgen output directory {}",
                parent.display()
            )
        })?;
        fs::write(&destination, &artifact.content).with_context(|| {
            format!(
                "failed to write generated artifact {}",
                destination.display()
            )
        })?;
        artifact.written_path = Some(
            destination
                .strip_prefix(project_root)
                .unwrap_or(&destination)
                .to_string_lossy()
                .to_string(),
        );
    }
    Ok(())
}

fn render_function_args(
    params: &[ast::Param],
    type_index: &TypeIndex,
    overrides: &BTreeMap<String, String>,
    seed: u64,
) -> anyhow::Result<String> {
    params
        .iter()
        .map(|param| {
            render_value_for_type(
                &param.ty,
                type_index,
                overrides.get(&param.name).map(|value| value.as_str()),
                seed,
                &param.name,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .map(|values| values.join(", "))
}

fn render_value_for_type(
    ty: &TypeExpr,
    type_index: &TypeIndex,
    override_value: Option<&str>,
    seed: u64,
    label: &str,
) -> anyhow::Result<String> {
    if let Some(value) = override_value {
        return Ok(value.to_string());
    }

    match &ty.kind {
        TypeKind::Unit => Ok("()".to_string()),
        TypeKind::Named { name, args } => match name.as_str() {
            "Int" | "I8" | "I16" | "I32" | "I64" | "ISize" => {
                Ok(seeded_small_int(seed, label).to_string())
            }
            "UInt" | "U8" | "U16" | "U32" | "U64" | "USize" => {
                Ok(seeded_small_int(seed, label).to_string())
            }
            "Bool" => Ok(if seeded_index(seed, label, 2) == 0 {
                "true".to_string()
            } else {
                "false".to_string()
            }),
            "String" => Ok(format!(
                "\"{}-{}\"",
                sanitize_identifier(label),
                seeded_index(seed, label, 97)
            )),
            "Float" | "Float32" | "Float64" => Ok(format!("{}.0", seeded_small_int(seed, label))),
            "Option" => {
                let inner = args
                    .first()
                    .ok_or_else(|| anyhow!("Option requires one type argument"))?;
                if seeded_index(seed, label, 2) == 0 {
                    Ok("None".to_string())
                } else {
                    Ok(format!(
                        "Some({})",
                        render_value_for_type(
                            inner,
                            type_index,
                            None,
                            seed,
                            &format!("{label}.some"),
                        )?
                    ))
                }
            }
            "Result" => {
                let inner = args
                    .first()
                    .ok_or_else(|| anyhow!("Result requires an ok type argument"))?;
                Ok(format!(
                    "Ok({})",
                    render_value_for_type(inner, type_index, None, seed, &format!("{label}.ok"))?
                ))
            }
            custom => {
                if let Some(strukt) = type_index.structs.get(custom) {
                    let fields = strukt
                        .def
                        .fields
                        .iter()
                        .map(|field| {
                            Ok(format!(
                                "{}: {}",
                                field.name,
                                render_value_for_type(
                                    &field.ty,
                                    type_index,
                                    None,
                                    seed,
                                    &format!("{label}.{}", field.name),
                                )?
                            ))
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    return Ok(format!("{} {{ {} }}", custom, fields.join(", ")));
                }
                if let Some(enm) = type_index.enums.get(custom) {
                    let index = seeded_index(seed, label, enm.def.variants.len());
                    let first = enm
                        .def
                        .variants
                        .get(index)
                        .ok_or_else(|| anyhow!("enum `{custom}` has no variants"))?;
                    if let Some(payload) = &first.payload {
                        return Ok(format!(
                            "{}({})",
                            first.name,
                            render_value_for_type(
                                payload,
                                type_index,
                                None,
                                seed,
                                &format!("{label}.{}", first.name),
                            )?
                        ));
                    }
                    return Ok(first.name.clone());
                }
                bail!("testgen does not yet know how to synthesize a value for type `{custom}`")
            }
        },
        TypeKind::DynTrait { trait_name } => {
            bail!("testgen does not support dynamic trait parameter `{trait_name}` yet")
        }
        TypeKind::Hole => bail!("testgen does not support type holes in generated values"),
    }
}

fn seeded_index(seed: u64, label: &str, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (stable_hash(seed, label) % len as u64) as usize
}

fn seeded_small_int(seed: u64, label: &str) -> i64 {
    (stable_hash(seed, label) % 5) as i64 + 1
}

fn stable_hash(seed: u64, label: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    label.hash(&mut hasher);
    hasher.finish()
}

fn derive_boundary_cases(expr: &Expr) -> Vec<BoundaryCase> {
    let mut out = Vec::new();
    collect_boundary_cases(expr, &mut out);
    out.sort_by(|lhs, rhs| lhs.param.cmp(&rhs.param).then(lhs.label.cmp(&rhs.label)));
    out.dedup_by(|lhs, rhs| lhs.param == rhs.param && lhs.label == rhs.label);
    out
}

fn collect_boundary_cases(expr: &Expr, out: &mut Vec<BoundaryCase>) {
    match &expr.kind {
        ExprKind::Binary {
            op: BinOp::And,
            lhs,
            rhs,
        }
        | ExprKind::Binary {
            op: BinOp::Or,
            lhs,
            rhs,
        } => {
            collect_boundary_cases(lhs, out);
            collect_boundary_cases(rhs, out);
        }
        ExprKind::Binary { .. } => {
            if let Some(case) = boundary_case(expr) {
                out.push(case);
            }
        }
        _ => {}
    }
}

fn boundary_case(expr: &Expr) -> Option<BoundaryCase> {
    let ExprKind::Binary { op, lhs, rhs } = &expr.kind else {
        return None;
    };

    if let (Some(param), Some(value)) = (path_name(lhs), int_literal(rhs)) {
        return boundary_from_comparison(param, *op, value);
    }
    if let (Some(param), Some(value)) = (path_name(rhs), int_literal(lhs)) {
        return boundary_from_comparison(param, flip_operator(*op), value);
    }
    None
}

fn boundary_from_comparison(param: String, op: BinOp, value: i64) -> Option<BoundaryCase> {
    let (positive_value, negative_value, label) = match op {
        BinOp::Ge => (
            value.to_string(),
            (value - 1).to_string(),
            format!("{}_ge_{}", param, value),
        ),
        BinOp::Gt => (
            (value + 1).to_string(),
            value.to_string(),
            format!("{}_gt_{}", param, value),
        ),
        BinOp::Le => (
            value.to_string(),
            (value + 1).to_string(),
            format!("{}_le_{}", param, value),
        ),
        BinOp::Lt => (
            (value - 1).to_string(),
            value.to_string(),
            format!("{}_lt_{}", param, value),
        ),
        BinOp::Eq => (
            value.to_string(),
            (value + 1).to_string(),
            format!("{}_eq_{}", param, value),
        ),
        _ => return None,
    };
    Some(BoundaryCase {
        param,
        positive_value,
        negative_value,
        label,
    })
}

fn path_name(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Var(name) => Some(name.clone()),
        _ => None,
    }
}

fn int_literal(expr: &Expr) -> Option<i64> {
    match expr.kind {
        ExprKind::Int(value) => Some(value),
        _ => None,
    }
}

fn flip_operator(op: BinOp) -> BinOp {
    match op {
        BinOp::Lt => BinOp::Gt,
        BinOp::Le => BinOp::Ge,
        BinOp::Gt => BinOp::Lt,
        BinOp::Ge => BinOp::Le,
        other => other,
    }
}

fn render_effects_clause(effects: &[String]) -> String {
    if effects.is_empty() {
        String::new()
    } else {
        format!(" effects {{ {} }}", effects.join(", "))
    }
}

fn render_capabilities_clause(capabilities: &[String]) -> String {
    if capabilities.is_empty() {
        String::new()
    } else {
        format!(" capabilities {{ {} }}", capabilities.join(", "))
    }
}

fn snippet_for_span(source: &str, span: Span) -> anyhow::Result<String> {
    let start = span.start.min(source.len());
    let end = span.end.min(source.len());
    if start > end || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        bail!("invalid source span {}..{}", span.start, span.end);
    }
    Ok(source[start..end].trim().to_string())
}

fn span_matches(target: &SymbolRecord, span: Span) -> bool {
    target.location.span_start == span.start && target.location.span_end == span.end
}

fn sanitize_identifier(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '_' || ch == '-' || ch == '.' {
            if !out.ends_with('_') {
                out.push('_');
            }
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "generated".to_string()
    } else {
        trimmed
    }
}

fn join_sections(parts: &[String]) -> String {
    parts
        .iter()
        .filter(|part| !part.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "Int"
            | "I8"
            | "I16"
            | "I32"
            | "I64"
            | "ISize"
            | "UInt"
            | "U8"
            | "U16"
            | "U32"
            | "U64"
            | "USize"
            | "Float"
            | "Float32"
            | "Float64"
            | "Bool"
            | "String"
            | "Option"
            | "Result"
            | "Unit"
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{derive_boundary_cases, generate, generate_tests, TestgenStrategy};
    use crate::attr_test_runner::run_attribute_tests;
    use crate::parser;
    use crate::test_harness::{run_harness, HarnessMode};
    use tempfile::tempdir;

    #[test]
    fn boundary_case_derivation_handles_comparisons() {
        let source = "fn demo(x: Int) -> Int requires x >= 0 ensures result >= 0 { x }";
        let (program, diagnostics) = parser::parse(source, "<memory>");
        assert!(diagnostics.iter().all(|diag| !diag.is_error()));
        let program = program.expect("program");
        let function = match &program.items[0] {
            crate::ast::Item::Function(function) => function,
            _ => panic!("expected function"),
        };
        let cases = derive_boundary_cases(function.requires.as_ref().expect("requires"));
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].param, "x");
        assert_eq!(cases[0].positive_value, "0");
        assert_eq!(cases[0].negative_value, "-1");
    }

    #[test]
    fn testgen_generates_effect_fixtures_for_simple_function() {
        let project = tempdir().expect("tempdir");
        std::fs::create_dir_all(project.path().join("src")).expect("mkdir src");
        std::fs::write(
            project.path().join("aic.toml"),
            "[package]\nname = \"testgen_fixture\"\nversion = \"0.1.0\"\n",
        )
        .expect("write manifest");
        std::fs::write(
            project.path().join("src/main.aic"),
            concat!(
                "module demo.testgen;\n",
                "import std.io;\n\n",
                "fn emit_signal(x: Int) -> Int effects { io } capabilities { io } {\n",
                "    print_int(x);\n",
                "    x\n",
                "}\n",
            ),
        )
        .expect("write source");

        let response = generate_tests(
            project.path(),
            TestgenStrategy::EffectCoverage,
            &["function".to_string(), "emit_signal".to_string()],
            7,
        )
        .expect("testgen response");
        assert_eq!(response.phase, "testgen");
        assert_eq!(response.strategy, "effect-coverage");
        assert_eq!(response.artifacts.len(), 2);
        assert!(response
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == "compile-fail-fixture"));
        assert!(response
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == "run-pass-fixture"));
    }

    fn write_testgen_fixture(root: &std::path::Path) {
        std::fs::create_dir_all(root.join("src")).expect("mkdir src");
        std::fs::write(
            root.join("aic.toml"),
            "[package]\nname = \"testgen_fixture\"\nversion = \"0.1.0\"\n",
        )
        .expect("write manifest");
        std::fs::write(
            root.join("src/main.aic"),
            concat!(
                "module demo.testgen;\n",
                "import std.io;\n\n",
                "struct User {\n",
                "    age: Int,\n",
                "} invariant age >= 0\n\n",
                "enum WorkflowState {\n",
                "    Idle,\n",
                "    Running(Int),\n",
                "    Failed,\n",
                "}\n\n",
                "fn normalize_age(age: Int) -> Int requires age >= 0 ensures result >= 0 {\n",
                "    age\n",
                "}\n\n",
                "fn emit_signal(x: Int) -> Int effects { io } capabilities { io } {\n",
                "    print_int(x);\n",
                "    x\n",
                "}\n",
            ),
        )
        .expect("write source");
    }

    #[test]
    fn testgen_materializes_and_executes_all_strategy_outputs() {
        let project = tempdir().expect("tempdir");
        write_testgen_fixture(project.path());

        generate(
            project.path(),
            TestgenStrategy::Boundary,
            &["function".to_string(), "normalize_age".to_string()],
            11,
            Some(project.path()),
        )
        .expect("generate boundary");
        generate(
            project.path(),
            TestgenStrategy::InvariantViolation,
            &["struct".to_string(), "User".to_string()],
            11,
            Some(project.path()),
        )
        .expect("generate invariant");
        generate(
            project.path(),
            TestgenStrategy::ExhaustiveMatch,
            &["enum".to_string(), "WorkflowState".to_string()],
            11,
            Some(project.path()),
        )
        .expect("generate exhaustive");
        generate(
            project.path(),
            TestgenStrategy::EffectCoverage,
            &["function".to_string(), "emit_signal".to_string()],
            11,
            Some(project.path()),
        )
        .expect("generate effect");

        let harness = run_harness(project.path(), HarnessMode::All).expect("run harness");
        assert_eq!(harness.failed, 0, "harness cases: {:#?}", harness.cases);
        assert_eq!(harness.total, 3);

        let attr = run_attribute_tests(project.path(), Some("test_"), 11).expect("run attrs");
        assert_eq!(attr.failed, 0, "attr cases: {:#?}", attr.cases);
        assert_eq!(attr.total, 6);
    }

    #[test]
    fn testgen_output_is_deterministic_for_fixed_seed() {
        let project = tempdir().expect("tempdir");
        write_testgen_fixture(project.path());

        let first = generate_tests(
            project.path(),
            TestgenStrategy::Boundary,
            &["function".to_string(), "normalize_age".to_string()],
            19,
        )
        .expect("first response");
        let second = generate_tests(
            project.path(),
            TestgenStrategy::Boundary,
            &["function".to_string(), "normalize_age".to_string()],
            19,
        )
        .expect("second response");

        assert_eq!(first, second);
    }

    #[test]
    fn testgen_emit_dir_is_resolved_from_project_root_without_duplicate_prefixes() {
        let project = tempdir().expect("tempdir");
        write_testgen_fixture(project.path());

        let response = generate(
            project.path(),
            TestgenStrategy::Boundary,
            &["function".to_string(), "normalize_age".to_string()],
            19,
            Some(Path::new("tests/generated")),
        )
        .expect("generate boundary into tests/generated");

        let artifact = response
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == "attribute-test-fixture")
            .expect("boundary artifact");
        assert_eq!(
            artifact.written_path.as_deref(),
            Some("tests/generated/boundary_normalize_age.aic")
        );
        assert!(
            project
                .path()
                .join("tests/generated/boundary_normalize_age.aic")
                .exists(),
            "expected materialized artifact in project-relative tests/generated"
        );
        assert!(
            !project
                .path()
                .join("tests/generated/tests/generated/boundary_normalize_age.aic")
                .exists(),
            "testgen must not duplicate the tests/generated prefix"
        );
    }

    #[test]
    fn testgen_rejects_unsupported_strategy_target_pairs() {
        let project = tempdir().expect("tempdir");
        write_testgen_fixture(project.path());

        let err = generate_tests(
            project.path(),
            TestgenStrategy::Boundary,
            &["struct".to_string(), "User".to_string()],
            0,
        )
        .expect_err("boundary should reject struct target");

        assert!(err
            .to_string()
            .contains("strategy `boundary` requires a function target"));
    }
}
