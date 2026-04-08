use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::codegen::{
    compile_with_clang_artifact_with_options, emit_llvm_with_resolution_and_options, ArtifactKind,
    CodegenOptions, CompileOptions, OptimizationLevel,
};
use crate::contracts::lower_runtime_asserts;
use crate::driver::{has_errors, run_frontend};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatrixMode {
    Debug,
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixTarget {
    pub id: String,
    pub os: String,
    pub execute: bool,
    pub modes: Vec<MatrixMode>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixCase {
    pub name: String,
    pub path: String,
    pub expected_stdout: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionMatrixDefinition {
    pub targets: Vec<MatrixTarget>,
    pub cases: Vec<MatrixCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixCaseResult {
    pub target: String,
    pub mode: MatrixMode,
    pub case: String,
    pub path: String,
    pub passed: bool,
    pub skipped: bool,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixReport {
    pub host_os: String,
    pub target_id: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cases: Vec<MatrixCaseResult>,
}

pub fn load_definition(path: &Path) -> anyhow::Result<ExecutionMatrixDefinition> {
    let raw = fs::read_to_string(path)?;
    let matrix = serde_json::from_str::<ExecutionMatrixDefinition>(&raw)?;
    Ok(matrix)
}

pub fn run_host_matrix(
    root: &Path,
    matrix: &ExecutionMatrixDefinition,
) -> anyhow::Result<MatrixReport> {
    let host_os = host_os_name();
    let target = matrix
        .targets
        .iter()
        .find(|target| target.os == host_os)
        .ok_or_else(|| anyhow::anyhow!("no matrix target configured for host os `{host_os}`"))?;

    let mut report = MatrixReport {
        host_os,
        target_id: target.id.clone(),
        total: 0,
        passed: 0,
        failed: 0,
        skipped: 0,
        cases: Vec::new(),
    };

    for mode in &target.modes {
        for case in &matrix.cases {
            report.total += 1;
            let result = run_case(root, target, *mode, case);
            match result {
                Ok(mut case_result) => {
                    if case_result.skipped {
                        report.skipped += 1;
                        case_result.passed = true;
                    } else if case_result.passed {
                        report.passed += 1;
                    } else {
                        report.failed += 1;
                    }
                    report.cases.push(case_result);
                }
                Err(err) => {
                    report.failed += 1;
                    report.cases.push(MatrixCaseResult {
                        target: target.id.clone(),
                        mode: *mode,
                        case: case.name.clone(),
                        path: case.path.clone(),
                        passed: false,
                        skipped: false,
                        details: err.to_string(),
                    });
                }
            }
        }
    }

    Ok(report)
}

fn run_case(
    root: &Path,
    target: &MatrixTarget,
    mode: MatrixMode,
    case: &MatrixCase,
) -> anyhow::Result<MatrixCaseResult> {
    if !target.execute {
        return Ok(MatrixCaseResult {
            target: target.id.clone(),
            mode,
            case: case.name.clone(),
            path: case.path.clone(),
            passed: false,
            skipped: true,
            details: target
                .notes
                .clone()
                .unwrap_or_else(|| "execution disabled for target".to_string()),
        });
    }

    let source_path = resolve_path(root, &case.path);
    let front = run_frontend(&source_path)?;
    if has_errors(&front.diagnostics) {
        anyhow::bail!("frontend diagnostics: {:#?}", front.diagnostics);
    }

    let lowered = lower_runtime_asserts(&front.ir);
    let debug_info = matches!(mode, MatrixMode::Debug);
    let opt_level = match mode {
        MatrixMode::Debug => OptimizationLevel::O0,
        MatrixMode::Release => OptimizationLevel::O2,
    };
    let llvm = emit_llvm_with_resolution_and_options(
        &lowered,
        Some(&front.resolution),
        &source_path.to_string_lossy(),
        CodegenOptions { debug_info },
    )
    .map_err(|diags| anyhow::anyhow!("llvm generation failed: {diags:#?}"))?;

    let tmp = unique_temp_dir("aicore-matrix");
    fs::create_dir_all(&tmp)?;
    let exe_name = if cfg!(windows) {
        "matrix-bin.exe"
    } else {
        "matrix-bin"
    };
    let exe = tmp.join(exe_name);

    compile_with_clang_artifact_with_options(
        &llvm.llvm_ir,
        &exe,
        &tmp,
        ArtifactKind::Exe,
        CompileOptions {
            debug_info,
            opt_level,
            ..CompileOptions::default()
        },
    )?;

    let output = Command::new(&exe).output()?;
    let _ = fs::remove_dir_all(&tmp);

    if !output.status.success() {
        anyhow::bail!(
            "matrix binary failed with status {:?}; stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let got = stdout
        .trim_end()
        .lines()
        .last()
        .unwrap_or_default()
        .to_string();
    if got != case.expected_stdout {
        anyhow::bail!(
            "unexpected output: expected `{}`, got `{}`",
            case.expected_stdout,
            got
        );
    }

    Ok(MatrixCaseResult {
        target: target.id.clone(),
        mode,
        case: case.name.clone(),
        path: case.path.clone(),
        passed: true,
        skipped: false,
        details: format!("stdout={}", case.expected_stdout),
    })
}

fn resolve_path(root: &Path, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    }
}

fn host_os_name() -> String {
    match std::env::consts::OS {
        "macos" => "macos".to_string(),
        "linux" => "linux".to_string(),
        "windows" => "windows".to_string(),
        other => other.to_string(),
    }
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{}-{}-{}-{}",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        seq
    ))
}

#[cfg(test)]
mod tests {
    use super::host_os_name;

    #[test]
    fn host_os_is_mapped_to_supported_label() {
        let host = host_os_name();
        assert!(!host.is_empty());
    }
}
