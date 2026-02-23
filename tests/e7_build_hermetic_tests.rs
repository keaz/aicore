use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::tempdir;

fn run_aic_in_dir(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run aic in temp dir")
}

fn write_fixture_source(root: &Path) -> PathBuf {
    let source = root.join("hermetic_demo.aic");
    fs::write(
        &source,
        concat!(
            "module hermetic.demo;\n",
            "fn main() -> Int {\n",
            "    0\n",
            "}\n",
        ),
    )
    .expect("write fixture source");
    source
}

fn sha256_hex(path: &Path) -> String {
    use sha2::Digest;
    let payload = fs::read(path).expect("read artifact");
    let mut hasher = sha2::Sha256::new();
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

#[test]
fn build_creates_default_manifest_and_content_addressed_artifact() {
    let dir = tempdir().expect("temp dir");
    let input = write_fixture_source(dir.path());
    let output = dir.path().join("demo-bin");

    let input_arg = input.to_string_lossy().to_string();
    let output_arg = output.to_string_lossy().to_string();
    let args = vec!["build", input_arg.as_str(), "-o", output_arg.as_str()];
    let run = run_aic_in_dir(dir.path(), &args);
    assert!(
        run.status.success(),
        "build failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let manifest_path = dir.path().join("build.json");
    assert!(manifest_path.exists(), "default build.json missing");
    assert!(output.exists(), "output artifact missing");

    let manifest_raw = fs::read_to_string(&manifest_path).expect("read manifest");
    let manifest: Value = serde_json::from_str(&manifest_raw).expect("parse manifest");
    let output_digest = sha256_hex(&output);

    assert_eq!(manifest["input_path"].as_str(), Some(input_arg.as_str()));
    assert_eq!(manifest["output_path"].as_str(), Some(output_arg.as_str()));
    assert_eq!(
        manifest["output_sha256"].as_str(),
        Some(output_digest.as_str())
    );
    assert_eq!(manifest["artifact_kind"].as_str(), Some("exe"));

    let cas_path_raw = manifest["content_addressed_artifact_path"]
        .as_str()
        .expect("manifest content-addressed path");
    assert!(
        cas_path_raw.contains(&output_digest),
        "content-addressed path does not contain digest: {cas_path_raw}"
    );
    let cas_path = PathBuf::from(cas_path_raw);
    assert!(cas_path.exists(), "content-addressed artifact missing");
    assert_eq!(
        fs::read(&output).expect("read output"),
        fs::read(&cas_path).expect("read content-addressed artifact"),
        "content-addressed artifact bytes should match output bytes"
    );
}

#[test]
fn build_verify_hash_succeeds_and_fails() {
    let dir = tempdir().expect("temp dir");
    let input = write_fixture_source(dir.path());
    let output = dir.path().join("demo-bin");
    let manifest = dir.path().join("manifest.json");

    let input_arg = input.to_string_lossy().to_string();
    let output_arg = output.to_string_lossy().to_string();
    let manifest_arg = manifest.to_string_lossy().to_string();

    let initial_args = vec![
        "build",
        input_arg.as_str(),
        "-o",
        output_arg.as_str(),
        "--manifest",
        manifest_arg.as_str(),
    ];
    let initial = run_aic_in_dir(dir.path(), &initial_args);
    assert!(
        initial.status.success(),
        "initial build failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&initial.stdout),
        String::from_utf8_lossy(&initial.stderr)
    );

    let digest = sha256_hex(&output);
    let pass_args = vec![
        "build",
        input_arg.as_str(),
        "-o",
        output_arg.as_str(),
        "--manifest",
        manifest_arg.as_str(),
        "--verify-hash",
        digest.as_str(),
    ];
    let pass = run_aic_in_dir(dir.path(), &pass_args);
    assert!(
        pass.status.success(),
        "verify-hash success case failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&pass.stdout),
        String::from_utf8_lossy(&pass.stderr)
    );

    let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let fail_args = vec![
        "build",
        input_arg.as_str(),
        "-o",
        output_arg.as_str(),
        "--manifest",
        manifest_arg.as_str(),
        "--verify-hash",
        wrong_hash,
    ];
    let fail = run_aic_in_dir(dir.path(), &fail_args);
    assert_eq!(
        fail.status.code(),
        Some(1),
        "expected diagnostic failure exit"
    );
    let stderr = String::from_utf8_lossy(&fail.stderr);
    assert!(
        stderr.contains("--verify-hash mismatch"),
        "missing verify-hash mismatch diagnostic: {stderr}"
    );
}

#[test]
fn build_manifest_is_deterministic_for_repeated_identical_builds() {
    let dir = tempdir().expect("temp dir");
    let input = write_fixture_source(dir.path());
    let output = dir.path().join("demo-bin");
    let manifest = dir.path().join("deterministic-build.json");

    let input_arg = input.to_string_lossy().to_string();
    let output_arg = output.to_string_lossy().to_string();
    let manifest_arg = manifest.to_string_lossy().to_string();

    let args = vec![
        "build",
        input_arg.as_str(),
        "-o",
        output_arg.as_str(),
        "--manifest",
        manifest_arg.as_str(),
    ];

    let first = run_aic_in_dir(dir.path(), &args);
    assert!(
        first.status.success(),
        "first build failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let first_manifest = fs::read_to_string(&manifest).expect("read first manifest");
    let first_json: Value = serde_json::from_str(&first_manifest).expect("parse first manifest");

    let second = run_aic_in_dir(dir.path(), &args);
    assert!(
        second.status.success(),
        "second build failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    let second_manifest = fs::read_to_string(&manifest).expect("read second manifest");
    let second_json: Value = serde_json::from_str(&second_manifest).expect("parse second manifest");

    assert_eq!(
        first_manifest, second_manifest,
        "manifest JSON must be byte-identical across repeated identical builds"
    );
    assert_eq!(
        first_json["output_sha256"], second_json["output_sha256"],
        "output digest should be stable across repeated identical builds"
    );
}
