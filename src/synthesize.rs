use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context};
use serde::Serialize;

use crate::ast::{self, BinOp, Expr, ExprKind, Item, TypeExpr, TypeKind, UnaryOp};
use crate::parser;
use crate::scaffold::{self, FnScaffoldOptions, ParamSpec};

const SYNTHESIZE_PROTOCOL_VERSION: &str = "1.0";
const SYNTHESIZE_PHASE: &str = "synthesize";
const SOURCE_KIND_SPEC: &str = "spec";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SynthesizeResponse {
    pub protocol_version: &'static str,
    pub phase: &'static str,
    pub source_kind: &'static str,
    pub target: String,
    pub spec_file: String,
    pub artifacts: Vec<SynthesizeArtifact>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SynthesizeArtifact {
    pub kind: String,
    pub name: String,
    pub path_hint: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedSpec {
    file: PathBuf,
    function: ast::Function,
    raw_requires: Option<String>,
    raw_ensures: Option<String>,
    auto_capabilities: bool,
}

#[derive(Debug, Clone, Default)]
struct SpecClauses {
    requires: Option<String>,
    ensures: Option<String>,
    effects: Vec<String>,
    capabilities: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct TypeIndex {
    structs: BTreeMap<String, Vec<StructShape>>,
    enums: BTreeMap<String, Vec<EnumShape>>,
}

#[derive(Debug, Clone)]
struct StructShape {
    name: String,
    fields: Vec<FieldShape>,
    invariant: Option<Expr>,
    invariant_text: Option<String>,
}

#[derive(Debug, Clone)]
struct FieldShape {
    name: String,
    ty: TypeExpr,
}

#[derive(Debug, Clone)]
struct EnumShape {
    name: String,
    variants: Vec<VariantShape>,
}

#[derive(Debug, Clone)]
struct VariantShape {
    name: String,
    payload: Option<TypeExpr>,
}

#[derive(Debug, Clone)]
struct BoundaryCase {
    path: Vec<String>,
    positive_value: String,
    negative_value: String,
    label: String,
}

#[derive(Debug, Clone)]
enum LiteralValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    String(String),
}

type OverrideMap = BTreeMap<Vec<String>, String>;

pub fn synthesize_from_spec(
    project_root: &Path,
    target: &str,
) -> anyhow::Result<SynthesizeResponse> {
    let spec = load_spec(project_root, target)?;
    let type_index = build_type_index(project_root)?;
    let body_expr = synthesize_body_expression(&spec, &type_index)?;
    let materialized_ensures = materialized_ensures(&spec);
    let options = synthesize_fn_options(&spec, materialized_ensures.clone());
    let function_scaffold = scaffold::scaffold_function_with_body(&options, &body_expr);
    let fixture = build_self_contained_fixture(&spec, &type_index, &function_scaffold.content)?;

    let base_name = sanitize_identifier(&spec.function.name);
    let mut notes = Vec::new();
    if spec.auto_capabilities {
        notes.push(
            "mirrored declared effects into capabilities because executable functions currently require capability authority".to_string(),
        );
    }
    if spec.raw_ensures.is_some() && materialized_ensures.is_none() {
        if let Some(ensures) = &spec.raw_ensures {
            notes.push(format!(
                "omitted non-lowerable ensures clause from runnable artifacts: {}",
                ensures.trim()
            ));
        }
    }
    if fixture.1 {
        notes.push(
            "no supported requires boundary was found; generated the failing test from a result-only ensures clause instead"
                .to_string(),
        );
    }

    Ok(SynthesizeResponse {
        protocol_version: SYNTHESIZE_PROTOCOL_VERSION,
        phase: SYNTHESIZE_PHASE,
        source_kind: SOURCE_KIND_SPEC,
        target: spec.function.name.clone(),
        spec_file: spec.file.to_string_lossy().to_string(),
        artifacts: vec![
            SynthesizeArtifact {
                kind: "function".to_string(),
                name: spec.function.name.clone(),
                path_hint: format!("src/generated/{base_name}.aic"),
                content: function_scaffold.content,
                reason: Some(
                    "materialize this into project source once the synthesized body is ready for iterative refinement"
                        .to_string(),
                ),
            },
            SynthesizeArtifact {
                kind: "attribute-test-fixture".to_string(),
                name: format!("{}_spec_tests", spec.function.name),
                path_hint: format!("tests/generated/{}_spec_tests.aic", base_name),
                content: fixture.0,
                reason: Some(
                    "self-contained happy-path and failing contract-boundary tests for `aic test`"
                        .to_string(),
                ),
            },
        ],
        notes,
    })
}

pub fn format_text(response: &SynthesizeResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "synthesize: {} {}",
        response.source_kind, response.target
    ));
    lines.push(format!("spec file: {}", response.spec_file));
    if !response.notes.is_empty() {
        lines.push("notes:".to_string());
        for note in &response.notes {
            lines.push(format!("  - {note}"));
        }
    }

    for artifact in &response.artifacts {
        lines.push(String::new());
        lines.push(format!(
            "artifact {} -> {}",
            artifact.kind, artifact.path_hint
        ));
        if let Some(reason) = &artifact.reason {
            lines.push(format!("reason: {reason}"));
        }
        lines.push(artifact.content.clone());
    }

    lines.join("\n")
}

