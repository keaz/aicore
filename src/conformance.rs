use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::codegen::{compile_with_clang, emit_llvm_with_resolution_and_options, CodegenOptions};
use crate::contracts::lower_runtime_asserts;
use crate::driver::{has_errors, run_frontend};
use crate::parser;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceCategory {
    Syntax,
    Typing,
    Diagnostics,
    Codegen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceCase {
    pub id: String,
    pub category: ConformanceCategory,
    pub path: String,
    #[serde(default)]
    pub expect_output: Option<String>,
    #[serde(default)]
    pub expect_error_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceCatalog {
    pub cases: Vec<ConformanceCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceCaseResult {
    pub id: String,
    pub category: ConformanceCategory,
    pub path: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub by_category: BTreeMap<String, usize>,
    pub cases: Vec<ConformanceCaseResult>,
}

pub fn load_catalog(path: &Path) -> anyhow::Result<ConformanceCatalog> {
    let raw = fs::read_to_string(path)?;
    let catalog = serde_json::from_str::<ConformanceCatalog>(&raw)?;
    Ok(catalog)
}

pub fn run_catalog(root: &Path, catalog: &ConformanceCatalog) -> anyhow::Result<ConformanceReport> {
    let mut report = ConformanceReport {
        total: 0,
        passed: 0,
        failed: 0,
        by_category: BTreeMap::new(),
        cases: Vec::new(),
    };

    for case in &catalog.cases {
        report.total += 1;
        *report
            .by_category
            .entry(format_category(case.category).to_string())
            .or_default() += 1;

        let result = run_case(root, case);
        let (passed, details) = match result {
            Ok(details) => (true, details),
            Err(err) => (false, err.to_string()),
        };

        if passed {
            report.passed += 1;
        } else {
            report.failed += 1;
        }

        report.cases.push(ConformanceCaseResult {
            id: case.id.clone(),
            category: case.category,
            path: case.path.clone(),
            passed,
            details,
        });
    }

    Ok(report)
}

fn run_case(root: &Path, case: &ConformanceCase) -> anyhow::Result<String> {
    let path = resolve_case_path(root, &case.path);
    if !path.exists() {
        anyhow::bail!("missing conformance file: {}", path.display());
    }

    match case.category {
        ConformanceCategory::Syntax => run_syntax_case(&path),
        ConformanceCategory::Typing => run_typing_case(&path),
        ConformanceCategory::Diagnostics => run_diagnostics_case(&path, &case.expect_error_codes),
        ConformanceCategory::Codegen => run_codegen_case(&path, case.expect_output.as_deref()),
    }
}

fn run_syntax_case(path: &Path) -> anyhow::Result<String> {
    let source = fs::read_to_string(path)?;
    let (_program, diagnostics) = parser::parse(&source, &path.to_string_lossy());
    let errors = diagnostics
        .iter()
        .filter(|d| d.is_error())
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        anyhow::bail!("syntax diagnostics: {errors:#?}");
    }
    Ok("parsed".to_string())
}

fn run_typing_case(path: &Path) -> anyhow::Result<String> {
    let out = run_frontend(path)?;
    if has_errors(&out.diagnostics) {
        anyhow::bail!("typing diagnostics: {:#?}", out.diagnostics);
    }
    Ok("typed".to_string())
}

fn run_diagnostics_case(path: &Path, expected_codes: &[String]) -> anyhow::Result<String> {
    let out = run_frontend(path)?;
    if !has_errors(&out.diagnostics) {
        anyhow::bail!("expected diagnostics but frontend succeeded");
    }
    if expected_codes.is_empty() {
        return Ok("diagnostics-observed".to_string());
    }

    let observed = out
        .diagnostics
        .iter()
        .map(|d| d.code.clone())
        .collect::<Vec<_>>();
    for code in expected_codes {
        if !observed.iter().any(|d| d == code) {
            anyhow::bail!("missing expected diagnostic {code}; observed={observed:?}");
        }
    }

    Ok(format!("matched={}", expected_codes.join(",")))
}

fn run_codegen_case(path: &Path, expected_output: Option<&str>) -> anyhow::Result<String> {
    let out = run_frontend(path)?;
    if has_errors(&out.diagnostics) {
        anyhow::bail!("codegen frontend diagnostics: {:#?}", out.diagnostics);
    }

    let lowered = lower_runtime_asserts(&out.ir);
    let llvm = emit_llvm_with_resolution_and_options(
        &lowered,
        Some(&out.resolution),
        &path.to_string_lossy(),
        CodegenOptions::default(),
    )
    .map_err(|diags| anyhow::anyhow!("llvm generation failed: {diags:#?}"))?;

    let tmp = unique_temp_dir("aicore-conformance");
    fs::create_dir_all(&tmp)?;

    let exe = tmp.join("conformance-bin");
    compile_with_clang(&llvm.llvm_ir, &exe, &tmp)?;

    let output = Command::new(&exe).output()?;
    let _ = fs::remove_dir_all(&tmp);
    if !output.status.success() {
        anyhow::bail!(
            "binary exited with status {:?}; stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    if let Some(expected) = expected_output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let got = stdout
            .trim_end()
            .lines()
            .last()
            .unwrap_or_default()
            .to_string();
        if got != expected {
            anyhow::bail!("expected output `{expected}`, got `{got}`");
        }
    }

    Ok("executed".to_string())
}

fn resolve_case_path(root: &Path, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return candidate.to_path_buf();
    }
    root.join(candidate)
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

fn format_category(category: ConformanceCategory) -> &'static str {
    match category {
        ConformanceCategory::Syntax => "syntax",
        ConformanceCategory::Typing => "typing",
        ConformanceCategory::Diagnostics => "diagnostics",
        ConformanceCategory::Codegen => "codegen",
    }
}
