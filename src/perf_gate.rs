use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::codegen::{emit_llvm_with_resolution_and_options, CodegenOptions};
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
pub struct PerfTargetBaselines {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub dataset: String,
    pub targets: BTreeMap<String, PerfBaseline>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfTrendMetric {
    pub observed_ms: f64,
    pub baseline_ms: f64,
    pub delta_pct: f64,
    pub budget_max_ms: f64,
    pub regression_limit_ms: f64,
    pub within_budget: bool,
    pub within_regression_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfTrendReport {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub dataset: String,
    pub target: String,
    pub regression_tolerance_pct: f64,
    pub dataset_fingerprint: String,
    pub parser: PerfTrendMetric,
    pub typecheck: PerfTrendMetric,
    pub codegen: PerfTrendMetric,
}

pub fn load_budget(path: &Path) -> anyhow::Result<PerfBudget> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<PerfBudget>(&raw)?)
}

pub fn load_baseline(path: &Path) -> anyhow::Result<PerfBaseline> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<PerfBaseline>(&raw)?)
}

pub fn baseline_from_report(report: &PerfReport) -> PerfBaseline {
    PerfBaseline {
        dataset: report.dataset.clone(),
        parser_ms: report.metrics.parser_ms,
        typecheck_ms: report.metrics.typecheck_ms,
        codegen_ms: report.metrics.codegen_ms,
    }
}

pub fn load_compare_baseline(path: &Path) -> anyhow::Result<PerfBaseline> {
    let raw = fs::read_to_string(path)?;

    if let Ok(baseline) = serde_json::from_str::<PerfBaseline>(&raw) {
        return Ok(baseline);
    }

    if let Ok(report) = serde_json::from_str::<PerfReport>(&raw) {
        return Ok(baseline_from_report(&report));
    }

    if let Ok(manifest) = serde_json::from_str::<PerfTargetBaselines>(&raw) {
        let target = host_target_label();
        return baseline_for_target(&manifest, target)
            .ok_or_else(|| anyhow::anyhow!("missing baseline for target `{target}`"));
    }

    let value = serde_json::from_str::<serde_json::Value>(&raw)?;
    if let Some(report_value) = value.get("report") {
        let report = serde_json::from_value::<PerfReport>(report_value.clone())?;
        return Ok(baseline_from_report(&report));
    }

    anyhow::bail!(
        "unsupported compare baseline format in {} (expected PerfBaseline, PerfReport, target baseline manifest, or bench envelope with `report`)",
        path.display()
    );
}

pub fn load_target_baselines(path: &Path) -> anyhow::Result<PerfTargetBaselines> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<PerfTargetBaselines>(&raw)?)
}

pub fn host_target_label() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