fn load_spec(project_root: &Path, target: &str) -> anyhow::Result<ParsedSpec> {
    let mut files = Vec::new();
    collect_aic_files(project_root, &mut files)?;
    files.sort();

    let mut matches = Vec::new();
    for file in files {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        if !source.contains("spec fn") {
            continue;
        }
        matches.extend(extract_specs_from_source(&source, &file)?);
    }

    let mut selected = matches
        .into_iter()
        .filter(|spec| spec.function.name == target)
        .collect::<Vec<_>>();

    match selected.len() {
        0 => bail!(
            "no `spec fn {target}` found under {}; place specs in `specs/*.aic` or any non-generated `.aic` file outside the compile path",
            project_root.display()
        ),
        1 => Ok(selected.remove(0)),
        _ => {
            let files = selected
                .into_iter()
                .map(|spec| spec.file.to_string_lossy().to_string())
                .collect::<Vec<_>>();
            bail!(
                "multiple `spec fn {target}` definitions found: {}",
                files.join(", ")
            )
        }
    }
}

fn extract_specs_from_source(source: &str, file: &Path) -> anyhow::Result<Vec<ParsedSpec>> {
    let mut specs = Vec::new();
    let mut cursor = 0usize;

    while let Some(start) = find_next_spec_start(source, cursor) {
        let (spec, end) = parse_spec_at(source, file, start)?;
        specs.push(spec);
        cursor = end;
    }

    Ok(specs)
}

fn find_next_spec_start(source: &str, from: usize) -> Option<usize> {
    for (rel, _) in source[from..].match_indices("spec fn") {
        let idx = from + rel;
        let line_start = source[..idx]
            .rfind('\n')
            .map(|value| value + 1)
            .unwrap_or(0);
        if source[line_start..idx].trim().is_empty() {
            return Some(idx);
        }
    }
    None
}

fn parse_spec_at(source: &str, file: &Path, start: usize) -> anyhow::Result<(ParsedSpec, usize)> {
    let after_spec = start + "spec ".len();
    let body_start = find_spec_body_start(source, after_spec)?;
    let body_end = find_matching_brace(source, body_start)?;
    let header = source[after_spec..body_start].trim();
    if !header.starts_with("fn ") {
        bail!(
            "{}: spec declarations must start with `spec fn`",
            file.display()
        );
    }
    for keyword in [" effects ", " capabilities ", " requires ", " ensures "] {
        if header.contains(keyword) {
            bail!(
                "{}: keep `effects`, `capabilities`, `requires`, and `ensures` inside the spec body for `aic synthesize`",
                file.display()
            );
        }
    }

    let body = &source[body_start + 1..body_end];
    let mut clauses = parse_spec_body(body).with_context(|| format!("in {}", file.display()))?;
    let auto_capabilities = clauses.capabilities.is_empty() && !clauses.effects.is_empty();
    if auto_capabilities {
        clauses.capabilities = clauses.effects.clone();
    }
    let synthetic = build_synthetic_function_source(header, &clauses);
    let function = parse_synthetic_function(&synthetic)?;

    Ok((
        ParsedSpec {
            file: file.to_path_buf(),
            function,
            raw_requires: clauses.requires,
            raw_ensures: clauses.ensures,
            auto_capabilities,
        },
        body_end + 1,
    ))
}

fn find_spec_body_start(source: &str, start: usize) -> anyhow::Result<usize> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in source[start..].char_indices() {
        let idx = start + offset;
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' if paren_depth == 0 && bracket_depth == 0 => return Ok(idx),
            _ => {}
        }
    }

    bail!("unterminated spec header")
}

fn find_matching_brace(source: &str, open_brace: usize) -> anyhow::Result<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in source[open_brace..].char_indices() {
        let idx = open_brace + offset;
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(idx);
                }
            }
            _ => {}
        }
    }

    bail!("unterminated spec body")
}

