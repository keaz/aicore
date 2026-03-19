use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use serde_json::json;

use crate::ast;
use crate::codegen::{
    compile_with_clang_artifact_with_options, emit_llvm_with_options, ArtifactKind, CodegenOptions,
    CompileOptions, LinkOptions, OptimizationLevel,
};
use crate::contracts::{lower_runtime_asserts, verify_static_with_context};
use crate::diagnostics::{Diagnostic, Severity, SuggestedFix};
use crate::effects::{
    normalize_capability_declarations_with_context, normalize_effect_declarations_with_context,
};
use crate::formatter::format_program;
use crate::ir;
use crate::ir_builder;
use crate::package_loader;
use crate::package_loader::LoadOptions;
use crate::package_workflow::{native_link_config, NativeLinkConfig};
use crate::resolver::{self, Resolution};
use crate::telemetry;
use crate::typecheck::{self, TypecheckOutput};

pub struct FrontendOutput {
    pub ast: ast::Program,
    pub ir: ir::Program,
    pub resolution: Resolution,
    pub typecheck: TypecheckOutput,
    pub diagnostics: Vec<Diagnostic>,
    pub item_modules: Vec<Option<Vec<String>>>,
    pub timings: FrontendTimings,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FrontendTimings {
    pub load_ms: f64,
    pub ir_build_ms: f64,
    pub effect_normalize_ms: f64,
    pub resolve_ms: f64,
    pub typecheck_ms: f64,
    pub verify_ms: f64,
}

impl FrontendTimings {
    pub fn total_ms(self) -> f64 {
        self.load_ms
            + self.ir_build_ms
            + self.effect_normalize_ms
            + self.resolve_ms
            + self.typecheck_ms
            + self.verify_ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildArtifact {
    Exe,
    Obj,
    Lib,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FrontendOptions {
    pub offline: bool,
}

pub fn run_frontend(path: &Path) -> anyhow::Result<FrontendOutput> {
    run_frontend_with_options(path, FrontendOptions::default())
}

pub fn run_frontend_with_options(
    path: &Path,
    options: FrontendOptions,
) -> anyhow::Result<FrontendOutput> {
    let file = path.to_string_lossy().to_string();
    let mut timings = FrontendTimings::default();
    let load_started = Instant::now();
    let mut load = package_loader::load_entry_with_options(
        path,
        LoadOptions {
            offline: options.offline,
        },
    )?;
    timings.load_ms = elapsed_ms(load_started);
    let mut diagnostics = Vec::new();
    diagnostics.append(&mut load.diagnostics);

    let ast = if let Some(ast) = load.program {
        ast
    } else {
        return Ok(FrontendOutput {
            ast: ast::Program {
                module: None,
                imports: Vec::new(),
                items: Vec::new(),
                span: crate::span::Span::new(0, 0),
            },
            ir: ir::Program {
                schema_version: ir::CURRENT_IR_SCHEMA_VERSION,
                module: None,
                imports: Vec::new(),
                items: Vec::new(),
                symbols: Vec::new(),
                types: Vec::new(),
                generic_instantiations: Vec::new(),
                span: crate::span::Span::new(0, 0),
            },
            resolution: Resolution {
                functions: Default::default(),
                module_function_infos: Default::default(),
                structs: Default::default(),
                enums: Default::default(),
                traits: Default::default(),
                trait_impls: Default::default(),
                imports: Default::default(),
                module_imports: Default::default(),
                entry_module: None,
                function_modules: Default::default(),
                module_functions: Default::default(),
                module_exported_functions: Default::default(),
                visible_functions: Default::default(),
                import_aliases: Default::default(),
                ambiguous_import_aliases: Default::default(),
                module_import_aliases: Default::default(),
                module_ambiguous_import_aliases: Default::default(),
            },
            typecheck: TypecheckOutput::default(),
            diagnostics,
            item_modules: Vec::new(),
            timings,
        });
    };

    let ir_build_started = Instant::now();
    let mut ir = ir_builder::build(&ast);
    timings.ir_build_ms = elapsed_ms(ir_build_started);
    let normalize_started = Instant::now();
    diagnostics.extend(normalize_effect_declarations_with_context(
        &mut ir,
        &file,
        Some(&load.item_modules),
        Some(&load.module_files),
    ));
    diagnostics.extend(normalize_capability_declarations_with_context(
        &mut ir,
        &file,
        Some(&load.item_modules),
        Some(&load.module_files),
    ));
    timings.effect_normalize_ms = elapsed_ms(normalize_started);

    let resolve_started = Instant::now();
    let (resolution, resolve_diags) = resolver::resolve_with_item_modules_imports_and_files(
        &ir,
        &file,
        Some(&load.item_modules),
        Some(&load.module_imports),
        Some(&load.module_files),
    );
    timings.resolve_ms = elapsed_ms(resolve_started);
    diagnostics.extend(resolve_diags);

    let typecheck_started = Instant::now();
    let typecheck = typecheck::check_with_context(
        &ir,
        &resolution,
        &file,
        Some(&load.item_modules),
        Some(&load.module_files),
    );
    timings.typecheck_ms = elapsed_ms(typecheck_started);
    ir.generic_instantiations = typecheck.generic_instantiations.clone();
    apply_call_arg_orders(&mut ir, &typecheck.call_arg_orders);
    diagnostics.extend(typecheck.diagnostics.iter().cloned());

    let verify_started = Instant::now();
    diagnostics.extend(verify_static_with_context(
        &ir,
        &file,
        Some(&load.item_modules),
        Some(&load.module_files),
    ));
    timings.verify_ms = elapsed_ms(verify_started);

    sort_diagnostics(&mut diagnostics);

    let attrs = BTreeMap::from([
        ("input".to_string(), json!(file)),
        ("offline".to_string(), json!(options.offline)),
    ]);
    telemetry::emit_phase(
        "frontend",
        "load",
        "ok",
        std::time::Duration::from_secs_f64(timings.load_ms / 1000.0),
        attrs.clone(),
    );
    telemetry::emit_phase(
        "frontend",
        "ir_build",
        "ok",
        std::time::Duration::from_secs_f64(timings.ir_build_ms / 1000.0),
        attrs.clone(),
    );
    telemetry::emit_phase(
        "frontend",
        "resolve",
        "ok",
        std::time::Duration::from_secs_f64(timings.resolve_ms / 1000.0),
        attrs.clone(),
    );
    telemetry::emit_phase(
        "frontend",
        "typecheck",
        if has_errors(&diagnostics) {
            "error"
        } else {
            "ok"
        },
        std::time::Duration::from_secs_f64(timings.typecheck_ms / 1000.0),
        attrs.clone(),
    );
    telemetry::emit_metric(
        "frontend",
        "diagnostic_count",
        diagnostics.len() as f64,
        attrs,
    );

    Ok(FrontendOutput {
        ast,
        ir,
        resolution,
        typecheck,
        diagnostics,
        item_modules: load.item_modules,
        timings,
    })
}

pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| matches!(d.severity, Severity::Error))
}

fn apply_call_arg_orders(program: &mut ir::Program, orders: &BTreeMap<ir::NodeId, Vec<usize>>) {
    for item in &mut program.items {
        match item {
            ir::Item::Function(func) => {
                if let Some(req) = func.requires.as_mut() {
                    reorder_call_args_in_expr(req, orders);
                }
                if let Some(ens) = func.ensures.as_mut() {
                    reorder_call_args_in_expr(ens, orders);
                }
                reorder_call_args_in_block(&mut func.body, orders);
            }
            ir::Item::Struct(strukt) => {
                for field in &mut strukt.fields {
                    if let Some(default) = field.default_value.as_mut() {
                        reorder_call_args_in_expr(default, orders);
                    }
                }
                if let Some(invariant) = strukt.invariant.as_mut() {
                    reorder_call_args_in_expr(invariant, orders);
                }
            }
            ir::Item::Enum(_) => {}
            ir::Item::Trait(trait_def) => {
                for method in &mut trait_def.methods {
                    if let Some(req) = method.requires.as_mut() {
                        reorder_call_args_in_expr(req, orders);
                    }
                    if let Some(ens) = method.ensures.as_mut() {
                        reorder_call_args_in_expr(ens, orders);
                    }
                    reorder_call_args_in_block(&mut method.body, orders);
                }
            }
            ir::Item::Impl(impl_def) => {
                for method in &mut impl_def.methods {
                    if let Some(req) = method.requires.as_mut() {
                        reorder_call_args_in_expr(req, orders);
                    }
                    if let Some(ens) = method.ensures.as_mut() {
                        reorder_call_args_in_expr(ens, orders);
                    }
                    reorder_call_args_in_block(&mut method.body, orders);
                }
            }
        }
    }
}

fn reorder_call_args_in_block(block: &mut ir::Block, orders: &BTreeMap<ir::NodeId, Vec<usize>>) {
    for stmt in &mut block.stmts {
        match stmt {
            ir::Stmt::Let { expr, .. }
            | ir::Stmt::Assign { expr, .. }
            | ir::Stmt::Expr { expr, .. }
            | ir::Stmt::Assert { expr, .. } => reorder_call_args_in_expr(expr, orders),
            ir::Stmt::Return {
                expr: Some(expr), ..
            } => reorder_call_args_in_expr(expr, orders),
            ir::Stmt::Return { expr: None, .. } => {}
        }
    }
    if let Some(tail) = &mut block.tail {
        reorder_call_args_in_expr(tail, orders);
    }
}

fn reorder_call_args_in_expr(expr: &mut ir::Expr, orders: &BTreeMap<ir::NodeId, Vec<usize>>) {
    match &mut expr.kind {
        ir::ExprKind::Call {
            callee,
            args,
            arg_names,
        } => {
            reorder_call_args_in_expr(callee, orders);
            for arg in args.iter_mut() {
                reorder_call_args_in_expr(arg, orders);
            }

            if let Some(order) = orders.get(&expr.node) {
                if order.len() == args.len() {
                    let valid = order.iter().all(|idx| *idx < args.len()) && {
                        let mut seen = std::collections::BTreeSet::new();
                        order.iter().all(|idx| seen.insert(*idx))
                    };
                    if valid {
                        let new_args = order
                            .iter()
                            .map(|idx| args[*idx].clone())
                            .collect::<Vec<_>>();
                        *args = new_args;
                        if !arg_names.is_empty() {
                            let mut reordered_names = order
                                .iter()
                                .map(|idx| arg_names.get(*idx).cloned().unwrap_or(None))
                                .collect::<Vec<_>>();
                            if reordered_names.iter().all(|name| name.is_none()) {
                                reordered_names.clear();
                            }
                            *arg_names = reordered_names;
                        }
                    }
                }
            }
        }
        ir::ExprKind::TemplateLiteral { args, .. } => {
            for arg in args.iter_mut() {
                reorder_call_args_in_expr(arg, orders);
            }
        }
        ir::ExprKind::Closure { body, .. } => reorder_call_args_in_block(body, orders),
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            reorder_call_args_in_expr(cond, orders);
            reorder_call_args_in_block(then_block, orders);
            reorder_call_args_in_block(else_block, orders);
        }
        ir::ExprKind::While { cond, body } => {
            reorder_call_args_in_expr(cond, orders);
            reorder_call_args_in_block(body, orders);
        }
        ir::ExprKind::Loop { body } => reorder_call_args_in_block(body, orders),
        ir::ExprKind::Break { expr: Some(inner) } => reorder_call_args_in_expr(inner, orders),
        ir::ExprKind::Break { expr: None } | ir::ExprKind::Continue => {}
        ir::ExprKind::Match {
            expr: scrutinee,
            arms,
        } => {
            reorder_call_args_in_expr(scrutinee, orders);
            for arm in arms {
                if let Some(guard) = arm.guard.as_mut() {
                    reorder_call_args_in_expr(guard, orders);
                }
                reorder_call_args_in_expr(&mut arm.body, orders);
            }
        }
        ir::ExprKind::Binary { lhs, rhs, .. } => {
            reorder_call_args_in_expr(lhs, orders);
            reorder_call_args_in_expr(rhs, orders);
        }
        ir::ExprKind::Unary { expr: inner, .. }
        | ir::ExprKind::Borrow { expr: inner, .. }
        | ir::ExprKind::Await { expr: inner }
        | ir::ExprKind::Try { expr: inner } => reorder_call_args_in_expr(inner, orders),
        ir::ExprKind::UnsafeBlock { block } => reorder_call_args_in_block(block, orders),
        ir::ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                reorder_call_args_in_expr(value, orders);
            }
        }
        ir::ExprKind::FieldAccess { base, .. } => reorder_call_args_in_expr(base, orders),
        ir::ExprKind::Int(_)
        | ir::ExprKind::Float(_)
        | ir::ExprKind::Bool(_)
        | ir::ExprKind::Char(_)
        | ir::ExprKind::String(_)
        | ir::ExprKind::Unit
        | ir::ExprKind::Var(_) => {}
    }
}

