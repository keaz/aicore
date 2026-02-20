use std::path::PathBuf;

use aicore::conformance::{load_catalog, run_catalog};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn conformance_pack_passes_all_cases() {
    let root = repo_root().join("examples/e8/conformance_pack");
    let catalog = load_catalog(&root.join("catalog.json")).expect("load catalog");

    let report = run_catalog(&root, &catalog).expect("run conformance");
    assert_eq!(report.total, 4, "report={report:#?}");
    assert_eq!(report.failed, 0, "report={report:#?}");
}

#[test]
fn conformance_report_is_deterministic_across_runs() {
    let root = repo_root().join("examples/e8/conformance_pack");
    let catalog = load_catalog(&root.join("catalog.json")).expect("load catalog");

    let first = run_catalog(&root, &catalog).expect("run first");
    let second = run_catalog(&root, &catalog).expect("run second");

    let a = serde_json::to_value(&first).expect("serialize first");
    let b = serde_json::to_value(&second).expect("serialize second");
    assert_eq!(a, b);
}