fn parse_spec_body(body: &str) -> anyhow::Result<SpecClauses> {
    let mut clauses = SpecClauses::default();
    for line in body.lines() {
        let raw = strip_line_comment(line).trim();
        if raw.is_empty() {
            continue;
        }
        if let Some(rest) = raw.strip_prefix("requires ") {
            if clauses.requires.is_some() {
                bail!("duplicate `requires` clause");
            }
            clauses.requires = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = raw.strip_prefix("ensures ") {
            if clauses.ensures.is_some() {
                bail!("duplicate `ensures` clause");
            }
            clauses.ensures = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = raw.strip_prefix("effects") {
            if !clauses.effects.is_empty() {
                bail!("duplicate `effects` clause");
            }
            clauses.effects = parse_braced_clause(rest, "effects")?;
            continue;
        }
        if let Some(rest) = raw.strip_prefix("capabilities") {
            if !clauses.capabilities.is_empty() {
                bail!("duplicate `capabilities` clause");
            }
            clauses.capabilities = parse_braced_clause(rest, "capabilities")?;
            continue;
        }
        bail!("unsupported spec clause `{raw}`");
    }
    Ok(clauses)
}

fn strip_line_comment(line: &str) -> &str {
    line.split_once("//").map(|(head, _)| head).unwrap_or(line)
}

fn parse_braced_clause(rest: &str, keyword: &str) -> anyhow::Result<Vec<String>> {
    let trimmed = rest.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        bail!("`{keyword}` clauses must use `{{ ... }}`");
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    Ok(inner
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn build_synthetic_function_source(header: &str, clauses: &SpecClauses) -> String {
    let mut signature = header.trim().to_string();
    if !clauses.effects.is_empty() {
        signature.push_str(" effects { ");
        signature.push_str(&clauses.effects.join(", "));
        signature.push_str(" }");
    }
    if !clauses.capabilities.is_empty() {
        signature.push_str(" capabilities { ");
        signature.push_str(&clauses.capabilities.join(", "));
        signature.push_str(" }");
    }
    if let Some(requires) = &clauses.requires {
        signature.push_str(" requires ");
        signature.push_str(requires.trim());
    }
    if let Some(ensures) = &clauses.ensures {
        signature.push_str(" ensures ");
        signature.push_str(ensures.trim());
    }

    format!("module synth.internal;\n{signature} {{\n    ()\n}}\n")
}

fn parse_synthetic_function(source: &str) -> anyhow::Result<ast::Function> {
    let (program, diagnostics) = parser::parse(source, "<synthesize-spec>");
    if diagnostics.iter().any(|diag| diag.is_error()) {
        bail!("failed to parse synthesized runtime function from spec: {diagnostics:#?}");
    }
    let program = program.ok_or_else(|| anyhow!("synthetic spec parse produced no AST"))?;
    for item in program.items {
        if let Item::Function(function) = item {
            return Ok(function);
        }
    }
    bail!("synthetic spec parse did not produce a function")
}

fn build_type_index(project_root: &Path) -> anyhow::Result<TypeIndex> {
    let mut files = Vec::new();
    collect_aic_files(project_root, &mut files)?;
    files.sort();

    let mut index = TypeIndex::default();
    for file in files {
        if path_has_component(&file, "specs") {
            continue;
        }
        let source = match fs::read_to_string(&file) {
            Ok(source) => source,
            Err(_) => continue,
        };
        if source.contains("spec fn") {
            continue;
        }
        let (program, diagnostics) = parser::parse(&source, &file.to_string_lossy());
        if diagnostics.iter().any(|diag| diag.is_error()) {
            continue;
        }
        let Some(program) = program else {
            continue;
        };

        index.extend_from_program(&program, &source);
    }

    Ok(index)
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

fn path_has_component(path: &Path, needle: &str) -> bool {
    path.components()
        .any(|component| component.as_os_str().to_string_lossy() == needle)
}

impl TypeIndex {
    fn extend_from_program(&mut self, program: &ast::Program, source: &str) {
        for item in &program.items {
            match item {
                Item::Struct(strukt) => {
                    self.structs
                        .entry(strukt.name.clone())
                        .or_default()
                        .push(StructShape {
                            name: strukt.name.clone(),
                            fields: strukt
                                .fields
                                .iter()
                                .map(|field| FieldShape {
                                    name: field.name.clone(),
                                    ty: field.ty.clone(),
                                })
                                .collect(),
                            invariant: strukt.invariant.clone(),
                            invariant_text: strukt
                                .invariant
                                .as_ref()
                                .and_then(|expr| source.get(expr.span.start..expr.span.end))
                                .map(|value| value.trim().to_string()),
                        });
                }
                Item::Enum(enm) => {
                    self.enums
                        .entry(enm.name.clone())
                        .or_default()
                        .push(EnumShape {
                            name: enm.name.clone(),
                            variants: enm
                                .variants
                                .iter()
                                .map(|variant| VariantShape {
                                    name: variant.name.clone(),
                                    payload: variant.payload.clone(),
                                })
                                .collect(),
                        });
                }
                Item::Function(_) | Item::Trait(_) | Item::Impl(_) => {}
            }
        }
    }

    fn struct_shape(&self, name: &str) -> anyhow::Result<Option<&StructShape>> {
        match self.structs.get(name) {
            None => Ok(None),
            Some(entries) if entries.len() == 1 => Ok(entries.first()),
            Some(_) => bail!("type `{name}` is ambiguous across multiple project files"),
        }
    }

    fn enum_shape(&self, name: &str) -> anyhow::Result<Option<&EnumShape>> {
        match self.enums.get(name) {
            None => Ok(None),
            Some(entries) if entries.len() == 1 => Ok(entries.first()),
            Some(_) => bail!("type `{name}` is ambiguous across multiple project files"),
        }
    }
}

fn synthesize_fn_options(
    spec: &ParsedSpec,
    materialized_ensures: Option<String>,
) -> FnScaffoldOptions {
    FnScaffoldOptions {
        name: spec.function.name.clone(),
        params: spec
            .function
            .params
            .iter()
            .map(|param| ParamSpec {
                name: param.name.clone(),
                ty: render_type_expr(&param.ty),
            })
            .collect(),
        return_type: render_type_expr(&spec.function.ret_type),
        effects: spec.function.effects.clone(),
        capabilities: spec.function.capabilities.clone(),
        requires: spec.raw_requires.clone(),
        ensures: materialized_ensures,
    }
}

fn synthesize_body_expression(spec: &ParsedSpec, type_index: &TypeIndex) -> anyhow::Result<String> {
    if let Some(ensures) = spec.function.ensures.as_ref() {
        if let ExprKind::Binary {
            op: BinOp::Eq,
            lhs,
            rhs,
        } = &ensures.kind
        {
            if is_var_named(lhs, "result") {
                return Ok(render_expr(rhs)?);
            }
            if is_var_named(rhs, "result") {
                return Ok(render_expr(lhs)?);
            }
        }
    }

    default_value_for_type(&spec.function.ret_type, type_index, &OverrideMap::new())
}

fn build_self_contained_fixture(
    spec: &ParsedSpec,
    type_index: &TypeIndex,
    function_content: &str,
) -> anyhow::Result<(String, bool)> {
    let dependencies = collect_type_dependencies(&spec.function, type_index)?;
    let mut sections = Vec::new();
    for dependency in dependencies {
        sections.push(render_dependency(&dependency, type_index)?);
        sections.push(String::new());
    }
    sections.push(function_content.to_string());
    sections.push(String::new());

    let (tests, used_ensures_fallback) = build_generated_tests(spec, type_index)?;
    for test in tests {
        sections.push(test);
        sections.push(String::new());
    }

    while matches!(sections.last(), Some(last) if last.is_empty()) {
        sections.pop();
    }

    Ok((sections.join("\n"), used_ensures_fallback))
}

fn collect_type_dependencies(
    function: &ast::Function,
    type_index: &TypeIndex,
) -> anyhow::Result<Vec<String>> {
    let mut ordered = Vec::new();
    let mut visited = BTreeSet::new();
    let mut stack = BTreeSet::new();

    for param in &function.params {
        visit_type_dependencies(
            &param.ty,
            type_index,
            &mut visited,
            &mut stack,
            &mut ordered,
        )?;
    }
    visit_type_dependencies(
        &function.ret_type,
        type_index,
        &mut visited,
        &mut stack,
        &mut ordered,
    )?;

    Ok(ordered)
}

fn visit_type_dependencies(
    ty: &TypeExpr,
    type_index: &TypeIndex,
    visited: &mut BTreeSet<String>,
    stack: &mut BTreeSet<String>,
    ordered: &mut Vec<String>,
) -> anyhow::Result<()> {
    let TypeKind::Named { name, args } = &ty.kind else {
        return Ok(());
    };

    if is_builtin_type(name) {
        for arg in args {
            visit_type_dependencies(arg, type_index, visited, stack, ordered)?;
        }
        return Ok(());
    }

    if visited.contains(name) {
        return Ok(());
    }
    if !stack.insert(name.clone()) {
        return Ok(());
    }

    if let Some(strukt) = type_index.struct_shape(name)? {
        for field in &strukt.fields {
            visit_type_dependencies(&field.ty, type_index, visited, stack, ordered)?;
        }
        ordered.push(name.clone());
        visited.insert(name.clone());
        stack.remove(name);
        return Ok(());
    }

    if let Some(enm) = type_index.enum_shape(name)? {
        for variant in &enm.variants {
            if let Some(payload) = &variant.payload {
                visit_type_dependencies(payload, type_index, visited, stack, ordered)?;
            }
        }
        ordered.push(name.clone());
        visited.insert(name.clone());
        stack.remove(name);
        return Ok(());
    }

    bail!("cannot synthesize self-contained fixture because type `{name}` is not defined in project sources")
}

fn render_dependency(name: &str, type_index: &TypeIndex) -> anyhow::Result<String> {
    if let Some(strukt) = type_index.struct_shape(name)? {
        return Ok(render_struct_definition(strukt));
    }
    if let Some(enm) = type_index.enum_shape(name)? {
        return Ok(render_enum_definition(enm));
    }
    bail!("missing dependency definition for `{name}`")
}

fn render_struct_definition(strukt: &StructShape) -> String {
    let mut lines = Vec::new();
    lines.push(format!("struct {} {{", strukt.name));
    for field in &strukt.fields {
        lines.push(format!(
            "    {}: {},",
            field.name,
            render_type_expr(&field.ty)
        ));
    }
    lines.push("}".to_string());
    if let Some(invariant) = &strukt.invariant_text {
        lines.push(format!("invariant {}", invariant.trim()));
    }
    lines.join("\n")
}

fn render_enum_definition(enm: &EnumShape) -> String {
    let mut lines = Vec::new();
    lines.push(format!("enum {} {{", enm.name));
    for variant in &enm.variants {
        match &variant.payload {
            Some(payload) => {
                lines.push(format!(
                    "    {}({}),",
                    variant.name,
                    render_type_expr(payload)
                ));
            }
            None => lines.push(format!("    {},", variant.name)),
        }
    }
    lines.push("}".to_string());
    lines.join("\n")
}

fn build_generated_tests(
    spec: &ParsedSpec,
    type_index: &TypeIndex,
) -> anyhow::Result<(Vec<String>, bool)> {
    let boundaries = spec
        .function
        .requires
        .as_ref()
        .map(derive_boundary_cases)
        .unwrap_or_default();
    let positive_overrides = positive_overrides(&boundaries);
    let positive_bindings = build_param_bindings(&spec.function, type_index, &positive_overrides)?;
    let can_assert = spec
        .function
        .ensures
        .as_ref()
        .map(can_assert_ensures)
        .unwrap_or(false);

    let mut tests = Vec::new();
    tests.push(render_positive_test(
        spec,
        &positive_bindings,
        boundaries.is_empty() && can_assert,
    ));

    if !boundaries.is_empty() {
        for boundary in &boundaries {
            let negative_overrides = negative_overrides(&boundaries, boundary);
            let negative_bindings =
                build_param_bindings(&spec.function, type_index, &negative_overrides)?;
            tests.push(render_negative_boundary_test(
                spec,
                &negative_bindings,
                &boundary.label,
            ));
        }
        return Ok((tests, false));
    }

    if let (Some(ensures), Some(raw_ensures)) =
        (spec.function.ensures.as_ref(), spec.raw_ensures.as_ref())
    {
        let param_names = spec
            .function
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect::<BTreeSet<_>>();
        if !expr_mentions_params(ensures, &param_names) && can_assert {
            tests.push(render_negative_ensures_test(
                spec,
                &positive_bindings,
                raw_ensures.trim(),
            ));
            return Ok((tests, true));
        }
    }

    Ok((tests, false))
}

fn positive_overrides(boundaries: &[BoundaryCase]) -> OverrideMap {
    let mut overrides = OverrideMap::new();
    for boundary in boundaries {
        overrides.insert(boundary.path.clone(), boundary.positive_value.clone());
    }
    overrides
}

fn negative_overrides(boundaries: &[BoundaryCase], failing: &BoundaryCase) -> OverrideMap {
    let mut overrides = positive_overrides(boundaries);
    overrides.insert(failing.path.clone(), failing.negative_value.clone());
    overrides
}

fn build_param_bindings(
    function: &ast::Function,
    type_index: &TypeIndex,
    overrides: &OverrideMap,
) -> anyhow::Result<Vec<(String, String)>> {
    function
        .params
        .iter()
        .map(|param| {
            let value = default_value_for_type(
                &param.ty,
                type_index,
                &overrides_for_root(overrides, &param.name),
            )?;
            Ok((param.name.clone(), value))
        })
        .collect()
}

fn overrides_for_root(overrides: &OverrideMap, root: &str) -> OverrideMap {
    let mut relative = OverrideMap::new();
    for (path, value) in overrides {
        if path.first().map(|segment| segment.as_str()) == Some(root) {
            relative.insert(path[1..].to_vec(), value.clone());
        }
    }
    relative
}

fn render_positive_test(
    spec: &ParsedSpec,
    bindings: &[(String, String)],
    assert_ensures: bool,
) -> String {
    let mut lines = Vec::new();
    lines.push("#[test]".to_string());
    lines.push(format!(
        "fn test_{}_happy_path() -> () {{",
        sanitize_identifier(&spec.function.name)
    ));
    append_bindings(&mut lines, bindings);
    let call = call_expression(&spec.function, bindings);
    if assert_ensures {
        if let Some(ensures) = &spec.raw_ensures {
            lines.push(format!("    let result = {call};"));
            lines.push(format!("    assert({});", ensures.trim()));
            lines.push("}".to_string());
            return lines.join("\n");
        }
    }
    lines.push(format!("    {call};"));
    lines.push("}".to_string());
    lines.join("\n")
}

fn render_negative_boundary_test(
    spec: &ParsedSpec,
    bindings: &[(String, String)],
    label: &str,
) -> String {
    let mut lines = Vec::new();
    lines.push("#[test]".to_string());
    lines.push("#[should_panic]".to_string());
    lines.push(format!(
        "fn test_{}_requires_{}() -> () {{",
        sanitize_identifier(&spec.function.name),
        sanitize_identifier(label)
    ));
    append_bindings(&mut lines, bindings);
    let call = call_expression(&spec.function, bindings);
    lines.push(format!("    {call};"));
    lines.push("}".to_string());
    lines.join("\n")
}

fn render_negative_ensures_test(
    spec: &ParsedSpec,
    bindings: &[(String, String)],
    ensures: &str,
) -> String {
    let mut lines = Vec::new();
    lines.push("#[test]".to_string());
    lines.push("#[should_panic]".to_string());
    lines.push(format!(
        "fn test_{}_ensures_guard() -> () {{",
        sanitize_identifier(&spec.function.name)
    ));
    append_bindings(&mut lines, bindings);
    let call = call_expression(&spec.function, bindings);
    lines.push(format!("    let result = {call};"));
    lines.push(format!("    assert(!({ensures}));"));
    lines.push("}".to_string());
    lines.join("\n")
}

fn append_bindings(lines: &mut Vec<String>, bindings: &[(String, String)]) {
    for (name, value) in bindings {
        lines.push(format!("    let {name} = {value};"));
    }
}

fn call_expression(function: &ast::Function, bindings: &[(String, String)]) -> String {
    let args = bindings
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}({args})", function.name)
}

fn derive_boundary_cases(expr: &Expr) -> Vec<BoundaryCase> {
    let mut out = Vec::new();
    collect_boundary_cases(expr, &mut out);
    out
}

fn collect_boundary_cases(expr: &Expr, out: &mut Vec<BoundaryCase>) {
    match &expr.kind {
        ExprKind::Binary {
            op: BinOp::And,
            lhs,
            rhs,
        } => {
            collect_boundary_cases(lhs, out);
            collect_boundary_cases(rhs, out);
        }
        _ => {
            if let Some(case) = boundary_case(expr) {
                out.push(case);
            }
        }
    }
}

fn boundary_case(expr: &Expr) -> Option<BoundaryCase> {
    match &expr.kind {
        ExprKind::Var(name) => Some(BoundaryCase {
            path: vec![name.clone()],
            positive_value: "true".to_string(),
            negative_value: "false".to_string(),
            label: format!("{name}_is_true"),
        }),
        ExprKind::Unary {
            op: UnaryOp::Not,
            expr,
        } => access_path(expr).map(|path| BoundaryCase {
            label: format!("{}_is_false", path.join("_")),
            path,
            positive_value: "false".to_string(),
            negative_value: "true".to_string(),
        }),
        ExprKind::Binary { op, lhs, rhs } => {
            if let (Some(path), Some(literal)) = (access_path(lhs), literal_value(rhs)) {
                return boundary_case_from_comparison(path, *op, literal);
            }
            if let (Some(path), Some(literal)) = (access_path(rhs), literal_value(lhs)) {
                return boundary_case_from_comparison(path, flip_operator(*op), literal);
            }
            None
        }
        _ => None,
    }
}

fn boundary_case_from_comparison(
    path: Vec<String>,
    op: BinOp,
    literal: LiteralValue,
) -> Option<BoundaryCase> {
    let label = format!("{}_{}", path.join("_"), operator_name(op));
    match literal {
        LiteralValue::Int(value) => {
            let (positive, negative) = match op {
                BinOp::Gt => (value + 1, value),
                BinOp::Ge => (value, value - 1),
                BinOp::Lt => (value - 1, value),
                BinOp::Le => (value, value + 1),
                BinOp::Eq => (value, value + 1),
                BinOp::Ne => (value + 1, value),
                _ => return None,
            };
            Some(BoundaryCase {
                path,
                positive_value: positive.to_string(),
                negative_value: negative.to_string(),
                label,
            })
        }
        LiteralValue::Float(value) => {
            let epsilon = 1.0;
            let (positive, negative) = match op {
                BinOp::Gt => (value + epsilon, value),
                BinOp::Ge => (value, value - epsilon),
                BinOp::Lt => (value - epsilon, value),
                BinOp::Le => (value, value + epsilon),
                BinOp::Eq => (value, value + epsilon),
                BinOp::Ne => (value + epsilon, value),
                _ => return None,
            };
            Some(BoundaryCase {
                path,
                positive_value: trim_float(positive),
                negative_value: trim_float(negative),
                label,
            })
        }
        LiteralValue::Bool(value) => {
            let (positive, negative) = match op {
                BinOp::Eq => (value, !value),
                BinOp::Ne => (!value, value),
                _ => return None,
            };
            Some(BoundaryCase {
                path,
                positive_value: positive.to_string(),
                negative_value: negative.to_string(),
                label,
            })
        }
        LiteralValue::Char(value) => {
            let positive = match op {
                BinOp::Eq => value,
                BinOp::Ne => next_char(value),
                _ => return None,
            };
            let negative = match op {
                BinOp::Eq => next_char(value),
                BinOp::Ne => value,
                _ => return None,
            };
            Some(BoundaryCase {
                path,
                positive_value: render_char(positive),
                negative_value: render_char(negative),
                label,
            })
        }
        LiteralValue::String(value) => {
            let alternative = if value.is_empty() {
                "sample".to_string()
            } else {
                format!("{value}_alt")
            };
            let (positive, negative) = match op {
                BinOp::Eq => (value, alternative),
                BinOp::Ne => (alternative, value),
                _ => return None,
            };
            Some(BoundaryCase {
                path,
                positive_value: render_string_literal(&positive),
                negative_value: render_string_literal(&negative),
                label,
            })
        }
    }
}

fn default_value_for_type(
    ty: &TypeExpr,
    type_index: &TypeIndex,
    overrides: &OverrideMap,
) -> anyhow::Result<String> {
    if let Some(value) = overrides.get(&Vec::new()) {
        return Ok(value.clone());
    }

    match &ty.kind {
        TypeKind::Unit => Ok("()".to_string()),
        TypeKind::Hole => bail!("cannot synthesize value for type hole"),
        TypeKind::DynTrait { trait_name } => {
            bail!("cannot synthesize value for dynamic trait type `{trait_name}`")
        }
        TypeKind::Named { name, args } => match name.as_str() {
            "Bool" => Ok("false".to_string()),
            "String" => Ok("\"\"".to_string()),
            "Int" | "UInt" | "USize" | "I8" | "I16" | "I32" | "I64" | "I128" | "U8" | "U16"
            | "U32" | "U64" | "U128" | "Float32" | "Float64" => Ok("0".to_string()),
            "Option" => Ok("None".to_string()),
            "Result" => {
                let ok_ty = args
                    .first()
                    .ok_or_else(|| anyhow!("missing `Result` ok type"))?;
                Ok(format!(
                    "Ok({})",
                    default_value_for_type(ok_ty, type_index, &OverrideMap::new())?
                ))
            }
            other => {
                if let Some(strukt) = type_index.struct_shape(other)? {
                    return render_struct_literal(strukt, type_index, overrides);
                }
                if let Some(enm) = type_index.enum_shape(other)? {
                    return render_enum_literal(enm, type_index);
                }
                bail!("cannot synthesize value for unknown type `{other}`")
            }
        },
    }
}

fn render_struct_literal(
    strukt: &StructShape,
    type_index: &TypeIndex,
    overrides: &OverrideMap,
) -> anyhow::Result<String> {
    let mut merged = invariant_positive_overrides(strukt);
    for (path, value) in overrides {
        merged.insert(path.clone(), value.clone());
    }

    let mut fields = Vec::new();
    for field in &strukt.fields {
        let field_key = vec![field.name.clone()];
        let value = if let Some(value) = merged.get(&field_key) {
            value.clone()
        } else {
            let nested = child_overrides(&merged, &field.name);
            default_value_for_type(&field.ty, type_index, &nested)?
        };
        fields.push(format!("{}: {}", field.name, value));
    }

    Ok(format!("{} {{ {} }}", strukt.name, fields.join(", ")))
}

fn render_enum_literal(enm: &EnumShape, type_index: &TypeIndex) -> anyhow::Result<String> {
    let variant = enm
        .variants
        .first()
        .ok_or_else(|| anyhow!("enum `{}` has no variants", enm.name))?;
    match &variant.payload {
        Some(payload) => Ok(format!(
            "{}({})",
            variant.name,
            default_value_for_type(payload, type_index, &OverrideMap::new())?
        )),
        None => Ok(format!("{}()", variant.name)),
    }
}

fn invariant_positive_overrides(strukt: &StructShape) -> OverrideMap {
    let mut overrides = OverrideMap::new();
    if let Some(expr) = &strukt.invariant {
        for boundary in derive_boundary_cases(expr) {
            overrides.insert(boundary.path, boundary.positive_value);
        }
    }
    overrides
}

fn child_overrides(overrides: &OverrideMap, root: &str) -> OverrideMap {
    let mut relative = OverrideMap::new();
    for (path, value) in overrides {
        if path.first().map(|segment| segment.as_str()) == Some(root) {
            relative.insert(path[1..].to_vec(), value.clone());
        }
    }
    relative
}

fn access_path(expr: &Expr) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::Var(name) => Some(vec![name.clone()]),
        ExprKind::FieldAccess { base, field } => {
            let mut path = access_path(base)?;
            path.push(field.clone());
            Some(path)
        }
        _ => None,
    }
}

fn literal_value(expr: &Expr) -> Option<LiteralValue> {
    match &expr.kind {
        ExprKind::Int(value) => Some(LiteralValue::Int(*value)),
        ExprKind::Float(value) => Some(LiteralValue::Float(*value)),
        ExprKind::Bool(value) => Some(LiteralValue::Bool(*value)),
        ExprKind::Char(value) => Some(LiteralValue::Char(*value)),
        ExprKind::String(value) => Some(LiteralValue::String(value.clone())),
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

fn operator_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Eq => "eq",
        BinOp::Ne => "ne",
        BinOp::Lt => "lt",
        BinOp::Le => "le",
        BinOp::Gt => "gt",
        BinOp::Ge => "ge",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::Add => "add",
        BinOp::Sub => "sub",
        BinOp::Mul => "mul",
        BinOp::Div => "div",
        BinOp::Mod => "mod",
        BinOp::BitAnd => "bitand",
        BinOp::BitOr => "bitor",
        BinOp::BitXor => "bitxor",
        BinOp::Shl => "shl",
        BinOp::Shr => "shr",
        BinOp::Ushr => "ushr",
    }
}

fn is_var_named(expr: &Expr, name: &str) -> bool {
    matches!(&expr.kind, ExprKind::Var(value) if value == name)
}

fn materialized_ensures(spec: &ParsedSpec) -> Option<String> {
    let ensures = spec.function.ensures.as_ref()?;
    if can_assert_ensures(ensures) {
        spec.raw_ensures.clone()
    } else {
        None
    }
}

fn can_assert_ensures(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Bool(_) => true,
        ExprKind::Var(name) => name == "result",
        ExprKind::Unary {
            op: UnaryOp::Not,
            expr,
        } => can_assert_ensures(expr),
        ExprKind::Binary { op, lhs, rhs } => match op {
            BinOp::And | BinOp::Or => can_assert_ensures(lhs) && can_assert_ensures(rhs),
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                (is_var_named(lhs, "result") && literal_value(rhs).is_some())
                    || (is_var_named(rhs, "result") && literal_value(lhs).is_some())
            }
            _ => false,
        },
        _ => false,
    }
}

