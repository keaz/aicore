#[cfg(test)]
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};

use crate::ast::{self, Expr, ExprKind, Pattern, PatternKind, Stmt};
use crate::driver::{has_errors, run_frontend_with_options, FrontendOptions};
use crate::machine_paths;
use crate::package_workflow::read_manifest;
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
#[serde(deny_unknown_fields)]
pub struct PatchDocument {
    pub operations: Vec<PatchOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct PatchParamSpec {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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

struct ValidationWorkspace {
    path: PathBuf,
}

impl ValidationWorkspace {
    fn root(&self) -> &Path {
        &self.path
    }
}

impl Drop for ValidationWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
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
    match read_patch_document(patch_path) {
        Ok(document) => apply_patch_document(project_root, &document, mode),
        Err(err) => Ok(document_error_response(patch_path, mode, err.to_string())),
    }
}

pub fn apply_patch_document(
    project_root: &Path,
    document: &PatchDocument,
    mode: PatchMode,
) -> anyhow::Result<PatchResponse> {
    let project_root = machine_paths::canonical_machine_path_buf(project_root);
    let mut file_states = BTreeMap::<PathBuf, FileState>::new();
    let mut applied_edits = Vec::<PatchEdit>::new();
    let mut previews = Vec::<PatchPreview>::new();
    let mut conflicts = Vec::<PatchConflict>::new();
    let mut validation_workspace = None;
    let mut seen_operation_targets = BTreeMap::<String, usize>::new();

    for (operation_index, operation) in document.operations.iter().enumerate() {
        match apply_operation(
            &project_root,
            operation,
            operation_index,
            &mut file_states,
            &mut validation_workspace,
            &mut seen_operation_targets,
        ) {
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
        if let Err(conflict) =
            write_changed_files_transactionally(&changed_paths, &file_states, &applied_edits)
        {
            conflicts.push(conflict);
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
    validation_workspace: &mut Option<ValidationWorkspace>,
    seen_operation_targets: &mut BTreeMap<String, usize>,
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

    let operation_target = operation_target_key(&target_path, operation);
    if let Some(previous_operation) =
        seen_operation_targets.insert(operation_target.clone(), operation_index)
    {
        return Err(PatchConflict {
            operation_index,
            kind: "overlap".to_string(),
            message: format!(
                "operation overlaps semantic target from operation {}: {}",
                previous_operation, operation_target
            ),
            file: Some(display_path(&target_path)),
        });
    }

    let current_source = match ensure_file_state(file_states, &target_path) {
        Ok(state) => state.current.clone(),
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
    let (program, diagnostics) = parser::parse(&current_source, &parse_label);
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
        &current_source,
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
            before: current_source
                .get(edit.start..edit.end)
                .unwrap_or_default()
                .to_string(),
            after: edit.replacement.clone(),
            message: edit.message.clone(),
            operation_index,
        })
        .collect::<Vec<_>>();

    let updated = match apply_text_edits(&current_source, &edits) {
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

    if let Err(message) = validate_semantics(
        project_root,
        &target_path,
        &current_source,
        &updated,
        file_states,
        validation_workspace,
    ) {
        return Err(PatchConflict {
            operation_index,
            kind: "validate_semantics".to_string(),
            message,
            file: Some(display_path(&target_path)),
        });
    }

    let state = file_states
        .get_mut(&target_path)
        .ok_or_else(|| PatchConflict {
            operation_index,
            kind: "state".to_string(),
            message: format!(
                "failed to reload in-memory state for {}",
                display_path(&target_path)
            ),
            file: Some(display_path(&target_path)),
        })?;
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
                return Ok(machine_paths::canonical_machine_path_buf(&fallback));
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

fn operation_target_key(target_path: &Path, operation: &PatchOperation) -> String {
    let file = display_path(target_path);
    match operation {
        PatchOperation::AddFunction { function, .. } => {
            format!("add_function::{file}::{}", function.name.trim())
        }
        PatchOperation::ModifyMatchArm {
            target_function,
            match_index,
            arm_pattern,
            ..
        } => format!(
            "modify_match_arm::{file}::{}::{}::{}",
            target_function.trim(),
            match_index,
            normalize_ws(arm_pattern)
        ),
        PatchOperation::AddField {
            target_struct,
            field,
            ..
        } => format!(
            "add_field::{file}::{}::{}",
            target_struct.trim(),
            field.name.trim()
        ),
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
        Ok(machine_paths::canonical_machine_path_buf(&path))
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
        .map(|symbol| machine_paths::canonical_machine_path_buf(Path::new(&symbol.location.file)))
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
        ExprKind::TemplateLiteral { args, .. } => {
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
    summarize_error_diagnostics(diagnostics)
}

fn summarize_error_diagnostics(diagnostics: &[crate::diagnostics::Diagnostic]) -> String {
    diagnostics
        .iter()
        .filter(|diag| diag.is_error())
        .take(3)
        .map(|diag| format!("{}: {}", diag.code, diag.message))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn display_path(path: &Path) -> String {
    machine_paths::canonical_machine_path(path)
}

fn document_error_response(path: &Path, mode: PatchMode, message: String) -> PatchResponse {
    PatchResponse {
        protocol_version: PATCH_PROTOCOL_VERSION.to_string(),
        phase: "patch".to_string(),
        mode: mode.as_str().to_string(),
        ok: false,
        files_changed: Vec::new(),
        applied_edits: Vec::new(),
        conflicts: vec![PatchConflict {
            operation_index: 0,
            kind: "document".to_string(),
            message,
            file: Some(display_path(path)),
        }],
        previews: Vec::new(),
    }
}

#[derive(Debug, Clone)]
struct StagedPatchWrite {
    path: PathBuf,
    staged_path: PathBuf,
    backup_path: PathBuf,
    expected_original: String,
    operation_index: usize,
}

fn write_changed_files_transactionally(
    changed_paths: &[PathBuf],
    file_states: &BTreeMap<PathBuf, FileState>,
    applied_edits: &[PatchEdit],
) -> Result<(), PatchConflict> {
    let mut staged_writes = Vec::<StagedPatchWrite>::new();
    for path in changed_paths {
        let state = file_states.get(path).ok_or_else(|| PatchConflict {
            operation_index: 0,
            kind: "write".to_string(),
            message: format!("missing in-memory state for {}", path.display()),
            file: Some(display_path(path)),
        })?;
        let operation_index = applied_edits
            .iter()
            .filter(|edit| edit.file == display_path(path))
            .map(|edit| edit.operation_index)
            .max()
            .unwrap_or(0);
        if should_simulate_write_prepare_failure(path) {
            return Err(PatchConflict {
                operation_index,
                kind: "write_prepare".to_string(),
                message: format!("simulated write-prepare failure for {}", display_path(path)),
                file: Some(display_path(path)),
            });
        }

        let on_disk = fs::read_to_string(path).map_err(|err| PatchConflict {
            operation_index,
            kind: "precondition".to_string(),
            message: format!(
                "failed to read precondition state for {}: {err}",
                path.display()
            ),
            file: Some(display_path(path)),
        })?;
        if on_disk != state.original {
            return Err(PatchConflict {
                operation_index,
                kind: "precondition".to_string(),
                message: format!(
                    "precondition failed: file changed before apply commit for {}",
                    path.display()
                ),
                file: Some(display_path(path)),
            });
        }

        let staged_path =
            allocate_peer_temp_path(path, "stage", operation_index).map_err(|err| {
                PatchConflict {
                    operation_index,
                    kind: "write_prepare".to_string(),
                    message: err,
                    file: Some(display_path(path)),
                }
            })?;
        write_temp_file(&staged_path, &state.current).map_err(|err| PatchConflict {
            operation_index,
            kind: "write_prepare".to_string(),
            message: format!(
                "failed to stage patch output for {}: {err}",
                staged_path.display()
            ),
            file: Some(display_path(path)),
        })?;

        let backup_path =
            allocate_peer_temp_path(path, "backup", operation_index).map_err(|err| {
                PatchConflict {
                    operation_index,
                    kind: "write_prepare".to_string(),
                    message: err,
                    file: Some(display_path(path)),
                }
            })?;
        staged_writes.push(StagedPatchWrite {
            path: path.clone(),
            staged_path,
            backup_path,
            expected_original: state.original.clone(),
            operation_index,
        });
    }

    run_before_commit_hook(changed_paths);

    for staged in &staged_writes {
        let on_disk = fs::read_to_string(&staged.path).map_err(|err| PatchConflict {
            operation_index: staged.operation_index,
            kind: "precondition".to_string(),
            message: format!(
                "failed to read precondition state for {}: {err}",
                staged.path.display()
            ),
            file: Some(display_path(&staged.path)),
        })?;
        if on_disk != staged.expected_original {
            cleanup_staged_outputs(&staged_writes);
            return Err(PatchConflict {
                operation_index: staged.operation_index,
                kind: "precondition".to_string(),
                message: format!(
                    "precondition failed: file changed before commit for {}",
                    staged.path.display()
                ),
                file: Some(display_path(&staged.path)),
            });
        }
    }

    if let Err(conflict) = commit_staged_writes(&staged_writes) {
        cleanup_staged_outputs(&staged_writes);
        return Err(conflict);
    }

    cleanup_backup_outputs(&staged_writes);
    cleanup_staged_outputs(&staged_writes);
    Ok(())
}

fn commit_staged_writes(staged_writes: &[StagedPatchWrite]) -> Result<(), PatchConflict> {
    let mut moved_to_backup = Vec::<usize>::new();
    for (index, staged) in staged_writes.iter().enumerate() {
        if let Err(err) = fs::rename(&staged.path, &staged.backup_path) {
            let rollback_errors = rollback_backups(staged_writes, &moved_to_backup);
            let mut message = format!(
                "commit phase failed while creating backup for {}: {err}",
                staged.path.display()
            );
            if !rollback_errors.is_empty() {
                message.push_str("; rollback failed for ");
                message.push_str(&rollback_errors.join(", "));
            }
            return Err(PatchConflict {
                operation_index: staged.operation_index,
                kind: "commit".to_string(),
                message,
                file: Some(display_path(&staged.path)),
            });
        }
        moved_to_backup.push(index);
    }

    for (index, staged) in staged_writes.iter().enumerate() {
        if should_simulate_commit_failure(index) {
            let rollback_errors = rollback_backups(staged_writes, &moved_to_backup);
            let mut message = format!(
                "simulated commit failure while replacing {}",
                staged.path.display()
            );
            if !rollback_errors.is_empty() {
                message.push_str("; rollback failed for ");
                message.push_str(&rollback_errors.join(", "));
            }
            return Err(PatchConflict {
                operation_index: staged.operation_index,
                kind: "commit".to_string(),
                message,
                file: Some(display_path(&staged.path)),
            });
        }
        if let Err(err) = fs::rename(&staged.staged_path, &staged.path) {
            let rollback_errors = rollback_backups(staged_writes, &moved_to_backup);
            let mut message = format!(
                "commit phase failed while replacing {}: {err}",
                staged.path.display()
            );
            if !rollback_errors.is_empty() {
                message.push_str("; rollback failed for ");
                message.push_str(&rollback_errors.join(", "));
            }
            return Err(PatchConflict {
                operation_index: staged.operation_index,
                kind: "commit".to_string(),
                message,
                file: Some(display_path(&staged.path)),
            });
        }
    }

    Ok(())
}

fn rollback_backups(staged_writes: &[StagedPatchWrite], moved_to_backup: &[usize]) -> Vec<String> {
    let mut failures = Vec::new();
    for index in moved_to_backup.iter().copied().rev() {
        let staged = &staged_writes[index];
        if staged.path.exists() {
            if let Err(err) = fs::remove_file(&staged.path) {
                failures.push(format!(
                    "{} (failed to remove partial target: {err})",
                    staged.path.display()
                ));
                continue;
            }
        }
        if staged.backup_path.exists() {
            if let Err(err) = fs::rename(&staged.backup_path, &staged.path) {
                failures.push(format!("{} ({err})", staged.path.display()));
            }
        }
    }
    failures
}

fn cleanup_staged_outputs(staged_writes: &[StagedPatchWrite]) {
    for staged in staged_writes {
        let _ = fs::remove_file(&staged.staged_path);
    }
}

fn cleanup_backup_outputs(staged_writes: &[StagedPatchWrite]) {
    for staged in staged_writes {
        let _ = fs::remove_file(&staged.backup_path);
    }
}

fn allocate_peer_temp_path(
    target: &Path,
    role: &str,
    operation_index: usize,
) -> Result<PathBuf, String> {
    let parent = target.parent().ok_or_else(|| {
        format!(
            "failed to resolve parent directory for {}",
            target.display()
        )
    })?;
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("patch");
    let pid = std::process::id();
    for _ in 0..32 {
        let sequence = PATCH_WRITE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{file_name}.aic-patch-{role}-{pid}-{operation_index}-{sequence}.tmp"
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "failed to allocate temporary {role} path for {}",
        target.display()
    ))
}

fn write_temp_file(path: &Path, content: &str) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(content.as_bytes())?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

fn validate_semantics(
    project_root: &Path,
    target_path: &Path,
    previous_source: &str,
    updated_source: &str,
    file_states: &BTreeMap<PathBuf, FileState>,
    validation_workspace: &mut Option<ValidationWorkspace>,
) -> Result<(), String> {
    let workspace = prepare_validation_workspace(project_root, validation_workspace)?;
    let temp_target = map_validation_path(workspace.root(), project_root, target_path);
    if let Some(parent) = temp_target.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to prepare validation path {}: {err}",
                parent.display()
            )
        })?;
    }

    fs::write(&temp_target, updated_source).map_err(|err| {
        format!(
            "failed to stage semantic validation input {}: {err}",
            temp_target.display()
        )
    })?;

    let result = run_validation_inputs(
        project_root,
        workspace.root(),
        file_states.keys().collect::<Vec<_>>(),
    );
    if let Err(message) = result {
        let _ = fs::write(&temp_target, previous_source);
        return Err(message);
    }

    Ok(())
}

fn prepare_validation_workspace<'a>(
    project_root: &Path,
    validation_workspace: &'a mut Option<ValidationWorkspace>,
) -> Result<&'a mut ValidationWorkspace, String> {
    if validation_workspace.is_none() {
        let path = fresh_temp_workspace("patch");
        fs::create_dir_all(&path)
            .map_err(|err| format!("failed to create temp validation workspace: {err}"))?;
        copy_tree(project_root, &path)
            .map_err(|err| format!("failed to copy validation workspace: {err}"))?;
        *validation_workspace = Some(ValidationWorkspace { path });
    }

    validation_workspace
        .as_mut()
        .ok_or_else(|| "failed to initialize validation workspace".to_string())
}

fn run_validation_inputs(
    project_root: &Path,
    temp_root: &Path,
    changed_paths: Vec<&PathBuf>,
) -> Result<(), String> {
    let inputs = resolve_validation_inputs(project_root, temp_root, changed_paths)?;
    for input in inputs {
        let output =
            run_frontend_with_options(&input, FrontendOptions::default()).map_err(|err| {
                format!(
                    "failed to run semantic validation for {}: {err}",
                    input.display()
                )
            })?;
        if has_errors(&output.diagnostics) {
            return Err(format!(
                "semantic validation failed for {}: {}",
                input.display(),
                summarize_error_diagnostics(&output.diagnostics)
            ));
        }
    }
    Ok(())
}

fn resolve_validation_inputs(
    project_root: &Path,
    temp_root: &Path,
    changed_paths: Vec<&PathBuf>,
) -> Result<Vec<PathBuf>, String> {
    if let Some(manifest) = read_manifest(project_root)
        .map_err(|err| format!("failed to read project manifest for semantic validation: {err}"))?
    {
        return Ok(vec![temp_root.join(manifest.main)]);
    }

    let fallback = project_root.join("src/main.aic");
    if fallback.exists() {
        return Ok(vec![temp_root.join("src/main.aic")]);
    }

    let mut inputs = changed_paths
        .into_iter()
        .map(|path| map_validation_path(temp_root, project_root, path))
        .collect::<Vec<_>>();
    inputs.sort();
    inputs.dedup();

    if inputs.is_empty() {
        return Err("no changed files available for semantic validation".to_string());
    }

    Ok(inputs)
}

fn map_validation_path(temp_root: &Path, project_root: &Path, path: &Path) -> PathBuf {
    if let Ok(relative) = path.strip_prefix(project_root) {
        temp_root.join(relative)
    } else {
        temp_root
            .join("external")
            .join(sanitize_external_path(path))
    }
}

fn sanitize_external_path(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn copy_tree(source_root: &Path, destination_root: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(destination_root)
        .with_context(|| format!("failed to create {}", destination_root.display()))?;
    let mut entries = fs::read_dir(source_root)
        .with_context(|| format!("failed to read {}", source_root.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let source_path = entry.path();
        let name = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if source_path == destination_root || name.starts_with("aicore-patch-") {
            continue;
        }
        if should_skip_validation_workspace_entry(name) {
            continue;
        }
        let destination_path = destination_root.join(entry.file_name());
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if ty.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "failed to copy {} into {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
            let mut permissions = fs::metadata(&destination_path)?.permissions();
            if permissions.readonly() {
                permissions.set_readonly(false);
                fs::set_permissions(&destination_path, permissions).with_context(|| {
                    format!(
                        "failed to normalize temp validation permissions for {}",
                        destination_path.display()
                    )
                })?;
            }
        }
    }
    Ok(())
}

fn should_skip_validation_workspace_entry(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "target"
            | ".aic"
            | ".aic-cache"
            | ".aic-checkpoints"
            | ".aic-replay"
            | ".aic-sessions"
    )
}

static PATCH_WRITE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(not(test))]
fn should_simulate_write_prepare_failure(_path: &Path) -> bool {
    false
}

