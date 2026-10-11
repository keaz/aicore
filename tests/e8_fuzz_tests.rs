use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use aicore::fuzzing::{
    load_corpus, release_gate_ok, replay_regressions, run_seeded_fuzz, write_crash_repro_artifacts,
    FuzzConfig, FuzzTarget,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn corpus_root() -> PathBuf {
    repo_root().join("tests/fuzz/corpus")
}

fn regressions_root() -> PathBuf {
    repo_root().join("tests/fuzz/regressions")
}

fn collect_aic_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_aic_files(&path, out);
        } else if path.extension().map(|ext| ext == "aic").unwrap_or(false) {
            out.push(path);
        }
    }
}

/// Members that previously sent trait/impl recovery into an infinite loop.
const RECOVERY_SNIPPETS: &[&str] = &[
    "\ntype Output;\n",
    "\nconst MAX: Int;\n",
    "\nfn extra(self: Int) -> Int { 0 }\n",
    "\ntrait Dangling[T] { type X; fn f(self: T) -> Int { 1 }\n",
    "\nimpl Dangling { const C: Int = 1; type Y = Int;\n",
    "}",
    "{",
];

#[test]
fn parser_terminates_within_step_budget_on_repo_sources_and_recovery_mutations() {
    let mut files = Vec::new();
    for dir in ["examples", "std", "tests/fuzz", "compiler/aic/libs"] {
        collect_aic_files(&repo_root().join(dir), &mut files);
    }
    files.sort();
    assert!(
        files.len() > 100,
        "expected repo .aic sources, got {}",
        files.len()
    );

    let mut inputs = Vec::new();
    for file in &files {
        let source = fs::read_to_string(file).expect("read .aic source");
        let label = file
            .strip_prefix(repo_root())
            .unwrap_or(file)
            .display()
            .to_string();
        if source.len() <= 16 * 1024 {
            let mut boundaries: Vec<usize> = source
                .char_indices()
                .filter(|(_, ch)| ch.is_whitespace())
                .map(|(idx, _)| idx)
                .collect();
            boundaries.push(source.len());
            for (n, snippet) in RECOVERY_SNIPPETS.iter().enumerate() {
                let at = boundaries[(n * 7919 + source.len()) % boundaries.len()];
                let mut mutated = source.clone();
                mutated.insert_str(at, snippet);
                inputs.push((format!("{label} + snippet {n} at byte {at}"), mutated));
            }
            let third = boundaries[boundaries.len() / 3];
            inputs.push((
                format!("{label} truncated at byte {third}"),
                source[..third].to_string(),
            ));
        }
        inputs.push((label, source));
    }

    // Parse on a worker thread so a regression that loops without inspecting
    // tokens still fails the test instead of hanging CI.
    let (tx, rx) = mpsc::channel();
    let worker_inputs = inputs.clone();
    thread::Builder::new()
        .name("parser-termination".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            for (idx, (_, source)) in worker_inputs.iter().enumerate() {
                let (_program, diagnostics, stats) =
                    aicore::parser::parse_with_stats(source, "termination.aic");
                let hit_e1099 = diagnostics.iter().any(|d| d.code == "E1099");
                if tx.send((idx, stats, hit_e1099)).is_err() {
                    return;
                }
            }
        })
        .expect("spawn parser worker");

    for (expected_idx, (label, _)) in inputs.iter().enumerate() {
        let (idx, stats, hit_e1099) = rx
            .recv_timeout(Duration::from_secs(30))
            .unwrap_or_else(|_| panic!("parser did not terminate within 30s on {label}"));
        assert_eq!(idx, expected_idx);
        assert!(
            !stats.budget_exhausted && !hit_e1099,
            "parser exhausted its step budget on {label}: {stats:?}"
        );
        assert!(
            stats.steps <= stats.tokens * 32 + 512,
            "parser work is not token-proportional on {label}: {stats:?}"
        );
    }
}

#[test]
fn fuzz_seeded_corpus_has_no_panics() {
    let config = FuzzConfig {
        iterations: 300,
        max_len: 768,
        seed: 0xE8F0_00D1,
    };

    for target in [
        FuzzTarget::Lexer,
        FuzzTarget::Parser,
        FuzzTarget::Typecheck,
        FuzzTarget::Formatter,
    ] {
        let corpus = load_corpus(&corpus_root(), target).expect("load corpus");
        assert!(!corpus.is_empty(), "empty corpus for {target:?}");

        let report = run_seeded_fuzz(target, &corpus, config);
        assert!(
            report.crashes.is_empty(),
            "fuzz crashes for {target:?}: {:#?}",
            report.crashes
        );
        assert_eq!(
            report.total_crashes, 0,
            "unexpected raw crashes: {report:#?}"
        );
        assert!(
            report.triage.is_empty(),
            "unexpected triage entries for {target:?}: {:#?}",
            report.triage
        );
    }
}

#[test]
fn fuzz_regressions_replay_without_panics() {
    for target in [
        FuzzTarget::Lexer,
        FuzzTarget::Parser,
        FuzzTarget::Typecheck,
        FuzzTarget::Formatter,
    ] {
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
    for target in [
        FuzzTarget::Lexer,
        FuzzTarget::Parser,
        FuzzTarget::Typecheck,
        FuzzTarget::Formatter,
    ] {
        let corpus = load_corpus(&corpus_root(), target).expect("load corpus");
        let report = run_seeded_fuzz(target, &corpus, config);
        assert!(
            report.crashes.is_empty(),
            "fuzz crashes for {target:?}: {:#?}",
            report.crashes
        );
        reports.push(report);
    }
    assert!(
        release_gate_ok(&reports),
        "nightly fuzz release gate failed: {:#?}",
        reports
    );

    let out_dir = repo_root().join("target/e8");
    fs::create_dir_all(&out_dir).expect("mkdir target/e8");
    let crash_root = out_dir.join("fuzz-crashers");
    for report in &reports {
        let artifacts =
            write_crash_repro_artifacts(report, &crash_root).expect("write crash repro artifacts");
        assert_eq!(
            artifacts.len(),
            report.triage.len(),
            "artifact count mismatch for {:#?}",
            report
        );
    }
    fs::write(
        out_dir.join("nightly-fuzz-report.json"),
        serde_json::to_string_pretty(&reports).expect("json"),
    )
    .expect("write report");
}
