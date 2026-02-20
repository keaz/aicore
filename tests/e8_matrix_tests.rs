use std::path::PathBuf;

use aicore::execution_matrix::{load_definition, run_host_matrix};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn execution_matrix_definition_covers_host_os() {
    let root = repo_root();
    let matrix = load_definition(&root.join("examples/e8/execution-matrix.json"))
        .expect("load execution matrix");
    let host = std::env::consts::OS;
    assert!(
        matrix.targets.iter().any(|target| target.os == host),
        "host os `{host}` missing from matrix definition"
    );
}

#[test]
fn execution_matrix_host_cases_pass() {
    let root = repo_root();
    let matrix = load_definition(&root.join("examples/e8/execution-matrix.json"))
        .expect("load execution matrix");
    let report = run_host_matrix(&root, &matrix).expect("run matrix");

    assert_eq!(report.failed, 0, "report={report:#?}");
    assert!(report.total > 0, "report={report:#?}");
}