#[cfg(test)]
fn should_simulate_write_prepare_failure(path: &Path) -> bool {
    TEST_WRITE_PREPARE_FAIL_PATH.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|candidate| candidate == path)
    })
}

#[cfg(not(test))]
fn should_simulate_commit_failure(_index: usize) -> bool {
    false
}

#[cfg(test)]
fn should_simulate_commit_failure(index: usize) -> bool {
    TEST_COMMIT_FAIL_INDEX.with(|slot| slot.borrow().as_ref().is_some_and(|value| *value == index))
}

#[cfg(not(test))]
fn run_before_commit_hook(_changed_paths: &[PathBuf]) {}

#[cfg(test)]
fn run_before_commit_hook(changed_paths: &[PathBuf]) {
    TEST_BEFORE_COMMIT_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow().as_ref() {
            (hook)(changed_paths);
        }
    });
}

fn fresh_temp_workspace(tag: &str) -> PathBuf {
    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let seq = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    #[cfg(test)]
    if let Some(root) = TEST_TEMP_ROOT.with(|slot| slot.borrow().clone()) {
        return root.join(format!("aicore-{tag}-{pid}-{nanos}-{seq}"));
    }
    std::env::temp_dir().join(format!("aicore-{tag}-{pid}-{nanos}-{seq}"))
}