fn expr_mentions_params(expr: &Expr, params: &BTreeSet<String>) -> bool {
    let mut mentioned = false;
    visit_expr(expr, &mut |node| {
        if let ExprKind::Var(name) = &node.kind {
            if params.contains(name) {
                mentioned = true;
            }
        }
    });
    mentioned
}

fn visit_expr(expr: &Expr, visit: &mut dyn FnMut(&Expr)) {
    visit(expr);
    match &expr.kind {
        ExprKind::Call { callee, args, .. } => {
            visit_expr(callee, visit);
            for arg in args {
                visit_expr(arg, visit);
            }
        }
        ExprKind::Closure { body, .. } => visit_block(body, visit),
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            visit_expr(cond, visit);
            visit_block(then_block, visit);
            visit_block(else_block, visit);
        }
        ExprKind::While { cond, body } => {
            visit_expr(cond, visit);
            visit_block(body, visit);
        }
        ExprKind::Loop { body } | ExprKind::UnsafeBlock { block: body } => visit_block(body, visit),
        ExprKind::Break { expr } => {
            if let Some(expr) = expr.as_deref() {
                visit_expr(expr, visit);
            }
        }
        ExprKind::Match { expr, arms } => {
            visit_expr(expr, visit);
            for arm in arms {
                if let Some(guard) = arm.guard.as_ref() {
                    visit_expr(guard, visit);
                }
                visit_expr(&arm.body, visit);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            visit_expr(lhs, visit);
            visit_expr(rhs, visit);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Borrow { expr, .. }
        | ExprKind::Await { expr }
        | ExprKind::Try { expr } => visit_expr(expr, visit),
        ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                visit_expr(value, visit);
            }
        }
        ExprKind::FieldAccess { base, .. } => visit_expr(base, visit),
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Char(_)
        | ExprKind::String(_)
        | ExprKind::Unit
        | ExprKind::Var(_)
        | ExprKind::Continue => {}
    }
}

