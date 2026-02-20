use std::fs;
use std::path::PathBuf;

use aicore::fuzzing::{load_corpus, replay_regressions, run_seeded_fuzz, FuzzConfig, FuzzTarget};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn corpus_root() -> PathBuf {
    repo_root().join("tests/fuzz/corpus")
}

fn regressions_root() -> PathBuf {
    repo_root().join("tests/fuzz/regressions")
}

#[test]
fn fuzz_seeded_corpus_has_no_panics() {
    let config = FuzzConfig {
        iterations: 300,
        max_len: 768,
        seed: 0xE8F0_00D1,
    };

    for target in [FuzzTarget::Lexer, FuzzTarget::Parser, FuzzTarget::Typecheck] {
        let corpus = load_corpus(&corpus_root(), target).expect("load corpus");
        assert!(!corpus.is_empty(), "empty corpus for {target:?}");

        let report = run_seeded_fuzz(target, &corpus, config);
        assert!(
            report.crashes.is_empty(),
            "fuzz crashes for {target:?}: {:#?}",
            report.crashes
        );
    }
}

#[test]
fn fuzz_regressions_replay_without_panics() {
    for target in [FuzzTarget::Lexer, FuzzTarget::Parser, FuzzTarget::Typecheck] {
        let replay = replay_regressions(&regressions_root(), target).expect("replay");
        assert!(!replay.is_empty(), "no regression fixtures for {target:?}");
        for case in replay {
            assert!(case.passed, "regression replay failed: {case:#?}");
        }
    }
}

#[test]
#[ignore = "nightly fuzz stress gate"]
fn fuzz_nightly_stress_suite() {
    let config = FuzzConfig {
        iterations: 2_000,
        max_len: 1_024,
        seed: 0xE8_5EED,
    };

    let mut reports = Vec::new();
    for target in [FuzzTarget::Lexer, FuzzTarget::Parser, FuzzTarget::Typecheck] {
        let corpus = load_corpus(&corpus_root(), target).expect("load corpus");
        let report = run_seeded_fuzz(target, &corpus, config);
        assert!(
            report.crashes.is_empty(),
            "fuzz crashes for {target:?}: {:#?}",
            report.crashes
        );
        reports.push(report);
    }

    let out_dir = repo_root().join("target/e8");
    fs::create_dir_all(&out_dir).expect("mkdir target/e8");
    fs::write(
        out_dir.join("nightly-fuzz-report.json"),
        serde_json::to_string_pretty(&reports).expect("json"),
    )
    .expect("write report");
}