#[cfg(test)]
thread_local! {
    static TEST_TEMP_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static TEST_WRITE_PREPARE_FAIL_PATH: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static TEST_COMMIT_FAIL_INDEX: RefCell<Option<usize>> = const { RefCell::new(None) };
    static TEST_BEFORE_COMMIT_HOOK: RefCell<Option<Box<dyn Fn(&[PathBuf])>>> = RefCell::new(None);
}

#[cfg(test)]
struct TestTempRootScope {
    previous: Option<PathBuf>,
}

#[cfg(test)]
impl Drop for TestTempRootScope {
    fn drop(&mut self) {
        TEST_TEMP_ROOT.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}

#[cfg(test)]
struct TestWritePrepareFailureScope {
    previous: Option<PathBuf>,
}

#[cfg(test)]
impl Drop for TestWritePrepareFailureScope {
    fn drop(&mut self) {
        TEST_WRITE_PREPARE_FAIL_PATH.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}

#[cfg(test)]
struct TestCommitFailureScope {
    previous: Option<usize>,
}

#[cfg(test)]
impl Drop for TestCommitFailureScope {
    fn drop(&mut self) {
        TEST_COMMIT_FAIL_INDEX.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}

#[cfg(test)]
struct TestBeforeCommitHookScope {
    previous: Option<Box<dyn Fn(&[PathBuf])>>,
}

#[cfg(test)]
impl Drop for TestBeforeCommitHookScope {
    fn drop(&mut self) {
        TEST_BEFORE_COMMIT_HOOK.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}

#[cfg(test)]
fn with_test_temp_root<T>(root: &Path, op: impl FnOnce() -> T) -> T {
    let scope = TEST_TEMP_ROOT.with(|slot| TestTempRootScope {
        previous: slot.replace(Some(root.to_path_buf())),
    });
    let output = op();
    drop(scope);
    output
}

#[cfg(test)]
fn with_test_write_prepare_failure<T>(path: &Path, op: impl FnOnce() -> T) -> T {
    let canonical = machine_paths::canonical_machine_path_buf(path);
    let scope = TEST_WRITE_PREPARE_FAIL_PATH.with(|slot| TestWritePrepareFailureScope {
        previous: slot.replace(Some(canonical)),
    });
    let output = op();
    drop(scope);
    output
}

#[cfg(test)]
fn with_test_commit_failure<T>(index: usize, op: impl FnOnce() -> T) -> T {
    let scope = TEST_COMMIT_FAIL_INDEX.with(|slot| TestCommitFailureScope {
        previous: slot.replace(Some(index)),
    });
    let output = op();
    drop(scope);
    output
}

#[cfg(test)]
fn with_test_before_commit_hook<T>(
    hook: impl Fn(&[PathBuf]) + 'static,
    op: impl FnOnce() -> T,
) -> T {
    let scope = TEST_BEFORE_COMMIT_HOOK.with(|slot| TestBeforeCommitHookScope {
        previous: slot.replace(Some(Box::new(hook))),
    });
    let output = op();
    drop(scope);
    output
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

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        apply_patch_document, copy_tree, format_patch_response_text, PatchDocument, PatchMode,
        PatchOperation, PatchParamSpec,
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
        let expected_file = crate::machine_paths::canonical_machine_path(&source_path);
        assert_eq!(preview.files_changed[0], expected_file);
        assert!(preview
            .applied_edits
            .iter()
            .all(|edit| edit.file == preview.files_changed[0]));
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
    fn patch_document_rejects_unknown_fields() {
        let err = serde_json::from_value::<PatchDocument>(json!({
            "operations": [
                {
                    "kind": "add_function",
                    "target_file": "src/main.aic",
                    "function": {
                        "name": "helper",
                        "return_type": "Int",
                        "body": "0",
                        "unexpected": true
                    }
                }
            ]
        }))
        .expect_err("unknown fields must be rejected");
        assert!(err.to_string().contains("unexpected"));
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

    #[test]
    fn patch_semantic_validation_rejects_invalid_function_body() {
        let dir = tempdir().expect("tempdir");
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("mkdir src");
        let source_path = src_dir.join("main.aic");

        let original = "module demo.patch;\nfn main() -> Int {\n    0\n}\n";
        fs::write(&source_path, original).expect("write source");

        let document = PatchDocument {
            operations: vec![PatchOperation::AddFunction {
                target_file: Some("src/main.aic".to_string()),
                after_symbol: Some("main".to_string()),
                function: super::PatchFunctionSpec {
                    name: "broken".to_string(),
                    params: Vec::new(),
                    return_type: "Int".to_string(),
                    body: "true".to_string(),
                    effects: Vec::new(),
                    capabilities: Vec::new(),
                    requires: None,
                    ensures: None,
                },
            }],
        };

        let preview = apply_patch_document(dir.path(), &document, PatchMode::Preview)
            .expect("preview response");
        assert!(!preview.ok);
        assert_eq!(preview.conflicts.len(), 1);
        assert_eq!(preview.conflicts[0].kind, "validate_semantics");

        let apply =
            apply_patch_document(dir.path(), &document, PatchMode::Apply).expect("apply response");
        assert!(!apply.ok);
        assert_eq!(apply.conflicts.len(), 1);
        assert_eq!(apply.conflicts[0].kind, "validate_semantics");

        let after = fs::read_to_string(&source_path).expect("read source after semantic failure");
        assert_eq!(after, original);
    }

    #[test]
    fn patch_overlapping_match_arm_operations_are_rejected() {
        let dir = tempdir().expect("tempdir");
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("mkdir src");
        let source_path = src_dir.join("main.aic");

        let original = concat!(
            "module demo.patch;\n",
            "fn main() -> Int {\n",
            "    match Ok(1) {\n",
            "        Ok(v) => v,\n",
            "        Err(e) => e,\n",
            "    }\n",
            "}\n",
        );
        fs::write(&source_path, original).expect("write source");

        let document = PatchDocument {
            operations: vec![
                PatchOperation::ModifyMatchArm {
                    target_file: Some("src/main.aic".to_string()),
                    target_function: "main".to_string(),
                    match_index: 0,
                    arm_pattern: "Err(e)".to_string(),
                    new_body: "0 - e".to_string(),
                },
                PatchOperation::ModifyMatchArm {
                    target_file: Some("src/main.aic".to_string()),
                    target_function: "main".to_string(),
                    match_index: 0,
                    arm_pattern: "Err(e)".to_string(),
                    new_body: "e + 1".to_string(),
                },
            ],
        };

        let response =
            apply_patch_document(dir.path(), &document, PatchMode::Apply).expect("patch response");
        assert!(!response.ok);
        assert_eq!(response.conflicts.len(), 1);
        assert_eq!(response.conflicts[0].kind, "overlap");

        let after = fs::read_to_string(&source_path).expect("read source after overlap conflict");
        assert_eq!(after, original);
    }

    #[test]
    fn patch_write_prepare_failure_leaves_workspace_unchanged() {
        let dir = tempdir().expect("tempdir");
        let first_dir = dir.path().join("a");
        let second_dir = dir.path().join("z");
        fs::create_dir_all(&first_dir).expect("mkdir first");
        fs::create_dir_all(&second_dir).expect("mkdir second");

        let first_path = first_dir.join("one.aic");
        let second_path = second_dir.join("two.aic");
        let first_original = "fn main() -> Int {\n    1\n}\n";
        let second_original = "fn main() -> Int {\n    2\n}\n";
        fs::write(&first_path, first_original).expect("write first source");
        fs::write(&second_path, second_original).expect("write second source");

        let document = PatchDocument {
            operations: vec![
                PatchOperation::AddFunction {
                    target_file: Some("a/one.aic".to_string()),
                    after_symbol: Some("main".to_string()),
                    function: super::PatchFunctionSpec {
                        name: "helper_one".to_string(),
                        params: Vec::new(),
                        return_type: "Int".to_string(),
                        body: "1".to_string(),
                        effects: Vec::new(),
                        capabilities: Vec::new(),
                        requires: None,
                        ensures: None,
                    },
                },
                PatchOperation::AddFunction {
                    target_file: Some("z/two.aic".to_string()),
                    after_symbol: Some("main".to_string()),
                    function: super::PatchFunctionSpec {
                        name: "helper_two".to_string(),
                        params: Vec::new(),
                        return_type: "Int".to_string(),
                        body: "2".to_string(),
                        effects: Vec::new(),
                        capabilities: Vec::new(),
                        requires: None,
                        ensures: None,
                    },
                },
            ],
        };

        let response = super::with_test_write_prepare_failure(&second_path, || {
            apply_patch_document(dir.path(), &document, PatchMode::Apply).expect("apply response")
        });

        assert!(!response.ok);
        assert_eq!(response.conflicts.len(), 1);
        assert_eq!(response.conflicts[0].kind, "write_prepare");
        assert_eq!(
            fs::read_to_string(&first_path).expect("read first after rollback"),
            first_original
        );
        assert_eq!(
            fs::read_to_string(&second_path).expect("read second after failed write"),
            second_original
        );
    }

    #[test]
    fn patch_commit_failure_restores_original_files() {
        let dir = tempdir().expect("tempdir");
        let first_dir = dir.path().join("a");
        let second_dir = dir.path().join("b");
        fs::create_dir_all(&first_dir).expect("mkdir first");
        fs::create_dir_all(&second_dir).expect("mkdir second");

        let first_path = first_dir.join("one.aic");
        let second_path = second_dir.join("two.aic");
        let first_original = "fn main() -> Int {\n    1\n}\n";
        let second_original = "fn main() -> Int {\n    2\n}\n";
        fs::write(&first_path, first_original).expect("write first source");
        fs::write(&second_path, second_original).expect("write second source");

        let document = PatchDocument {
            operations: vec![
                PatchOperation::AddFunction {
                    target_file: Some("a/one.aic".to_string()),
                    after_symbol: Some("main".to_string()),
                    function: super::PatchFunctionSpec {
                        name: "helper_one".to_string(),
                        params: Vec::new(),
                        return_type: "Int".to_string(),
                        body: "1".to_string(),
                        effects: Vec::new(),
                        capabilities: Vec::new(),
                        requires: None,
                        ensures: None,
                    },
                },
                PatchOperation::AddFunction {
                    target_file: Some("b/two.aic".to_string()),
                    after_symbol: Some("main".to_string()),
                    function: super::PatchFunctionSpec {
                        name: "helper_two".to_string(),
                        params: Vec::new(),
                        return_type: "Int".to_string(),
                        body: "2".to_string(),
                        effects: Vec::new(),
                        capabilities: Vec::new(),
                        requires: None,
                        ensures: None,
                    },
                },
            ],
        };

        let response = super::with_test_commit_failure(0, || {
            apply_patch_document(dir.path(), &document, PatchMode::Apply).expect("apply response")
        });

        assert!(!response.ok);
        assert_eq!(response.conflicts.len(), 1);
        assert_eq!(response.conflicts[0].kind, "commit");
        assert_eq!(
            fs::read_to_string(&first_path).expect("read first after commit rollback"),
            first_original
        );
        assert_eq!(
            fs::read_to_string(&second_path).expect("read second after commit rollback"),
            second_original
        );
    }

    #[test]
    fn patch_precondition_failure_blocks_commit_when_file_changes_mid_apply() {
        let dir = tempdir().expect("tempdir");
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("mkdir src");
        let source_path = src_dir.join("main.aic");

        let original = "module demo.patch;\nfn main() -> Int {\n    0\n}\n";
        fs::write(&source_path, original).expect("write source");

        let document = PatchDocument {
            operations: vec![PatchOperation::AddFunction {
                target_file: Some("src/main.aic".to_string()),
                after_symbol: Some("main".to_string()),
                function: super::PatchFunctionSpec {
                    name: "helper".to_string(),
                    params: Vec::new(),
                    return_type: "Int".to_string(),
                    body: "1".to_string(),
                    effects: Vec::new(),
                    capabilities: Vec::new(),
                    requires: None,
                    ensures: None,
                },
            }],
        };

        let concurrent_path = source_path.clone();
        let concurrent_contents = "module demo.patch;\nfn main() -> Int {\n    41\n}\n".to_string();
        let response = super::with_test_before_commit_hook(
            move |_changed| {
                fs::write(&concurrent_path, &concurrent_contents).expect("mutate before commit");
            },
            || {
                apply_patch_document(dir.path(), &document, PatchMode::Apply)
                    .expect("apply response")
            },
        );

        assert!(!response.ok);
        assert_eq!(response.conflicts.len(), 1);
        assert_eq!(response.conflicts[0].kind, "precondition");
        assert_eq!(
            fs::read_to_string(&source_path).expect("read source after precondition conflict"),
            "module demo.patch;\nfn main() -> Int {\n    41\n}\n"
        );
    }

    #[test]
    fn copy_tree_skips_transient_validation_workspace_state() {
        let source = tempdir().expect("source");
        let destination = tempdir().expect("destination");
        fs::create_dir_all(source.path().join("src")).expect("mkdir src");
        fs::create_dir_all(source.path().join(".aic-cache/harness")).expect("mkdir cache");
        fs::create_dir_all(source.path().join(".aic-replay")).expect("mkdir replay");
        fs::write(
            source.path().join("src/main.aic"),
            "module demo.patch;\nfn main() -> Int {\n    0\n}\n",
        )
        .expect("write source main");
        fs::write(
            source.path().join(".aic-cache/harness/ignored.aic"),
            "module ignored.cache;\nfn main() -> Int { 0 }\n",
        )
        .expect("write cached file");
        fs::write(source.path().join(".aic-replay/session.json"), "{}\n")
            .expect("write replay file");

        copy_tree(source.path(), destination.path()).expect("copy tree");

        assert!(
            destination.path().join("src/main.aic").exists(),
            "expected source files to be copied"
        );
        assert!(
            !destination.path().join(".aic-cache").exists(),
            "validation workspace copy must skip .aic-cache"
        );
        assert!(
            !destination.path().join(".aic-replay").exists(),
            "validation workspace copy must skip replay artifacts"
        );
    }

    #[test]
    fn patch_preview_succeeds_when_tmpdir_points_inside_project_root() {
        let dir = tempdir().expect("tempdir");
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("mkdir src");
        fs::create_dir_all(dir.path().join(".aic-cache/harness")).expect("mkdir cache");
        let source_path = src_dir.join("main.aic");
        fs::write(
            &source_path,
            concat!(
                "module demo.patch;\n",
                "fn main() -> Int {\n",
                "    0\n",
                "}\n",
            ),
        )
        .expect("write source");
        fs::write(
            dir.path().join(".aic-cache/harness/ignored.aic"),
            "module ignored.cache;\nfn main() -> Int {\n    0\n}\n",
        )
        .expect("write cached source");

        let document = PatchDocument {
            operations: vec![PatchOperation::AddFunction {
                target_file: Some("src/main.aic".to_string()),
                after_symbol: Some("main".to_string()),
                function: super::PatchFunctionSpec {
                    name: "helper".to_string(),
                    params: Vec::new(),
                    return_type: "Int".to_string(),
                    body: "1".to_string(),
                    effects: Vec::new(),
                    capabilities: Vec::new(),
                    requires: None,
                    ensures: None,
                },
            }],
        };

        let preview = super::with_test_temp_root(dir.path(), || {
            apply_patch_document(dir.path(), &document, PatchMode::Preview)
        })
        .expect("preview response");

        assert!(preview.ok, "conflicts: {:#?}", preview.conflicts);
        assert_eq!(preview.conflicts.len(), 0);
    }
}