pub fn sort_and_cap_diagnostics(
    mut diagnostics: Vec<Diagnostic>,
    max_errors: usize,
) -> Vec<Diagnostic> {
    sort_diagnostics(&mut diagnostics);
    diagnostics.dedup();
    diagnostics.truncate(max_errors);
    diagnostics
}

pub fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by_cached_key(diagnostic_sort_key);
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DiagnosticSortKey {
    first_span_start: usize,
    first_span_end: usize,
    first_span_file: String,
    severity_rank: u8,
    code: String,
    message: String,
    spans: Vec<(String, usize, usize, Option<String>)>,
    help: Vec<String>,
    fixes: Vec<DiagnosticFixKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DiagnosticFixKey {
    message: String,
    replacement: Option<String>,
    start: Option<usize>,
    end: Option<usize>,
}

fn diagnostic_sort_key(diag: &Diagnostic) -> DiagnosticSortKey {
    let first_span = diag.spans.first();
    DiagnosticSortKey {
        first_span_start: first_span.map(|span| span.start).unwrap_or(usize::MAX),
        first_span_end: first_span.map(|span| span.end).unwrap_or(usize::MAX),
        first_span_file: first_span.map(|span| span.file.clone()).unwrap_or_default(),
        severity_rank: severity_rank(&diag.severity),
        code: diag.code.clone(),
        message: diag.message.clone(),
        spans: diag
            .spans
            .iter()
            .map(|span| (span.file.clone(), span.start, span.end, span.label.clone()))
            .collect(),
        help: diag.help.clone(),
        fixes: diag.suggested_fixes.iter().map(fix_sort_key).collect(),
    }
}

fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Note => 2,
    }
}

