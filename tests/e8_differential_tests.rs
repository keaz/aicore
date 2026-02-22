use std::path::PathBuf;

use aicore::differential::{run_randomized_roundtrip, run_roundtrip_corpus, run_roundtrip_file};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn differential_roundtrip_matches_for_e8_corpus() {
    let root = repo_root().join("examples/e8");
    let report = run_roundtrip_corpus(&root).expect("run differential corpus");
    assert!(report.total >= 6, "report={report:#?}");
    assert_eq!(report.diverged, 0, "report={report:#?}");
}

#[test]
fn differential_roundtrip_matches_reference_seed_file() {
    let file = repo_root().join("examples/e8/roundtrip_random_seed.aic");
    let result = run_roundtrip_file(&file).expect("run differential file");
    assert!(result.passed, "result={result:#?}");
}

#[test]
fn differential_randomized_suite_matches_reference_path() {
    let report = run_randomized_roundtrip(0xD1FF_0093, 64).expect("run randomized differential");
    assert_eq!(report.total, 64, "report={report:#?}");
    assert_eq!(report.diverged, 0, "report={report:#?}");
}

#[test]
fn differential_tests_directory_corpus_is_stable() {
    let root = repo_root().join("tests/differential");
    let report = run_roundtrip_corpus(&root).expect("run differential tests corpus");
    assert!(report.total >= 1, "report={report:#?}");
    assert_eq!(report.diverged, 0, "report={report:#?}");
}
