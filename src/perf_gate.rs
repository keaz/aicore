use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::codegen::emit_llvm;
use crate::contracts::lower_runtime_asserts;
use crate::driver::has_errors;
use crate::ir_builder;
use crate::parser;
use crate::resolver;
use crate::typecheck;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfBudget {
    pub dataset: String,
    #[serde(default = "default_iterations")]
    pub iterations: usize,
    pub parser_ms_max: f64,
    pub typecheck_ms_max: f64,
    pub codegen_ms_max: f64,
    #[serde(default = "default_regression_tolerance_pct")]
    pub regression_tolerance_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfBaseline {
    pub dataset: String,
    pub parser_ms: f64,
    pub typecheck_ms: f64,
    pub codegen_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfMetrics {
    pub parser_ms: f64,
    pub typecheck_ms: f64,
    pub codegen_ms: f64,
    pub file_count: usize,
    pub total_bytes: usize,
    pub dataset_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfReport {
    pub dataset: String,
    pub metrics: PerfMetrics,
    pub budget: PerfBudget,
    pub baseline: Option<PerfBaseline>,
    pub violations: Vec<String>,
}

pub fn load_budget(path: &Path) -> anyhow::Result<PerfBudget> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<PerfBudget>(&raw)?)
}

pub fn load_baseline(path: &Path) -> anyhow::Result<PerfBaseline> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<PerfBaseline>(&raw)?)
}

pub fn run_perf_gate(
    repo_root: &Path,
    budget: &PerfBudget,
    baseline: Option<&PerfBaseline>,
) -> anyhow::Result<PerfReport> {
    let dataset_path = resolve_path(repo_root, &budget.dataset);
    let metrics = benchmark_dataset(&dataset_path, budget.iterations)?;

    let mut violations = Vec::new();
    check_budget_max(
        "parser_ms",
        metrics.parser_ms,
        budget.parser_ms_max,
        &mut violations,
    );
    check_budget_max(
        "typecheck_ms",
        metrics.typecheck_ms,
        budget.typecheck_ms_max,
        &mut violations,
    );
    check_budget_max(
        "codegen_ms",
        metrics.codegen_ms,
        budget.codegen_ms_max,
        &mut violations,
    );

    if let Some(base) = baseline {
        if base.dataset != budget.dataset {
            violations.push(format!(
                "baseline dataset mismatch: baseline=`{}` budget=`{}`",
                base.dataset, budget.dataset
            ));
        }
        let tolerance = 1.0 + (budget.regression_tolerance_pct / 100.0);
        check_regression(
            "parser_ms",
            metrics.parser_ms,
            base.parser_ms * tolerance,
            &mut violations,
        );
        check_regression(
            "typecheck_ms",
            metrics.typecheck_ms,
            base.typecheck_ms * tolerance,
            &mut violations,
        );
        check_regression(
            "codegen_ms",
            metrics.codegen_ms,
            base.codegen_ms * tolerance,
            &mut violations,
        );
    }

    Ok(PerfReport {
        dataset: budget.dataset.clone(),
        metrics,
        budget: budget.clone(),
        baseline: baseline.cloned(),
        violations,
    })
}

pub fn benchmark_dataset(dataset_root: &Path, iterations: usize) -> anyhow::Result<PerfMetrics> {
    let mut files = collect_dataset_files(dataset_root)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if files.is_empty() {
        anyhow::bail!("benchmark dataset is empty: {}", dataset_root.display());
    }

    let mut parser_ns = 0f64;
    let mut typecheck_ns = 0f64;
    let mut codegen_ns = 0f64;
    let loops = iterations.max(1);

    for _ in 0..loops {
        let start = Instant::now();
        for (path, source) in &files {
            let (_program, _diags) = parser::parse(source, &path.to_string_lossy());
        }
        parser_ns += start.elapsed().as_nanos() as f64;

        let start = Instant::now();
        for (path, source) in &files {
            let (program, _parse_diags) = parser::parse(source, &path.to_string_lossy());
            if let Some(program) = program {
                let ir = ir_builder::build(&program);
                let (resolution, _resolve_diags) = resolver::resolve(&ir, &path.to_string_lossy());
                let _ = typecheck::check(&ir, &resolution, &path.to_string_lossy());
            }
        }
        typecheck_ns += start.elapsed().as_nanos() as f64;

        let start = Instant::now();
        for (path, source) in &files {
            let (program, parse_diags) = parser::parse(source, &path.to_string_lossy());
            if parse_diags.iter().any(|d| d.is_error()) {
                anyhow::bail!("benchmark dataset parse failure in {}", path.display());
            }
            let Some(program) = program else {
                anyhow::bail!("benchmark parse produced no AST in {}", path.display());
            };
            let ir = ir_builder::build(&program);
            let (resolution, resolve_diags) = resolver::resolve(&ir, &path.to_string_lossy());
            if resolve_diags.iter().any(|d| d.is_error()) {
                anyhow::bail!("benchmark resolve failure in {}", path.display());
            }
            let typecheck_out = typecheck::check(&ir, &resolution, &path.to_string_lossy());
            if has_errors(&typecheck_out.diagnostics) {
                anyhow::bail!("benchmark typecheck failure in {}", path.display());
            }
            let lowered = lower_runtime_asserts(&ir);
            let _ = emit_llvm(&lowered, &path.to_string_lossy())
                .map_err(|diags| anyhow::anyhow!("benchmark codegen failure: {diags:#?}"))?;
        }
        codegen_ns += start.elapsed().as_nanos() as f64;
    }

    let fingerprint = dataset_fingerprint(dataset_root, &files);
    let total_bytes = files.iter().map(|(_, source)| source.len()).sum::<usize>();

    Ok(PerfMetrics {
        parser_ms: round_ms((parser_ns / loops as f64) / 1_000_000.0),
        typecheck_ms: round_ms((typecheck_ns / loops as f64) / 1_000_000.0),
        codegen_ms: round_ms((codegen_ns / loops as f64) / 1_000_000.0),
        file_count: files.len(),
        total_bytes,
        dataset_fingerprint: fingerprint,
    })
}