fn visit_block(block: &ast::Block, visit: &mut dyn FnMut(&Expr)) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let { expr, .. }
            | ast::Stmt::Assign { expr, .. }
            | ast::Stmt::Expr { expr, .. } => {
                visit_expr(expr, visit);
            }
            ast::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    visit_expr(expr, visit);
                }
            }
            ast::Stmt::Assert { expr, .. } => visit_expr(expr, visit),
        }
    }
    if let Some(tail) = block.tail.as_deref() {
        visit_expr(tail, visit);
    }
}

fn render_expr(expr: &Expr) -> anyhow::Result<String> {
    match &expr.kind {
        ExprKind::Int(value) => Ok(value.to_string()),
        ExprKind::Float(value) => Ok(trim_float(*value)),
        ExprKind::Bool(value) => Ok(value.to_string()),
        ExprKind::Char(value) => Ok(render_char(*value)),
        ExprKind::String(value) => Ok(render_string_literal(value)),
        ExprKind::Unit => Ok("()".to_string()),
        ExprKind::Var(name) => Ok(name.clone()),
        ExprKind::Call { callee, args, .. } => {
            let callee = render_expr(callee)?;
            let args = args
                .iter()
                .map(render_expr)
                .collect::<anyhow::Result<Vec<_>>>()?
                .join(", ");
            Ok(format!("{callee}({args})"))
        }
        ExprKind::Binary { op, lhs, rhs } => Ok(format!(
            "{} {} {}",
            render_expr(lhs)?,
            render_bin_op(*op),
            render_expr(rhs)?
        )),
        ExprKind::Unary { op, expr } => {
            Ok(format!("{}{}", render_unary_op(*op), render_expr(expr)?))
        }
        ExprKind::FieldAccess { base, field } => Ok(format!("{}.{}", render_expr(base)?, field)),
        ExprKind::StructInit { name, fields } => {
            let fields = fields
                .iter()
                .map(|(field, value, _)| Ok(format!("{field}: {}", render_expr(value)?)))
                .collect::<anyhow::Result<Vec<_>>>()?
                .join(", ");
            Ok(format!("{name} {{ {fields} }}"))
        }
        _ => bail!("unsupported expression form for synthesized body"),
    }
}

