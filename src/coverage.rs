use std::fs;
use std::path::{Path, PathBuf};

use aicore::ast::Item;
use aicore::diagnostics::{Diagnostic, Severity};
use aicore::parser;
use serde::{Deserialize, Serialize};

pub const COVERAGE_REPORT_SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoverageCheckResult {
    pub min_pct: f64,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoverageSummary {
    pub files_total: usize,
    pub files_covered: usize,
    pub functions_total: usize,
    pub functions_covered: usize,
    pub coverage_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoverageFileReport {
    pub path: String,
    pub functions_total: usize,
    pub functions_covered: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoverageReport {
    pub phase: String,
    pub schema_version: String,
    pub input: String,
    pub summary: CoverageSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<CoverageCheckResult>,
    pub files: Vec<CoverageFileReport>,
}

pub fn build_report(input: &Path, diagnostics: &[Diagnostic]) -> anyhow::Result<CoverageReport> {
    let mut files = collect_aic_files(input)?;
    files.sort();

    let mut file_reports = Vec::with_capacity(files.len());
    let mut files_covered = 0usize;
    let mut functions_total = 0usize;
    let mut functions_covered = 0usize;

    for file in files {
        let function_total = function_count(&file)?;
        let error_count = error_count_for_file(&file, diagnostics);
        let function_covered = if error_count == 0 { function_total } else { 0 };
        if error_count == 0 {
            files_covered += 1;
        }
        functions_total += function_total;
        functions_covered += function_covered;

        file_reports.push(CoverageFileReport {
            path: render_path(&file),
            functions_total: function_total,
            functions_covered: function_covered,
            error_count,
        });
    }

    let files_total = file_reports.len();
    let coverage_pct = if functions_total > 0 {
        percentage(functions_covered, functions_total)
    } else if files_total > 0 {
        percentage(files_covered, files_total)
    } else {
        100.0
    };

    Ok(CoverageReport {
        phase: "coverage".to_string(),
        schema_version: COVERAGE_REPORT_SCHEMA_VERSION.to_string(),
        input: render_path(input),
        summary: CoverageSummary {
            files_total,
            files_covered,
            functions_total,
            functions_covered,
            coverage_pct,
        },
        check: None,
        files: file_reports,
    })
}

pub fn apply_threshold(report: &mut CoverageReport, min_pct: f64) {
    let min_pct = round_two(min_pct);
    report.check = Some(CoverageCheckResult {
        min_pct,
        passed: report.summary.coverage_pct + f64::EPSILON >= min_pct,
    });
}

pub fn write_report(path: &Path, report: &CoverageReport) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let encoded = serde_json::to_string_pretty(report)?;
    fs::write(path, encoded)?;
    Ok(())
}

fn collect_aic_files(input: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if input.is_file() {
        if is_aic_file(input) {
            files.push(input.to_path_buf());
        }
        return Ok(files);
    }
    if input.is_dir() {
        collect_aic_files_recursive(input, &mut files)?;
    }
    Ok(files)
}

fn collect_aic_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by(|a, b| a.path().cmp(&b.path()));

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            collect_aic_files_recursive(&path, out)?;
        } else if file_type.is_file() && is_aic_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|part| part.to_str()) else {
        return false;
    };
    matches!(name, ".git" | "target" | ".aic-cache")
}

fn is_aic_file(path: &Path) -> bool {
    path.extension().and_then(|part| part.to_str()) == Some("aic")
}

fn function_count(path: &Path) -> anyhow::Result<usize> {
    let source = fs::read_to_string(path)?;
    let (program, _) = parser::parse(&source, &path.to_string_lossy());
    let count = program
        .map(|program| {
            program
                .items
                .iter()
                .filter(|item| matches!(item, Item::Function(_)))
                .count()
        })
        .unwrap_or(0);
    Ok(count)
}

fn error_count_for_file(path: &Path, diagnostics: &[Diagnostic]) -> usize {
    let file_display = path.to_string_lossy().to_string();
    let file_canonical = fs::canonicalize(path).ok();
    diagnostics
        .iter()
        .filter(|diag| matches!(diag.severity, Severity::Error))
        .filter(|diag| {
            diag.spans.iter().any(|span| {
                span_matches_file(&span.file, path, &file_display, file_canonical.as_deref())
            })
        })
        .count()
}

fn span_matches_file(
    span_file: &str,
    file_path: &Path,
    file_display: &str,
    file_canonical: Option<&Path>,
) -> bool {
    if span_file == file_display || Path::new(span_file) == file_path {
        return true;
    }
    if let Some(file_canonical) = file_canonical {
        if let Some(span_canonical) = canonicalize_span_file(span_file) {
            return span_canonical == file_canonical;
        }
    }
    false
}

fn canonicalize_span_file(span_file: &str) -> Option<PathBuf> {
    let span_path = Path::new(span_file);
    if span_path.exists() {
        return fs::canonicalize(span_path).ok();
    }
    if span_path.is_absolute() {
        return None;
    }
    let cwd = std::env::current_dir().ok()?;
    let joined = cwd.join(span_path);
    if joined.exists() {
        fs::canonicalize(joined).ok()
    } else {
        None
    }
}

fn render_path(path: &Path) -> String {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(relative) = canonical.strip_prefix(&cwd) {
            return relative.to_string_lossy().replace('\\', "/");
        }
    }
    canonical.to_string_lossy().replace('\\', "/")
}

fn percentage(covered: usize, total: usize) -> f64 {
    if total == 0 {
        return 100.0;
    }
    round_two(covered as f64 * 100.0 / total as f64)
}

fn round_two(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