pub fn write_report(path: &Path, report: &PerfReport) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(report)?;
    fs::write(path, json)?;
    Ok(())
}

fn check_budget_max(metric: &str, observed: f64, max_allowed: f64, violations: &mut Vec<String>) {
    if observed > max_allowed {
        violations.push(format!(
            "budget exceeded: {} observed {:.3} > max {:.3}",
            metric, observed, max_allowed
        ));
    }
}

fn check_regression(metric: &str, observed: f64, max_allowed: f64, violations: &mut Vec<String>) {
    if observed > max_allowed {
        violations.push(format!(
            "regression exceeded: {} observed {:.3} > baseline-adjusted {:.3}",
            metric, observed, max_allowed
        ));
    }
}

fn dataset_fingerprint(dataset_root: &Path, files: &[(PathBuf, String)]) -> String {
    let mut hasher = Sha256::new();
    for (path, source) in files {
        let relative = path.strip_prefix(dataset_root).unwrap_or(path.as_path());
        let stable_key = stable_path_key(relative);
        hasher.update(stable_key.as_bytes());
        hasher.update([0]);
        hasher.update(source.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn stable_path_key(path: &Path) -> String {
    let mut key = String::new();
    for component in path.components() {
        let part = match component {
            Component::RootDir => continue,
            Component::Prefix(prefix) => prefix.as_os_str().to_string_lossy().into_owned(),
            Component::CurDir => ".".to_string(),
            Component::ParentDir => "..".to_string(),
            Component::Normal(segment) => segment.to_string_lossy().into_owned(),
        };

        if !key.is_empty() {
            key.push('/');
        }
        key.push_str(&part);
    }
    key
}

fn collect_dataset_files(root: &Path) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }

    let mut entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_dataset_files(&path)?);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) == Some("aic") {
            let source = fs::read_to_string(&path)?;
            out.push((path, source));
        }
    }
    Ok(out)
}

fn resolve_path(root: &Path, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    }
}

fn round_ms(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn default_iterations() -> usize {
    3
}

fn default_regression_tolerance_pct() -> f64 {
    25.0
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{check_budget_max, check_regression, dataset_fingerprint};

    #[test]
    fn reports_budget_violation_when_metric_exceeds_cap() {
        let mut violations = Vec::new();
        check_budget_max("parser_ms", 11.0, 10.0, &mut violations);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn reports_regression_violation_against_baseline_limit() {
        let mut violations = Vec::new();
        check_regression("typecheck_ms", 21.0, 20.0, &mut violations);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn dataset_fingerprint_is_root_independent() {
        let root_a = PathBuf::from("/tmp/bench-a");
        let root_b = PathBuf::from("/tmp/bench-b");

        let files_a = vec![(
            root_a.join("pkg/main.aic"),
            "fn main() -> Int { 0 }\n".to_string(),
        )];
        let files_b = vec![(
            root_b.join("pkg/main.aic"),
            "fn main() -> Int { 0 }\n".to_string(),
        )];

        let hash_a = dataset_fingerprint(&root_a, &files_a);
        let hash_b = dataset_fingerprint(&root_b, &files_b);
        assert_eq!(hash_a, hash_b);
    }
}
