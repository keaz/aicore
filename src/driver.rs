use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::codegen::{
    compile_with_clang_artifact_with_options, emit_llvm_with_options, ArtifactKind, CodegenOptions,
    CompileOptions,
};
use crate::contracts::{lower_runtime_asserts, verify_static};
use crate::diagnostics::{Diagnostic, Severity};
use crate::effects::normalize_effect_declarations;
use crate::formatter::format_program;
use crate::ir;
use crate::ir_builder;
use crate::package_loader;
use crate::package_loader::LoadOptions;
use crate::resolver::{self, Resolution};
use crate::typecheck::{self, TypecheckOutput};

pub struct FrontendOutput {
    pub ir: ir::Program,
    pub resolution: Resolution,
    pub typecheck: TypecheckOutput,
    pub diagnostics: Vec<Diagnostic>,
    pub item_modules: Vec<Option<Vec<String>>>,
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
    let mut load = package_loader::load_entry_with_options(
        path,
        LoadOptions {
            offline: options.offline,
        },
    )?;
    let mut diagnostics = Vec::new();
    diagnostics.append(&mut load.diagnostics);

    let ast = if let Some(ast) = load.program {
        ast
    } else {
        return Ok(FrontendOutput {
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
                structs: Default::default(),
                enums: Default::default(),
                traits: Default::default(),
                trait_impls: Default::default(),
                imports: Default::default(),
                entry_module: None,
                function_modules: Default::default(),
                module_functions: Default::default(),
                visible_functions: Default::default(),
                import_aliases: Default::default(),
                ambiguous_import_aliases: Default::default(),
            },
            typecheck: TypecheckOutput::default(),
            diagnostics,
            item_modules: Vec::new(),
        });
    };

    let mut ir = ir_builder::build(&ast);
    diagnostics.extend(normalize_effect_declarations(&mut ir, &file));
    let (resolution, resolve_diags) =
        resolver::resolve_with_item_modules(&ir, &file, Some(&load.item_modules));
    diagnostics.extend(resolve_diags);

    let typecheck = typecheck::check(&ir, &resolution, &file);
    ir.generic_instantiations = typecheck.generic_instantiations.clone();
    diagnostics.extend(typecheck.diagnostics.iter().cloned());

    diagnostics.extend(verify_static(&ir, &file));

    diagnostics.sort_by(|a, b| {
        let a_pos = a.spans.first().map(|s| s.start).unwrap_or(usize::MAX);
        let b_pos = b.spans.first().map(|s| s.start).unwrap_or(usize::MAX);
        a_pos
            .cmp(&b_pos)
            .then(a.code.cmp(&b.code))
            .then(a.message.cmp(&b.message))
    });

    Ok(FrontendOutput {
        ir,
        resolution,
        typecheck,
        diagnostics,
        item_modules: load.item_modules,
    })
}

pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| matches!(d.severity, Severity::Error))
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

    let work_dir = std::env::temp_dir().join("aicore_build");
    let output = compile_with_clang_artifact_with_options(
        &llvm.llvm_ir,
        output,
        &work_dir,
        artifact.to_codegen(),
        CompileOptions { debug_info },
    )?;
    Ok(output)
}

pub fn run(path: &Path) -> anyhow::Result<i32> {
    let exe = std::env::temp_dir().join("aicore_run_bin");
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