fn fix_sort_key(fix: &SuggestedFix) -> DiagnosticFixKey {
    DiagnosticFixKey {
        message: fix.message.clone(),
        replacement: fix.replacement.clone(),
        start: fix.start,
        end: fix.end,
    }
}

pub fn format_source(path: &Path, write: bool) -> anyhow::Result<String> {
    let front = run_frontend(path)?;
    if has_errors(&front.diagnostics) {
        anyhow::bail!("cannot format file with diagnostics errors")
    }
    let formatted = format_program(&front.ir);
    if write {
        fs::write(path, &formatted)?;
    }
    Ok(formatted)
}

pub fn emit_ir_json(path: &Path) -> anyhow::Result<String> {
    let front = run_frontend(path)?;
    if has_errors(&front.diagnostics) {
        anyhow::bail!("cannot emit IR with diagnostics errors")
    }
    let json = serde_json::to_string_pretty(&front.ir)?;
    Ok(json)
}

pub fn build(path: &Path, output: &Path) -> anyhow::Result<PathBuf> {
    build_with_artifact(path, output, BuildArtifact::Exe)
}

pub fn build_with_artifact(
    path: &Path,
    output: &Path,
    artifact: BuildArtifact,
) -> anyhow::Result<PathBuf> {
    build_with_artifact_options(path, output, artifact, false)
}