pub fn baseline_for_target(manifest: &PerfTargetBaselines, target: &str) -> Option<PerfBaseline> {
    manifest.targets.get(target).cloned()
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

pub fn build_trend_report(report: &PerfReport, target: &str) -> Option<PerfTrendReport> {
    let baseline = report.baseline.as_ref()?;
    let tolerance = 1.0 + (report.budget.regression_tolerance_pct / 100.0);

    Some(PerfTrendReport {
        schema_version: default_schema_version(),
        dataset: report.dataset.clone(),
        target: target.to_string(),
        regression_tolerance_pct: report.budget.regression_tolerance_pct,
        dataset_fingerprint: report.metrics.dataset_fingerprint.clone(),
        parser: build_trend_metric(
            report.metrics.parser_ms,
            baseline.parser_ms,
            report.budget.parser_ms_max,
            tolerance,
        ),
        typecheck: build_trend_metric(
            report.metrics.typecheck_ms,
            baseline.typecheck_ms,
            report.budget.typecheck_ms_max,
            tolerance,
        ),
        codegen: build_trend_metric(
            report.metrics.codegen_ms,
            baseline.codegen_ms,
            report.budget.codegen_ms_max,
            tolerance,
        ),
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
            let _ = emit_llvm_with_resolution_and_options(
                &lowered,
                Some(&resolution),
                &path.to_string_lossy(),
                CodegenOptions::default(),
            )
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

pub fn write_trend_report(path: &Path, report: &PerfTrendReport) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(report)?;
    fs::write(path, json)?;
    Ok(())
}

fn build_trend_metric(
    observed_ms: f64,
    baseline_ms: f64,
    budget_max_ms: f64,
    tolerance: f64,
) -> PerfTrendMetric {
    let regression_limit_ms = round_ms(baseline_ms * tolerance);
    let delta_pct = if baseline_ms.abs() < f64::EPSILON {
        0.0
    } else {
        round_ms(((observed_ms - baseline_ms) / baseline_ms) * 100.0)
    };
    let within_budget = observed_ms <= budget_max_ms;
    let within_regression_limit = observed_ms <= regression_limit_ms;

    PerfTrendMetric {
        observed_ms,
        baseline_ms,
        delta_pct,
        budget_max_ms,
        regression_limit_ms,
        within_budget,
        within_regression_limit,
    }
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

fn default_schema_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        baseline_from_report, build_trend_report, check_budget_max, check_regression,
        dataset_fingerprint, PerfBaseline, PerfBudget, PerfMetrics, PerfReport,
    };

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

    #[test]
    fn trend_report_is_deterministic_for_fixed_inputs() {
        let report = PerfReport {
            dataset: "examples/e8/large_project_bench".to_string(),
            metrics: PerfMetrics {
                parser_ms: 50.0,
                typecheck_ms: 60.0,
                codegen_ms: 70.0,
                file_count: 3,
                total_bytes: 1234,
                dataset_fingerprint: "abc123".to_string(),
            },
            budget: PerfBudget {
                dataset: "examples/e8/large_project_bench".to_string(),
                iterations: 3,
                parser_ms_max: 120.0,
                typecheck_ms_max: 120.0,
                codegen_ms_max: 120.0,
                regression_tolerance_pct: 10.0,
            },
            baseline: Some(PerfBaseline {
                dataset: "examples/e8/large_project_bench".to_string(),
                parser_ms: 40.0,
                typecheck_ms: 50.0,
                codegen_ms: 60.0,
            }),
            violations: Vec::new(),
        };

        let a = build_trend_report(&report, "linux").expect("build trend report");
        let b = build_trend_report(&report, "linux").expect("build trend report");

        assert_eq!(a, b);
        assert_eq!(a.parser.delta_pct, 25.0);
        assert_eq!(a.typecheck.delta_pct, 20.0);
        assert_eq!(a.codegen.delta_pct, 16.667);
        assert!(!a.codegen.within_regression_limit);
    }

    #[test]
    fn baseline_from_report_copies_dataset_and_observed_metrics() {
        let report = PerfReport {
            dataset: "examples/e8/large_project_bench".to_string(),
            metrics: PerfMetrics {
                parser_ms: 12.5,
                typecheck_ms: 22.5,
                codegen_ms: 32.5,
                file_count: 2,
                total_bytes: 900,
                dataset_fingerprint: "fingerprint".to_string(),
            },
            budget: PerfBudget {
                dataset: "examples/e8/large_project_bench".to_string(),
                iterations: 1,
                parser_ms_max: 100.0,
                typecheck_ms_max: 100.0,
                codegen_ms_max: 100.0,
                regression_tolerance_pct: 10.0,
            },
            baseline: None,
            violations: Vec::new(),
        };

        let baseline = baseline_from_report(&report);
        assert_eq!(baseline.dataset, report.dataset);
        assert_eq!(baseline.parser_ms, report.metrics.parser_ms);
        assert_eq!(baseline.typecheck_ms, report.metrics.typecheck_ms);
        assert_eq!(baseline.codegen_ms, report.metrics.codegen_ms);
    }
}
