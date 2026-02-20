use std::fs;
use std::path::PathBuf;

use aicore::perf_gate::{load_baseline, load_budget, run_perf_gate, write_report};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn perf_budget_gate_passes_for_reference_dataset() {
    let root = repo_root();
    let budget = load_budget(&root.join("docs/perf-budget.json")).expect("load budget");
    let baseline = load_baseline(&root.join("docs/perf-baseline.json")).expect("load baseline");

    let report = run_perf_gate(&root, &budget, Some(&baseline)).expect("run perf gate");
    let output = root.join("target/e8/perf-report.json");
    write_report(&output, &report).expect("write perf report");

    assert!(
        report.violations.is_empty(),
        "perf budget violations: {:#?}",
        report.violations
    );
}

#[test]
fn perf_dataset_fingerprint_matches_checked_in_reference() {
    let root = repo_root();
    let budget = load_budget(&root.join("docs/perf-budget.json")).expect("load budget");
    let report = run_perf_gate(&root, &budget, None).expect("run perf gate");

    let expected = fs::read_to_string(root.join("docs/perf-dataset-fingerprint.txt"))
        .expect("read fingerprint")
        .trim()
        .to_string();

    assert_eq!(report.metrics.dataset_fingerprint, expected);
}
