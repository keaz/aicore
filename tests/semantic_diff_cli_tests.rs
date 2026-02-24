use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
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

#[test]
fn semantic_diff_cli_emits_json_changes_and_summary() {
    let dir = tempdir().expect("tempdir");
    let old_file = dir.path().join("old.aic");
    let new_file = dir.path().join("new.aic");

    fs::write(
        &old_file,
        "module demo.main;\nfn value(x: Int) -> Int { x }\n",
    )
    .expect("write old file");
    fs::write(
        &new_file,
        "module demo.main;\nfn value(x: Int) -> Float { 1.0 }\n",
    )
    .expect("write new file");

    let old_path = old_file.to_string_lossy().to_string();
    let new_path = new_file.to_string_lossy().to_string();
    let output = run_aic(&["diff", "--semantic", &old_path, &new_path]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let payload: Value = serde_json::from_slice(&output.stdout).expect("parse JSON");
    let changes = payload["changes"].as_array().expect("changes array");
    assert!(!changes.is_empty(), "payload={payload:#}");
    assert!(changes
        .iter()
        .any(|change| change["kind"] == "return_changed"));
    assert!(payload["summary"]["breaking"].is_u64());
    assert!(payload["summary"]["non_breaking"].is_u64());
}

#[test]
fn semantic_diff_cli_fail_on_breaking_returns_non_zero() {
    let dir = tempdir().expect("tempdir");
    let old_file = dir.path().join("old.aic");
    let new_file = dir.path().join("new.aic");

    fs::write(&old_file, "module demo.main;\nfn f(x: Int) -> Int { x }\n").expect("write old file");
    fs::write(
        &new_file,
        "module demo.main;\nfn f(x: Int) -> Float { 1.0 }\n",
    )
    .expect("write new file");

    let old_path = old_file.to_string_lossy().to_string();
    let new_path = new_file.to_string_lossy().to_string();
    let output = run_aic(&[
        "diff",
        "--semantic",
        &old_path,
        &new_path,
        "--fail-on-breaking",
    ]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let payload: Value = serde_json::from_slice(&output.stdout).expect("parse JSON");
    assert!(
        payload["summary"]["breaking"].as_u64().unwrap_or(0) > 0,
        "payload={payload:#}"
    );
}

#[test]
fn semantic_diff_cli_reports_cross_module_changes() {
    let dir = tempdir().expect("tempdir");
    let old_root = dir.path().join("old");
    let new_root = dir.path().join("new");
    fs::create_dir_all(old_root.join("demo")).expect("mkdir old demo");
    fs::create_dir_all(new_root.join("demo")).expect("mkdir new demo");

    fs::write(
        old_root.join("main.aic"),
        "module demo.main;\nimport demo.util;\nfn main() -> Int { 0 }\n",
    )
    .expect("write old main");
    fs::write(
        new_root.join("main.aic"),
        "module demo.main;\nimport demo.util;\nfn main() -> Int { 0 }\n",
    )
    .expect("write new main");

    fs::write(
        old_root.join("demo/util.aic"),
        "module demo.util;\nfn helper() -> Int effects { io } { 1 }\n",
    )
    .expect("write old util");
    fs::write(
        new_root.join("demo/util.aic"),
        "module demo.util;\nfn helper() -> Int effects { io, fs } { 1 }\n",
    )
    .expect("write new util");

    let old_main = old_root.join("main.aic").to_string_lossy().to_string();
    let new_main = new_root.join("main.aic").to_string_lossy().to_string();
    let output = run_aic(&["diff", "--semantic", &old_main, &new_main]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let payload: Value = serde_json::from_slice(&output.stdout).expect("parse JSON");
    let changes = payload["changes"].as_array().expect("changes array");
    assert!(changes.iter().any(|change| {
        change["module"] == "demo.util"
            && change["function"] == "helper"
            && change["kind"] == "effects_changed"
    }));
}