pub fn build_with_artifact_options(
    path: &Path,
    output: &Path,
    artifact: BuildArtifact,
    debug_info: bool,
) -> anyhow::Result<PathBuf> {
    let project_root = resolve_project_root(path);
    let link = resolve_native_link_options(&project_root)?;
    let front = run_frontend(path)?;
    if has_errors(&front.diagnostics) {
        anyhow::bail!("build failed due to diagnostics")
    }

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm_with_options(
        &lowered,
        &path.to_string_lossy(),
        CodegenOptions { debug_info },
    )
    .map_err(|_| anyhow::anyhow!("llvm codegen failed"))?;

    let work_dir = fresh_work_dir("build");
    let output = compile_with_clang_artifact_with_options(
        &llvm.llvm_ir,
        output,
        &work_dir,
        artifact.to_codegen(),
        CompileOptions {
            debug_info,
            opt_level: OptimizationLevel::O0,
            target_triple: None,
            static_link: false,
            link,
        },
    )?;
    Ok(output)
}

pub fn run(path: &Path) -> anyhow::Result<i32> {
    let exe = fresh_work_dir("run-bin").join("aicore_run_bin");
    let output = build_with_artifact(path, &exe, BuildArtifact::Exe)?;
    let status = Command::new(output).status()?;
    Ok(status.code().unwrap_or(1))
}

