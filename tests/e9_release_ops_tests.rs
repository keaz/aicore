use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;

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

fn run_aic_with_env(args: &[&str], key: &str, value: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(args)
        .current_dir(repo_root())
        .env(key, value)
        .output()
        .expect("run aic")
}

#[test]
fn release_manifest_generation_and_verification_are_deterministic() {
    let dir = tempdir().expect("tempdir");
    let out1 = dir.path().join("manifest-a.json");
    let out2 = dir.path().join("manifest-b.json");

    let root = repo_root();
    let root_str = root.to_string_lossy().to_string();
    let out1_str = out1.to_string_lossy().to_string();
    let out2_str = out2.to_string_lossy().to_string();

    let first = run_aic(&[
        "release",
        "manifest",
        "--root",
        &root_str,
        "--output",
        &out1_str,
        "--source-date-epoch",
        "1700000000",
    ]);
    assert_eq!(first.status.code(), Some(0));

    let second = run_aic(&[
        "release",
        "manifest",
        "--root",
        &root_str,
        "--output",
        &out2_str,
        "--source-date-epoch",
        "1700000000",
    ]);
    assert_eq!(second.status.code(), Some(0));

    let a = fs::read_to_string(&out1).expect("read manifest a");
    let b = fs::read_to_string(&out2).expect("read manifest b");
    assert_eq!(a, b);

    let verify = run_aic(&[
        "release",
        "verify-manifest",
        "--root",
        &root_str,
        "--manifest",
        &out1_str,
    ]);
    assert_eq!(verify.status.code(), Some(0));
}

#[test]
fn release_sbom_and_provenance_verification_flow() {
    let dir = tempdir().expect("tempdir");
    let root = repo_root();

    let sbom = dir.path().join("sbom.json");
    let artifact = dir.path().join("artifact.bin");
    let provenance = dir.path().join("provenance.json");

    fs::write(&artifact, b"artifact-payload").expect("write artifact");

    let root_str = root.to_string_lossy().to_string();
    let sbom_str = sbom.to_string_lossy().to_string();
    let artifact_str = artifact.to_string_lossy().to_string();
    let provenance_str = provenance.to_string_lossy().to_string();

    let sbom_cmd = run_aic(&[
        "release",
        "sbom",
        "--root",
        &root_str,
        "--output",
        &sbom_str,
        "--source-date-epoch",
        "1700000000",
    ]);
    assert_eq!(sbom_cmd.status.code(), Some(0));

    let provenance_cmd = run_aic_with_env(
        &[
            "release",
            "provenance",
            "--artifact",
            &artifact_str,
            "--sbom",
            &sbom_str,
            "--output",
            &provenance_str,
            "--key-env",
            "AIC_SIGNING_KEY",
            "--key-id",
            "ci-test",
        ],
        "AIC_SIGNING_KEY",
        "integration-test-key",
    );
    assert_eq!(provenance_cmd.status.code(), Some(0));

    let verify_ok = run_aic_with_env(
        &[
            "release",
            "verify-provenance",
            "--provenance",
            &provenance_str,
            "--key-env",
            "AIC_SIGNING_KEY",
        ],
        "AIC_SIGNING_KEY",
        "integration-test-key",
    );
    assert_eq!(verify_ok.status.code(), Some(0));

    fs::write(&artifact, b"tampered-payload").expect("tamper artifact");

    let verify_fail = run_aic_with_env(
        &[
            "release",
            "verify-provenance",
            "--provenance",
            &provenance_str,
            "--key-env",
            "AIC_SIGNING_KEY",
            "--json",
        ],
        "AIC_SIGNING_KEY",
        "integration-test-key",
    );
    assert_eq!(verify_fail.status.code(), Some(1));
    let failures: serde_json::Value =
        serde_json::from_slice(&verify_fail.stdout).expect("parse verification json");
    assert!(failures.is_array());
    assert!(!failures.as_array().expect("array").is_empty());
}

#[test]
fn release_policy_and_security_audit_commands_work() {
    let policy = run_aic(&["release", "policy", "--check", "--json"]);
    assert_eq!(policy.status.code(), Some(0));
    let policy_json: serde_json::Value =
        serde_json::from_slice(&policy.stdout).expect("policy json");
    assert_eq!(policy_json["policy"]["version"], "1.0");
    assert!(policy_json["problems"].is_array());

    let security = run_aic(&["release", "security-audit", "--json"]);
    assert_eq!(security.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_slice(&security.stdout).expect("security json");
    assert_eq!(report["ok"], true);
    assert!(report["checks"].is_array());
}

#[cfg(target_os = "linux")]
#[test]
fn run_supports_ci_sandbox_profile() {
    let out = run_aic(&["run", "examples/option_match.aic", "--sandbox", "ci"]);
    assert_eq!(out.status.code(), Some(0));
}
