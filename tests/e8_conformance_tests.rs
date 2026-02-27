use std::fs;
use std::path::PathBuf;
use std::process::Command;

use aicore::conformance::{load_catalog, run_catalog};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_aic(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("run aic")
}

fn has_code(output: &[u8], code: &str) -> bool {
    let diagnostics: serde_json::Value =
        serde_json::from_slice(output).expect("parse diagnostics json");
    diagnostics
        .as_array()
        .expect("diagnostics array")
        .iter()
        .any(|item| item.get("code").and_then(|value| value.as_str()) == Some(code))
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

#[test]
fn verification_quality_docs_cover_qv_gates() {
    let root = repo_root();
    let readme = fs::read_to_string(root.join("docs/verification-quality/README.md"))
        .expect("read verification-quality README");
    for token in [
        "QV-T1",
        "QV-T2",
        "QV-T3",
        "QV-T4",
        "QV-T5",
        "AGX3-T3",
        "examples/verify/qv_contract_proof_fail.aic",
        "examples/verify/qv_contract_proof_fixed.aic",
        "concurrency-stress-replay.md",
        "e8_concurrency_stress_tests",
    ] {
        assert!(
            readme.contains(token),
            "verification-quality README missing token: {token}"
        );
    }

    let contracts =
        fs::read_to_string(root.join("docs/verification-quality/contracts-proof-obligations.md"))
            .expect("read contracts runbook");
    assert!(contracts.contains("E4002"));
    assert!(contracts.contains("Theorem Subset"));

    let effects = fs::read_to_string(root.join("docs/verification-quality/effect-protocols.md"))
        .expect("read effects runbook");
    assert!(effects.contains("E2006"));
    assert!(effects.contains("E2009"));
    assert!(effects.contains("IntChannel"));
    assert!(effects.contains("FileHandle"));

    let fuzz =
        fs::read_to_string(root.join("docs/verification-quality/fuzz-differential-runbook.md"))
            .expect("read fuzz+differential runbook");
    assert!(fuzz.contains("e8_fuzz_tests"));
    assert!(fuzz.contains("e8_differential_tests"));

    let perf = fs::read_to_string(root.join("docs/verification-quality/perf-sla-playbook.md"))
        .expect("read perf runbook");
    assert!(perf.contains("budget.v1.json"));
    assert!(perf.contains("Regression Triage"));

    let concurrency =
        fs::read_to_string(root.join("docs/verification-quality/concurrency-stress-replay.md"))
            .expect("read concurrency stress runbook");
    assert!(concurrency.contains("AIC_CONC_STRESS_REPLAY"));
    assert!(concurrency.contains("concurrency-stress-report.json"));

    let incident =
        fs::read_to_string(root.join("docs/verification-quality/incident-reproduction.md"))
            .expect("read incident runbook");
    assert!(incident.contains("qv_contract_proof_fail"));
    assert!(incident.contains("make test-e8"));
    assert!(incident.contains("concurrency-stress-replay.txt"));
}

#[test]
fn verification_quality_workflows_are_release_blocking() {
    let root = repo_root();

    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci workflow");
    for token in [
        "E8 verification gates",
        "make test-e8",
        "Upload E8 concurrency stress artifacts",
        "e8-concurrency-stress-linux",
        "Upload E8 perf report",
        "e8-perf-report-linux",
    ] {
        assert!(ci.contains(token), "ci workflow missing token: {token}");
    }

    let release = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect("read release workflow");
    for token in ["release-preflight", "make ci", "release-build"] {
        assert!(
            release.contains(token),
            "release workflow missing token: {token}"
        );
    }

    let nightly = fs::read_to_string(root.join(".github/workflows/nightly-fuzz.yml"))
        .expect("read nightly fuzz workflow");
    for token in [
        "fuzz-nightly",
        "make test-e8-nightly-fuzz",
        "nightly-fuzz-report",
    ] {
        assert!(
            nightly.contains(token),
            "nightly fuzz workflow missing token: {token}"
        );
    }
}

#[test]
fn verification_quality_examples_report_expected_statuses() {
    let contract_fail = run_aic(&[
        "check",
        "examples/verify/qv_contract_proof_fail.aic",
        "--json",
    ]);
    assert_eq!(contract_fail.status.code(), Some(1));
    assert!(
        has_code(&contract_fail.stdout, "E4002"),
        "expected E4002 from contract failure example"
    );

    let contract_fixed = run_aic(&[
        "check",
        "examples/verify/qv_contract_proof_fixed.aic",
        "--json",
    ]);
    assert_eq!(contract_fixed.status.code(), Some(0));
    assert!(
        !has_code(&contract_fixed.stdout, "E4002"),
        "fixed contract example should not emit E4002"
    );

    let protocol_fail = run_aic(&[
        "check",
        "examples/verify/file_protocol_invalid.aic",
        "--json",
    ]);
    assert_eq!(protocol_fail.status.code(), Some(1));
    assert!(
        has_code(&protocol_fail.stdout, "E2006"),
        "expected E2006 from invalid protocol example"
    );

    let protocol_ok = run_aic(&["check", "examples/verify/file_protocol.aic", "--json"]);
    assert_eq!(protocol_ok.status.code(), Some(0));

    let generic_protocol_fail = run_aic(&[
        "check",
        "examples/verify/generic_channel_protocol_invalid.aic",
        "--json",
    ]);
    assert_eq!(generic_protocol_fail.status.code(), Some(1));
    assert!(
        has_code(&generic_protocol_fail.stdout, "E2006"),
        "expected E2006 from generic channel protocol invalid example"
    );

    let fs_protocol_fail = run_aic(&["check", "examples/verify/fs_protocol_invalid.aic", "--json"]);
    assert_eq!(fs_protocol_fail.status.code(), Some(1));
    assert!(
        has_code(&fs_protocol_fail.stdout, "E2006"),
        "expected E2006 from invalid fs protocol example"
    );

    let net_proc_fail = run_aic(&[
        "check",
        "examples/verify/net_proc_protocol_invalid.aic",
        "--json",
    ]);
    assert_eq!(net_proc_fail.status.code(), Some(1));
    assert!(
        has_code(&net_proc_fail.stdout, "E2006"),
        "expected E2006 from invalid net/proc protocol example"
    );

    let capability_fail = run_aic(&[
        "check",
        "examples/verify/capability_missing_invalid.aic",
        "--json",
    ]);
    assert_eq!(capability_fail.status.code(), Some(1));
    assert!(
        has_code(&capability_fail.stdout, "E2009"),
        "expected E2009 from missing capability example"
    );

    let fs_protocol_ok = run_aic(&["check", "examples/verify/fs_protocol_ok.aic", "--json"]);
    assert_eq!(fs_protocol_ok.status.code(), Some(0));

    let net_proc_ok = run_aic(&[
        "check",
        "examples/verify/net_proc_protocol_ok.aic",
        "--json",
    ]);
    assert_eq!(net_proc_ok.status.code(), Some(0));

    let capability_ok = run_aic(&[
        "check",
        "examples/verify/capability_protocol_ok.aic",
        "--json",
    ]);
    assert_eq!(capability_ok.status.code(), Some(0));

    let generic_protocol_ok = run_aic(&[
        "check",
        "examples/verify/generic_channel_protocol_ok.aic",
        "--json",
    ]);
    assert_eq!(generic_protocol_ok.status.code(), Some(0));
}