fn render_type_expr(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeKind::Unit => "()".to_string(),
        TypeKind::Hole => "_".to_string(),
        TypeKind::DynTrait { trait_name } => format!("dyn {trait_name}"),
        TypeKind::Named { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{name}[{}]",
                    args.iter()
                        .map(render_type_expr)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

fn render_bin_op(op: BinOp) -> &'static str {
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

fn render_unary_op(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
        UnaryOp::BitNot => "~",
    }
}

fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "Bool"
            | "String"
            | "Int"
            | "UInt"
            | "USize"
            | "I8"
            | "I16"
            | "I32"
            | "I64"
            | "I128"
            | "U8"
            | "U16"
            | "U32"
            | "U64"
            | "U128"
            | "Float32"
            | "Float64"
            | "Option"
            | "Result"
            | "Unit"
    )
}

fn render_string_literal(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn render_char(value: char) -> String {
    let escaped = match value {
        '\'' => "\\'".to_string(),
        '\\' => "\\\\".to_string(),
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        other => other.to_string(),
    };
    format!("'{escaped}'")
}

fn trim_float(value: f64) -> String {
    let mut rendered = format!("{value}");
    if !rendered.contains('.') {
        rendered.push_str(".0");
    }
    rendered
}

fn next_char(value: char) -> char {
    char::from_u32(value as u32 + 1).unwrap_or('z')
}

fn sanitize_identifier(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "synth".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::attr_test_runner::run_attribute_tests;

    use super::{format_text, synthesize_from_spec};

    fn write_spec_project(root: &std::path::Path, spec_body: &str) {
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::create_dir_all(root.join("specs")).expect("mkdir specs");
        fs::write(
            root.join("aic.toml"),
            "[package]\nname = \"spec_first\"\nmain = \"src/main.aic\"\n",
        )
        .expect("write aic.toml");
        fs::write(
            root.join("src/main.aic"),
            concat!(
                "module demo.spec_first;\n",
                "struct User {\n",
                "    age: Int,\n",
                "    name: String,\n",
                "} invariant age >= 0\n",
                "\n",
                "enum ValidationError {\n",
                "    Internal,\n",
                "    EmptyName,\n",
                "}\n",
                "\n",
                "fn main() -> Int {\n",
                "    0\n",
                "}\n",
            ),
        )
        .expect("write main.aic");
        fs::write(root.join("specs/validate_user.aic"), spec_body).expect("write spec");
    }

    #[test]
    fn synthesize_generates_runnable_attribute_fixture() {
        let project = tempdir().expect("tempdir");
        write_spec_project(
            project.path(),
            concat!(
                "spec fn validate_user(user: User) -> Result[Bool, ValidationError] {\n",
                "    requires user.age >= 0\n",
                "    ensures result == Ok(false)\n",
                "    effects { io }\n",
                "}\n",
            ),
        );

        let response =
            synthesize_from_spec(project.path(), "validate_user").expect("synthesize response");
        assert_eq!(response.phase, "synthesize");
        assert_eq!(response.source_kind, "spec");
        assert_eq!(response.artifacts.len(), 2);
        assert!(response
            .notes
            .iter()
            .any(|note| note.contains("capability")));

        let function = response
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == "function")
            .expect("function artifact");
        assert!(function.content.contains("capabilities { io }"));
        assert!(function.content.contains("Ok(false)"));

        let fixture = response
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == "attribute-test-fixture")
            .expect("fixture artifact");
        assert!(fixture.content.contains("#[should_panic]"));

        fs::write(project.path().join("generated_tests.aic"), &fixture.content)
            .expect("write generated fixture");
        let report =
            run_attribute_tests(project.path(), Some("validate_user"), 0).expect("run tests");
        assert_eq!(report.total, 2);
        assert_eq!(report.failed, 0, "{report:#?}");
    }

    #[test]
    fn synthesize_uses_result_only_ensures_when_requires_are_absent() {
        let project = tempdir().expect("tempdir");
        write_spec_project(
            project.path(),
            concat!(
                "spec fn validate_user(user: User) -> Bool {\n",
                "    ensures result == true\n",
                "}\n",
            ),
        );

        let response =
            synthesize_from_spec(project.path(), "validate_user").expect("synthesize response");
        let function = response
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == "function")
            .expect("function artifact");
        assert!(function.content.contains("\n    true\n}"));
        assert!(response
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == "attribute-test-fixture")
            .expect("fixture artifact")
            .content
            .contains("assert(!(result == true));"));
        assert!(format_text(&response).contains("synthesize: spec validate_user"));
    }
}
