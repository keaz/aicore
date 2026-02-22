use std::fs;
use std::path::PathBuf;

use aicore::perf_gate::{
    baseline_for_target, build_trend_report, host_target_label, load_budget, load_target_baselines,
    run_perf_gate, write_report, write_trend_report,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn perf_budget_gate_passes_for_reference_dataset() {
    let root = repo_root();
    let budget =
        load_budget(&root.join("benchmarks/service_baseline/budget.v1.json")).expect("load budget");
    let target_baselines =
        load_target_baselines(&root.join("benchmarks/service_baseline/baselines.v1.json"))
            .expect("load target baselines");
    let target = host_target_label();
    let baseline = baseline_for_target(&target_baselines, target)
        .unwrap_or_else(|| panic!("missing baseline for target `{target}`"));

    let report = run_perf_gate(&root, &budget, Some(&baseline)).expect("run perf gate");
    write_report(&root.join("target/e8/perf-report.json"), &report).expect("write perf report");
    write_report(
        &root.join(format!("target/e8/perf-report-{target}.json")),
        &report,
    )
    .expect("write target report");

    let trend = build_trend_report(&report, target).expect("build trend report");
    write_trend_report(
        &root.join(format!("target/e8/perf-trend-{target}.json")),
        &trend,
    )
    .expect("write trend report");

    assert!(
        report.violations.is_empty(),
        "perf budget violations: {:#?}",
        report.violations
    );
}

#[test]
fn perf_dataset_fingerprint_matches_checked_in_reference() {
    let root = repo_root();
    let budget =
        load_budget(&root.join("benchmarks/service_baseline/budget.v1.json")).expect("load budget");
    let report = run_perf_gate(&root, &budget, None).expect("run perf gate");

    let expected =
        fs::read_to_string(root.join("benchmarks/service_baseline/dataset-fingerprint.txt"))
            .expect("read fingerprint")
            .trim()
            .to_string();

    assert_eq!(report.metrics.dataset_fingerprint, expected);
}

#[test]
fn perf_target_baselines_include_host_target() {
    let root = repo_root();
    let baselines =
        load_target_baselines(&root.join("benchmarks/service_baseline/baselines.v1.json"))
            .expect("load target baselines");
    let target = host_target_label();
    assert!(
        baseline_for_target(&baselines, target).is_some(),
        "missing host target baseline: {target}"
    );
}