pub fn diagnostics_json(path: &Path) -> anyhow::Result<String> {
    let front = run_frontend(path)?;
    Ok(serde_json::to_string_pretty(&front.diagnostics)?)
}

pub fn diagnostics_pretty(diags: &[Diagnostic]) -> String {
    let mut out = String::new();
    for d in diags {
        let sev = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };
        out.push_str(&format!("{}[{}]: {}\n", sev, d.code, d.message));
        for span in &d.spans {
            out.push_str(&format!(
                "  --> {}:{}-{}\n",
                span.file, span.start, span.end
            ));
            if let Some(label) = &span.label {
                out.push_str(&format!("      = {}\n", label));
            }
        }
        for help in &d.help {
            out.push_str(&format!("      help: {}\n", help));
        }
    }
    out
}

impl BuildArtifact {
    fn to_codegen(self) -> ArtifactKind {
        match self {
            BuildArtifact::Exe => ArtifactKind::Exe,
            BuildArtifact::Obj => ArtifactKind::Obj,
            BuildArtifact::Lib => ArtifactKind::Lib,
        }
    }
}

fn resolve_project_root(path: &Path) -> PathBuf {
    let fallback = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    let mut dir = fallback.clone();

    loop {
        if dir.join("aic.toml").exists() {
            return dir;
        }
        let Some(parent) = dir.parent() else {
            return fallback;
        };
        dir = parent.to_path_buf();
    }
}

fn resolve_native_link_options(project_root: &Path) -> anyhow::Result<LinkOptions> {
    let native = native_link_config(project_root)?;
    Ok(native_to_link_options(project_root, &native))
}

fn native_to_link_options(project_root: &Path, native: &NativeLinkConfig) -> LinkOptions {
    LinkOptions {
        search_paths: native
            .search_paths
            .iter()
            .map(|path| resolve_native_path(project_root, path))
            .collect(),
        libs: native.libs.clone(),
        objects: native
            .objects
            .iter()
            .map(|path| resolve_native_path(project_root, path))
            .collect(),
    }
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn fresh_work_dir(tag: &str) -> PathBuf {
    static WORK_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let seq = WORK_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("aicore-{tag}-{pid}-{nanos}-{seq}"))
}

fn resolve_native_path(project_root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    }
}
