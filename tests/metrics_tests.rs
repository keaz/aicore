use std::fs;

use aicore::metrics::{
    apply_thresholds, build_report, resolve_thresholds, MetricsThresholdOverrides,
    MetricsThresholds, DEFAULT_MAX_CYCLOMATIC,
};
use tempfile::tempdir;

fn write_metrics_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("metrics_fixture.aic");
    fs::write(
        &source,
        concat!(
            "module metrics.fixture;\n",
            "fn beta(a: Int, b: Int) -> Int effects { io } {\n",
            "    if a > 0 && b > 0 {\n",
            "        if a > b { a } else { b }\n",
            "    } else {\n",
            "        0\n",
            "    }\n",
            "}\n",
            "fn alpha(v: Int) -> Int {\n",
            "    if v > 0 { v } else { 0 }\n",
            "}\n",
        ),
    )
    .expect("write metrics fixture");
    (dir, source)
}

#[test]
fn build_report_computes_expected_per_function_metrics() {
    let (_dir, source) = write_metrics_fixture();
    let report = build_report(&source).expect("build report");
    assert_eq!(report.phase, "metrics");
    assert_eq!(report.schema_version, "1.0");
    assert_eq!(report.functions.len(), 2);
    assert_eq!(report.functions[0].name, "alpha");
    assert_eq!(report.functions[1].name, "beta");

    let alpha = &report.functions[0];
    assert_eq!(alpha.cyclomatic_complexity, 2);
    assert_eq!(alpha.cognitive_complexity, 1);
    assert_eq!(alpha.max_nesting_depth, 1);
    assert_eq!(alpha.params, 1);
    assert_eq!(alpha.rating, "A");

    let beta = &report.functions[1];
    assert_eq!(beta.cyclomatic_complexity, 4);
    assert_eq!(beta.cognitive_complexity, 4);
    assert_eq!(beta.max_nesting_depth, 2);
    assert_eq!(beta.params, 2);
    assert_eq!(beta.effects, vec!["io".to_string()]);
    assert_eq!(beta.rating, "B");
}

#[test]
fn threshold_check_reports_expected_violations() {
    let (_dir, source) = write_metrics_fixture();
    let mut report = build_report(&source).expect("build report");
    apply_thresholds(
        &mut report,
        MetricsThresholds {
            max_cyclomatic: Some(3),
            max_cognitive: None,
            max_lines: None,
            max_params: Some(1),
            max_nesting_depth: None,
        },
    );

    let check = report.check.expect("check result");
    assert!(!check.passed);
    assert_eq!(check.violations.len(), 2);
    assert_eq!(check.violations[0].function, "beta");
    assert_eq!(check.violations[0].metric, "cyclomatic_complexity");
    assert_eq!(check.violations[0].actual, 4);
    assert_eq!(check.violations[0].max, 3);
    assert_eq!(check.violations[1].function, "beta");
    assert_eq!(check.violations[1].metric, "params");
    assert_eq!(check.violations[1].actual, 2);
    assert_eq!(check.violations[1].max, 1);
}

#[test]
fn threshold_resolution_prefers_cli_overrides_and_applies_default_cyclomatic() {
    let resolved = resolve_thresholds(
        MetricsThresholds {
            max_cyclomatic: Some(9),
            max_cognitive: Some(22),
            max_lines: None,
            max_params: Some(4),
            max_nesting_depth: None,
        },
        MetricsThresholdOverrides {
            max_cyclomatic: Some(5),
            max_cognitive: None,
            max_lines: Some(120),
            max_params: None,
            max_nesting_depth: None,
        },
    );
    assert_eq!(resolved.max_cyclomatic, Some(5));
    assert_eq!(resolved.max_cognitive, Some(22));
    assert_eq!(resolved.max_lines, Some(120));
    assert_eq!(resolved.max_params, Some(4));

    let defaults = resolve_thresholds(
        MetricsThresholds::default(),
        MetricsThresholdOverrides::default(),
    );
    assert_eq!(defaults.max_cyclomatic, Some(DEFAULT_MAX_CYCLOMATIC));
}
