use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};

use crate::ast::{self, Expr, ExprKind, Pattern, PatternKind, Stmt};
use crate::parser;
use crate::span::Span;
use crate::symbol_query::{self, SymbolKind};

const PATCH_PROTOCOL_VERSION: &str = "1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchMode {
    Preview,
    Apply,
}

impl PatchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PatchMode::Preview => "preview",
            PatchMode::Apply => "apply",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchDocument {
    pub operations: Vec<PatchOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PatchOperation {
    AddFunction {
        #[serde(default)]
        target_file: Option<String>,
        #[serde(default)]
        after_symbol: Option<String>,
        function: PatchFunctionSpec,
    },
    ModifyMatchArm {
        #[serde(default)]
        target_file: Option<String>,
        target_function: String,
        match_index: usize,
        arm_pattern: String,
        new_body: String,
    },
    AddField {
        #[serde(default)]
        target_file: Option<String>,
        target_struct: String,
        field: PatchFieldSpec,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchFunctionSpec {
    pub name: String,
    #[serde(default)]
    pub params: Vec<PatchParamSpec>,
    pub return_type: String,
    pub body: String,
    #[serde(default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub requires: Option<String>,
    #[serde(default)]
    pub ensures: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchParamSpec {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchFieldSpec {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchEdit {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
    pub message: String,
    pub operation_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchPreview {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub before: String,
    pub after: String,
    pub message: String,
    pub operation_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchConflict {
    pub operation_index: usize,
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchResponse {
    pub protocol_version: String,
    pub phase: String,
    pub mode: String,
    pub ok: bool,
    pub files_changed: Vec<String>,
    pub applied_edits: Vec<PatchEdit>,
    pub conflicts: Vec<PatchConflict>,
    pub previews: Vec<PatchPreview>,
}

#[derive(Debug, Clone)]
struct FileState {
    original: String,
    current: String,
}

#[derive(Debug, Clone)]
struct OperationApply {
    edits: Vec<PatchEdit>,
    previews: Vec<PatchPreview>,
}

#[derive(Debug, Clone)]
struct MatchExprCandidate {
    arms: Vec<MatchArmCandidate>,
}

#[derive(Debug, Clone)]
struct MatchArmCandidate {
    pattern: String,
    body_span: Span,
}

pub fn read_patch_document(path: &Path) -> anyhow::Result<PatchDocument> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read patch document {}", path.display()))?;
    let parsed = serde_json::from_str::<PatchDocument>(&raw)
        .with_context(|| format!("failed to parse patch document {}", path.display()))?;

    if parsed.operations.is_empty() {
        anyhow::bail!("patch document must contain at least one operation");
    }

    Ok(parsed)
}

pub fn run_patch(
    project_root: &Path,
    patch_path: &Path,
    mode: PatchMode,
) -> anyhow::Result<PatchResponse> {
    let document = read_patch_document(patch_path)?;
    apply_patch_document(project_root, &document, mode)
}

pub fn apply_patch_document(
    project_root: &Path,
    document: &PatchDocument,
    mode: PatchMode,
) -> anyhow::Result<PatchResponse> {
    let mut file_states = BTreeMap::<PathBuf, FileState>::new();
    let mut applied_edits = Vec::<PatchEdit>::new();
    let mut previews = Vec::<PatchPreview>::new();
    let mut conflicts = Vec::<PatchConflict>::new();

    for (operation_index, operation) in document.operations.iter().enumerate() {
        match apply_operation(project_root, operation, operation_index, &mut file_states) {
            Ok(applied) => {
                applied_edits.extend(applied.edits);
                previews.extend(applied.previews);
            }
            Err(conflict) => {
                conflicts.push(conflict);
            }
        }
    }

    let mut changed_paths = file_states
        .iter()
        .filter_map(|(path, state)| {
            if state.current != state.original {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    changed_paths.sort();

    if matches!(mode, PatchMode::Apply) && conflicts.is_empty() {
        for path in &changed_paths {
            let state = file_states
                .get(path)
                .ok_or_else(|| anyhow!("missing in-memory state for {}", path.display()))?;
            fs::write(path, &state.current)
                .with_context(|| format!("failed to write patched file {}", path.display()))?;
        }
    }

    let mut files_changed = changed_paths
        .iter()
        .map(|path| display_path(path))
        .collect::<Vec<_>>();
    files_changed.sort();
    files_changed.dedup();

    Ok(PatchResponse {
        protocol_version: PATCH_PROTOCOL_VERSION.to_string(),
        phase: "patch".to_string(),
        mode: mode.as_str().to_string(),
        ok: conflicts.is_empty(),
        files_changed,
        applied_edits,
        conflicts,
        previews,
    })
}

fn apply_operation(
    project_root: &Path,
    operation: &PatchOperation,
    operation_index: usize,
    file_states: &mut BTreeMap<PathBuf, FileState>,
) -> Result<OperationApply, PatchConflict> {
    let target_path = match resolve_target_file(project_root, operation) {
        Ok(path) => path,
        Err(message) => {
            return Err(PatchConflict {
                operation_index,
                kind: "resolve_target".to_string(),
                message,
                file: None,
            });
        }
    };

    let state = match ensure_file_state(file_states, &target_path) {
        Ok(state) => state,
        Err(err) => {
            return Err(PatchConflict {
                operation_index,
                kind: "read_file".to_string(),
                message: err.to_string(),
                file: Some(display_path(&target_path)),
            });
        }
    };

    let parse_label = display_path(&target_path);
    let (program, diagnostics) = parser::parse(&state.current, &parse_label);
    if diagnostics.iter().any(|diag| diag.is_error()) {
        return Err(PatchConflict {
            operation_index,
            kind: "parse_before".to_string(),
            message: summarize_parse_errors(&diagnostics),
            file: Some(parse_label),
        });
    }
    let Some(program) = program else {
        return Err(PatchConflict {
            operation_index,
            kind: "parse_before".to_string(),
            message: "failed to parse target file into AST".to_string(),
            file: Some(parse_label),
        });
    };

    let edits = match plan_operation(
        operation,
        operation_index,
        &target_path,
        &state.current,
        &program,
    ) {
        Ok(edits) => edits,
        Err(message) => {
            return Err(PatchConflict {
                operation_index,
                kind: "plan".to_string(),
                message,
                file: Some(display_path(&target_path)),
            });
        }
    };

    let previews = edits
        .iter()
        .map(|edit| PatchPreview {
            file: edit.file.clone(),
            start: edit.start,
            end: edit.end,
            before: state
                .current
                .get(edit.start..edit.end)
                .unwrap_or_default()
                .to_string(),
            after: edit.replacement.clone(),
            message: edit.message.clone(),
            operation_index,
        })
        .collect::<Vec<_>>();

    let updated = match apply_text_edits(&state.current, &edits) {
        Ok(updated) => updated,
        Err(err) => {
            return Err(PatchConflict {
                operation_index,
                kind: "apply".to_string(),
                message: err.to_string(),
                file: Some(display_path(&target_path)),
            });
        }
    };

    if let Err(message) = validate_source_parses(&target_path, &updated) {
        return Err(PatchConflict {
            operation_index,
            kind: "validate".to_string(),
            message,
            file: Some(display_path(&target_path)),
        });
    }

    state.current = updated;

    Ok(OperationApply { edits, previews })
}

fn ensure_file_state<'a>(
    file_states: &'a mut BTreeMap<PathBuf, FileState>,
    path: &Path,
) -> anyhow::Result<&'a mut FileState> {
    if !file_states.contains_key(path) {
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read source file {}", path.display()))?;
        file_states.insert(
            path.to_path_buf(),
            FileState {
                original: source.clone(),
                current: source,
            },
        );
    }

    file_states
        .get_mut(path)
        .ok_or_else(|| anyhow!("failed to initialize source state for {}", path.display()))
}

fn resolve_target_file(project_root: &Path, operation: &PatchOperation) -> Result<PathBuf, String> {
    match operation {
        PatchOperation::AddFunction {
            target_file,
            after_symbol,
            ..
        } => {
            if let Some(file) = target_file {
                return resolve_explicit_file(project_root, file);
            }
            if let Some(symbol) = after_symbol {
                return resolve_symbol_file(project_root, SymbolKind::Function, symbol);
            }
            let fallback = project_root.join("src/main.aic");
            if fallback.exists() {
                return Ok(fallback);
            }
            Err("add_function requires `target_file` when `src/main.aic` is missing".to_string())
        }
        PatchOperation::ModifyMatchArm {
            target_file,
            target_function,
            ..
        } => {
            if let Some(file) = target_file {
                resolve_explicit_file(project_root, file)
            } else {
                resolve_symbol_file(project_root, SymbolKind::Function, target_function)
            }
        }
        PatchOperation::AddField {
            target_file,
            target_struct,
            ..
        } => {
            if let Some(file) = target_file {
                resolve_explicit_file(project_root, file)
            } else {
                resolve_symbol_file(project_root, SymbolKind::Struct, target_struct)
            }
        }
    }
}

fn resolve_explicit_file(project_root: &Path, raw: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(raw);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        project_root.join(candidate)
    };

    if path.exists() {
        Ok(path)
    } else {
        Err(format!("target file does not exist: {}", path.display()))
    }
}

fn resolve_symbol_file(
    project_root: &Path,
    kind: SymbolKind,
    symbol_name: &str,
) -> Result<PathBuf, String> {
    let symbols = symbol_query::list_symbols(project_root)
        .map_err(|err| format!("failed to index symbols: {err}"))?;

    let mut files = symbols
        .iter()
        .filter(|symbol| symbol.kind == kind && symbol.name == symbol_name)
        .map(|symbol| PathBuf::from(&symbol.location.file))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    files.sort();

    match files.len() {
        0 => Err(format!(
            "unable to resolve {} `{}` in project symbol index",
            kind.as_str(),
            symbol_name
        )),
        1 => Ok(files.remove(0)),
        _ => Err(format!(
            "ambiguous {} `{}` across files: {}",
            kind.as_str(),
            symbol_name,
            files
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn plan_operation(
    operation: &PatchOperation,
    operation_index: usize,
    target_path: &Path,
    source: &str,
    program: &ast::Program,
) -> Result<Vec<PatchEdit>, String> {
    let file = display_path(target_path);

    match operation {
        PatchOperation::AddFunction {
            after_symbol,
            function,
            ..
        } => plan_add_function(
            source,
            &file,
            program,
            operation_index,
            after_symbol.as_deref(),
            function,
        ),
        PatchOperation::ModifyMatchArm {
            target_function,
            match_index,
            arm_pattern,
            new_body,
            ..
        } => plan_modify_match_arm(
            source,
            &file,
            program,
            operation_index,
            target_function,
            *match_index,
            arm_pattern,
            new_body,
        ),
        PatchOperation::AddField {
            target_struct,
            field,
            ..
        } => plan_add_field(
            source,
            &file,
            program,
            operation_index,
            target_struct,
            field,
        ),
    }
}

fn plan_add_function(
    source: &str,
    file: &str,
    program: &ast::Program,
    operation_index: usize,
    after_symbol: Option<&str>,
    function: &PatchFunctionSpec,
) -> Result<Vec<PatchEdit>, String> {
    if function.name.trim().is_empty() {
        return Err("add_function requires non-empty `function.name`".to_string());
    }
    if function.return_type.trim().is_empty() {
        return Err("add_function requires non-empty `function.return_type`".to_string());
    }
    if function.body.trim().is_empty() {
        return Err("add_function requires non-empty `function.body`".to_string());
    }

    let duplicate = program.items.iter().any(|item| {
        if let ast::Item::Function(existing) = item {
            existing.name == function.name
        } else {
            false
        }
    });
    if duplicate {
        return Err(format!(
            "function `{}` already exists in {}",
            function.name, file
        ));
    }

    let insertion_offset = match after_symbol {
        Some(symbol) => {
            let item = program
                .items
                .iter()
                .find(|item| item.name() == symbol)
                .ok_or_else(|| format!("after_symbol `{symbol}` was not found in {}", file))?;
            item_end_offset(item)
        }
        None => source.len(),
    };

    if insertion_offset > source.len() {
        return Err(format!(
            "computed insertion offset {} exceeds source length {}",
            insertion_offset,
            source.len()
        ));
    }

    let function_text = render_function_spec(function);
    let replacement = if insertion_offset >= source.len() {
        format!("\n\n{function_text}\n")
    } else {
        format!("\n\n{function_text}")
    };

    Ok(vec![PatchEdit {
        file: file.to_string(),
        start: insertion_offset,
        end: insertion_offset,
        replacement,
        message: format!("add function `{}`", function.name),
        operation_index,
    }])
}

fn item_end_offset(item: &ast::Item) -> usize {
    match item {
        ast::Item::Struct(strukt) => strukt
            .invariant
            .as_ref()
            .map(|expr| expr.span.end)
            .unwrap_or(strukt.span.end),
        _ => item.span().end,
    }
}

fn render_function_spec(function: &PatchFunctionSpec) -> String {
    let params = function
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name.trim(), param.ty.trim()))
        .collect::<Vec<_>>()
        .join(", ");

    let mut signature = format!(
        "fn {}({}) -> {}",
        function.name.trim(),
        params,
        function.return_type.trim()
    );

    if !function.effects.is_empty() {
        signature.push_str(" effects { ");
        signature.push_str(
            &function
                .effects
                .iter()
                .map(|effect| effect.trim())
                .filter(|effect| !effect.is_empty())
                .collect::<Vec<_>>()
                .join(", "),
        );
        signature.push_str(" }");
    }

    if !function.capabilities.is_empty() {
        signature.push_str(" capabilities { ");
        signature.push_str(
            &function
                .capabilities
                .iter()
                .map(|cap| cap.trim())
                .filter(|cap| !cap.is_empty())
                .collect::<Vec<_>>()
                .join(", "),
        );
        signature.push_str(" }");
    }

    if let Some(requires) = &function.requires {
        let requires = requires.trim();
        if !requires.is_empty() {
            signature.push_str(" requires ");
            signature.push_str(requires);
        }
    }

    if let Some(ensures) = &function.ensures {
        let ensures = ensures.trim();
        if !ensures.is_empty() {
            signature.push_str(" ensures ");
            signature.push_str(ensures);
        }
    }

    format!(
        "{} {{\n{}\n}}",
        signature,
        indent_block_body(&function.body)
    )
}

fn indent_block_body(body: &str) -> String {
    body.lines()
        .map(|line| {
            if line.trim().is_empty() {
                "    ".to_string()
            } else {
                format!("    {}", line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn plan_add_field(
    source: &str,
    file: &str,
    program: &ast::Program,
    operation_index: usize,
    target_struct: &str,
    field: &PatchFieldSpec,
) -> Result<Vec<PatchEdit>, String> {
    if field.name.trim().is_empty() || field.ty.trim().is_empty() {
        return Err("add_field requires `field.name` and `field.ty`".to_string());
    }

    let mut structs = program
        .items
        .iter()
        .filter_map(|item| {
            if let ast::Item::Struct(strukt) = item {
                if strukt.name == target_struct {
                    Some(strukt)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if structs.is_empty() {
        return Err(format!(
            "struct `{}` was not found in {}",
            target_struct, file
        ));
    }
    if structs.len() > 1 {
        return Err(format!(
            "multiple struct declarations named `{}` found in {}",
            target_struct, file
        ));
    }

    let strukt = structs.remove(0);
    if strukt.fields.iter().any(|entry| entry.name == field.name) {
        return Err(format!(
            "field `{}` already exists on struct `{}`",
            field.name, target_struct
        ));
    }

    let close_offset = find_struct_closing_brace_offset(source, strukt.span).ok_or_else(|| {
        format!(
            "failed to locate closing brace for struct `{}`",
            target_struct
        )
    })?;

    let mut edits = Vec::new();
    if let Some(last_field) = strukt.fields.last() {
        let after_last = last_field.span.end.min(source.len());
        let between = source.get(after_last..close_offset).unwrap_or_default();
        if !between.contains(',') {
            edits.push(PatchEdit {
                file: file.to_string(),
                start: after_last,
                end: after_last,
                replacement: ",".to_string(),
                message: format!(
                    "normalize trailing comma before adding `{}` to `{}`",
                    field.name, target_struct
                ),
                operation_index,
            });
        }

        edits.push(PatchEdit {
            file: file.to_string(),
            start: close_offset,
            end: close_offset,
            replacement: format!("\n    {}: {},", field.name.trim(), field.ty.trim()),
            message: format!("add field `{}` to struct `{}`", field.name, target_struct),
            operation_index,
        });
    } else {
        edits.push(PatchEdit {
            file: file.to_string(),
            start: close_offset,
            end: close_offset,
            replacement: format!("\n    {}: {},\n", field.name.trim(), field.ty.trim()),
            message: format!(
                "add first field `{}` to struct `{}`",
                field.name, target_struct
            ),
            operation_index,
        });
    }

    Ok(edits)
}

fn find_struct_closing_brace_offset(source: &str, span: Span) -> Option<usize> {
    if source.is_empty() {
        return None;
    }

    let start = span.start.min(source.len());
    let end = span.end.min(source.len());
    if end == 0 || start >= end {
        return None;
    }

    let bytes = source.as_bytes();
    if bytes[end - 1] == b'}' {
        return Some(end - 1);
    }

    for idx in (start..end).rev() {
        if bytes[idx] == b'}' {
            return Some(idx);
        }
    }

    None
}

fn plan_modify_match_arm(
    source: &str,
    file: &str,
    program: &ast::Program,
    operation_index: usize,
    target_function: &str,
    match_index: usize,
    arm_pattern: &str,
    new_body: &str,
) -> Result<Vec<PatchEdit>, String> {
    if target_function.trim().is_empty() {
        return Err("modify_match_arm requires non-empty `target_function`".to_string());
    }
    if arm_pattern.trim().is_empty() {
        return Err("modify_match_arm requires non-empty `arm_pattern`".to_string());
    }
    if new_body.trim().is_empty() {
        return Err("modify_match_arm requires non-empty `new_body`".to_string());
    }

    let mut candidates = find_functions_by_name(program, target_function);
    if candidates.is_empty() {
        return Err(format!(
            "function `{}` was not found in {}",
            target_function, file
        ));
    }
    if candidates.len() > 1 {
        return Err(format!(
            "function `{}` resolved to multiple declarations in {}; disambiguate with `target_file`",
            target_function, file
        ));
    }
    let function = candidates.remove(0);

    let mut matches = Vec::new();
    collect_match_expr_candidates_from_block(&function.body, &mut matches);

    if match_index >= matches.len() {
        return Err(format!(
            "match_index {} is out of range for function `{}` ({} match expression(s))",
            match_index,
            target_function,
            matches.len()
        ));
    }

    let target_match = &matches[match_index];
    let normalized_pattern = normalize_ws(arm_pattern);

    let selected_arm = target_match
        .arms
        .iter()
        .find(|arm| normalize_ws(&arm.pattern) == normalized_pattern)
        .ok_or_else(|| {
            let available = target_match
                .arms
                .iter()
                .map(|arm| arm.pattern.clone())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "pattern `{}` was not found in match {} of `{}`; available arms: {}",
                arm_pattern, match_index, target_function, available
            )
        })?;

    let body_start = selected_arm.body_span.start.min(source.len());
    let body_end = selected_arm.body_span.end.min(source.len());
    if body_end < body_start {
        return Err(format!(
            "invalid body span {}..{} for arm `{}`",
            body_start, body_end, arm_pattern
        ));
    }

    Ok(vec![PatchEdit {
        file: file.to_string(),
        start: body_start,
        end: body_end,
        replacement: new_body.trim().to_string(),
        message: format!(
            "modify match arm `{}` in function `{}` (match #{})",
            arm_pattern, target_function, match_index
        ),
        operation_index,
    }])
}

fn find_functions_by_name<'a>(program: &'a ast::Program, target: &str) -> Vec<&'a ast::Function> {
    let qualified_lookup = target.contains("::");
    let mut out = Vec::new();

    for item in &program.items {
        match item {
            ast::Item::Function(function) => {
                if !qualified_lookup && function.name == target {
                    out.push(function);
                }
            }
            ast::Item::Trait(trait_def) => {
                for method in &trait_def.methods {
                    let qualified = format!("{}::{}", trait_def.name, method.name);
                    if qualified == target || (!qualified_lookup && method.name == target) {
                        out.push(method);
                    }
                }
            }
            ast::Item::Impl(impl_def) => {
                for method in &impl_def.methods {
                    let qualified = format!("{}::{}", impl_def.trait_name, method.name);
                    if qualified == target || (!qualified_lookup && method.name == target) {
                        out.push(method);
                    }
                }
            }
            ast::Item::Struct(_) | ast::Item::Enum(_) => {}
        }
    }

    out
}

fn collect_match_expr_candidates_from_block(block: &ast::Block, out: &mut Vec<MatchExprCandidate>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { expr, .. }
            | Stmt::Assign { expr, .. }
            | Stmt::Expr { expr, .. }
            | Stmt::Assert { expr, .. } => collect_match_expr_candidates_from_expr(expr, out),
            Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    collect_match_expr_candidates_from_expr(expr, out);
                }
            }
        }
    }

    if let Some(tail) = &block.tail {
        collect_match_expr_candidates_from_expr(tail, out);
    }
}

fn collect_match_expr_candidates_from_expr(expr: &Expr, out: &mut Vec<MatchExprCandidate>) {
    match &expr.kind {
        ExprKind::Match { expr: inner, arms } => {
            let candidates = arms
                .iter()
                .map(|arm| MatchArmCandidate {
                    pattern: render_pattern(&arm.pattern),
                    body_span: arm.body.span,
                })
                .collect::<Vec<_>>();
            out.push(MatchExprCandidate { arms: candidates });

            collect_match_expr_candidates_from_expr(inner, out);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_match_expr_candidates_from_expr(guard, out);
                }
                collect_match_expr_candidates_from_expr(&arm.body, out);
            }
        }
        ExprKind::Call { callee, args, .. } => {
            collect_match_expr_candidates_from_expr(callee, out);
            for arg in args {
                collect_match_expr_candidates_from_expr(arg, out);
            }
        }
        ExprKind::Closure { body, .. } => collect_match_expr_candidates_from_block(body, out),
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_match_expr_candidates_from_expr(cond, out);
            collect_match_expr_candidates_from_block(then_block, out);
            collect_match_expr_candidates_from_block(else_block, out);
        }
        ExprKind::While { cond, body } => {
            collect_match_expr_candidates_from_expr(cond, out);
            collect_match_expr_candidates_from_block(body, out);
        }
        ExprKind::Loop { body } => collect_match_expr_candidates_from_block(body, out),
        ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                collect_match_expr_candidates_from_expr(expr, out);
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_match_expr_candidates_from_expr(lhs, out);
            collect_match_expr_candidates_from_expr(rhs, out);
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Borrow { expr, .. }
        | ExprKind::Await { expr }
        | ExprKind::Try { expr } => collect_match_expr_candidates_from_expr(expr, out),
        ExprKind::UnsafeBlock { block } => collect_match_expr_candidates_from_block(block, out),
        ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                collect_match_expr_candidates_from_expr(value, out);
            }
        }
        ExprKind::FieldAccess { base, .. } => collect_match_expr_candidates_from_expr(base, out),
        ExprKind::Continue
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Char(_)
        | ExprKind::String(_)
        | ExprKind::Unit
        | ExprKind::Var(_) => {}
    }
}

fn render_pattern(pattern: &Pattern) -> String {
    match &pattern.kind {
        PatternKind::Wildcard => "_".to_string(),
        PatternKind::Var(name) => name.clone(),
        PatternKind::Int(value) => value.to_string(),
        PatternKind::Bool(value) => value.to_string(),
        PatternKind::Char(value) => format!("{:?}", value),
        PatternKind::String(value) => format!("\"{}\"", value),
        PatternKind::Unit => "()".to_string(),
        PatternKind::Or { patterns } => patterns
            .iter()
            .map(render_pattern)
            .collect::<Vec<_>>()
            .join(" | "),
        PatternKind::Variant { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}({})",
                    name,
                    args.iter()
                        .map(render_pattern)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        PatternKind::Struct {
            name,
            fields,
            has_rest,
        } => {
            let mut rendered = fields
                .iter()
                .map(|field| format!("{}: {}", field.name, render_pattern(&field.pattern)))
                .collect::<Vec<_>>();
            if *has_rest {
                rendered.push("..".to_string());
            }
            format!("{} {{ {} }}", name, rendered.join(", "))
        }
    }
}

fn normalize_ws(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn apply_text_edits(source: &str, edits: &[PatchEdit]) -> anyhow::Result<String> {
    let mut ordered = edits.to_vec();
    ordered.sort_by(|a, b| {
        b.start
            .cmp(&a.start)
            .then(b.end.cmp(&a.end))
            .then(b.operation_index.cmp(&a.operation_index))
    });

    let mut output = source.to_string();
    for edit in ordered {
        if edit.end < edit.start {
            anyhow::bail!(
                "invalid edit range {}..{} in {}",
                edit.start,
                edit.end,
                edit.file
            );
        }
        if edit.end > output.len() {
            anyhow::bail!(
                "edit {}..{} out of bounds for {} (len={})",
                edit.start,
                edit.end,
                edit.file,
                output.len()
            );
        }
        if !output.is_char_boundary(edit.start) || !output.is_char_boundary(edit.end) {
            anyhow::bail!(
                "edit {}..{} is not UTF-8 boundary in {}",
                edit.start,
                edit.end,
                edit.file
            );
        }
        output.replace_range(edit.start..edit.end, &edit.replacement);
    }

    Ok(output)
}

fn validate_source_parses(file: &Path, source: &str) -> Result<(), String> {
    let file_label = display_path(file);
    let (program, diagnostics) = parser::parse(source, &file_label);
    if diagnostics.iter().any(|diag| diag.is_error()) {
        return Err(summarize_parse_errors(&diagnostics));
    }
    if program.is_none() {
        return Err("parser returned no AST".to_string());
    }
    Ok(())
}

fn summarize_parse_errors(diagnostics: &[crate::diagnostics::Diagnostic]) -> String {
    diagnostics
        .iter()
        .filter(|diag| diag.is_error())
        .take(3)
        .map(|diag| format!("{}: {}", diag.code, diag.message))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub fn format_patch_response_text(response: &PatchResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "patch {}: ok={} files_changed={} edits={} conflicts={}",
        response.mode,
        response.ok,
        response.files_changed.len(),
        response.applied_edits.len(),
        response.conflicts.len()
    ));

    for file in &response.files_changed {
        lines.push(format!("  changed {file}"));
    }

    for preview in &response.previews {
        lines.push(format!(
            "  edit#{} {}:{}..{} {}",
            preview.operation_index, preview.file, preview.start, preview.end, preview.message
        ));
        if !preview.before.is_empty() {
            lines.push(format!("    - {}", preview.before.replace('\n', "\\n")));
        }
        lines.push(format!("    + {}", preview.after.replace('\n', "\\n")));
    }

    for conflict in &response.conflicts {
        let file = conflict
            .file
            .as_ref()
            .map(|value| format!(" ({value})"))
            .unwrap_or_default();
        lines.push(format!(
            "  conflict#{} [{}] {}{}",
            conflict.operation_index, conflict.kind, conflict.message, file
        ));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        apply_patch_document, format_patch_response_text, PatchDocument, PatchMode, PatchOperation,
        PatchParamSpec,
    };

    #[test]
    fn patch_document_preview_and_apply_modify_source() {
        let dir = tempdir().expect("tempdir");
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("mkdir src");
        let source_path = src_dir.join("main.aic");

        fs::write(
            &source_path,
            concat!(
                "module demo.patch;\n",
                "struct Config {\n",
                "    port: Int\n",
                "}\n",
                "fn handle_result(x: Result[Int, Int]) -> Int {\n",
                "    match x {\n",
                "        Ok(v) => v,\n",
                "        Err(e) => e,\n",
                "    }\n",
                "}\n",
                "fn main() -> Int {\n",
                "    handle_result(Ok(1))\n",
                "}\n",
            ),
        )
        .expect("write source");

        let document = PatchDocument {
            operations: vec![
                PatchOperation::AddField {
                    target_file: Some("src/main.aic".to_string()),
                    target_struct: "Config".to_string(),
                    field: super::PatchFieldSpec {
                        name: "timeout".to_string(),
                        ty: "Int".to_string(),
                    },
                },
                PatchOperation::ModifyMatchArm {
                    target_file: Some("src/main.aic".to_string()),
                    target_function: "handle_result".to_string(),
                    match_index: 0,
                    arm_pattern: "Err(e)".to_string(),
                    new_body: "0 - e".to_string(),
                },
                PatchOperation::AddFunction {
                    target_file: Some("src/main.aic".to_string()),
                    after_symbol: Some("handle_result".to_string()),
                    function: super::PatchFunctionSpec {
                        name: "validate_port".to_string(),
                        params: vec![PatchParamSpec {
                            name: "c".to_string(),
                            ty: "Config".to_string(),
                        }],
                        return_type: "Bool".to_string(),
                        body: "c.port >= 0".to_string(),
                        effects: Vec::new(),
                        capabilities: Vec::new(),
                        requires: None,
                        ensures: None,
                    },
                },
            ],
        };

        let preview = apply_patch_document(dir.path(), &document, PatchMode::Preview)
            .expect("patch preview succeeds");
        assert!(preview.ok, "conflicts: {:#?}", preview.conflicts);
        assert_eq!(preview.mode, "preview");
        assert_eq!(preview.conflicts.len(), 0);
        assert!(!preview.files_changed.is_empty());
        assert!(
            format_patch_response_text(&preview).contains("patch preview"),
            "text formatter should summarize preview"
        );

        let after_preview = fs::read_to_string(&source_path).expect("read after preview");
        assert!(after_preview.contains("Err(e) => e"));

        let apply =
            apply_patch_document(dir.path(), &document, PatchMode::Apply).expect("patch apply");
        assert!(apply.ok, "conflicts: {:#?}", apply.conflicts);

        let rewritten = fs::read_to_string(&source_path).expect("read rewritten source");
        assert!(rewritten.contains("timeout: Int"));
        assert!(rewritten.contains("Err(e) => 0 - e"));
        assert!(rewritten.contains("fn validate_port(c: Config) -> Bool"));
    }

    #[test]
    fn patch_conflict_is_reported_for_missing_struct() {
        let dir = tempdir().expect("tempdir");
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("mkdir src");
        let source_path = src_dir.join("main.aic");

        fs::write(
            &source_path,
            "module demo.patch;\nfn main() -> Int {\n    0\n}\n",
        )
        .expect("write source");

        let document = PatchDocument {
            operations: vec![PatchOperation::AddField {
                target_file: Some("src/main.aic".to_string()),
                target_struct: "Missing".to_string(),
                field: super::PatchFieldSpec {
                    name: "timeout".to_string(),
                    ty: "Int".to_string(),
                },
            }],
        };

        let response =
            apply_patch_document(dir.path(), &document, PatchMode::Apply).expect("patch response");
        assert!(!response.ok);
        assert_eq!(response.conflicts.len(), 1);

        let after = fs::read_to_string(&source_path).expect("read source after failed patch");
        assert!(after.contains("fn main() -> Int"));
        assert!(!after.contains("timeout"));
    }
}
