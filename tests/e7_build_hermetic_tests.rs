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

fn write_wasm_io_fixture_source(root: &Path) -> PathBuf {
    let source = root.join("wasm_io_demo.aic");
    fs::write(
        &source,
        concat!(
            "import std.io;\n",
            "fn main() -> Int effects { io } {\n",
            "    print_int(42);\n",
            "    0\n",
            "}\n",
        ),
    )
    .expect("write wasm io fixture source");
    source
}

fn wasm_target_unavailable(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("wasm32-unknown-unknown")
        && (lower.contains("no available targets")
            || lower.contains("unknown target")
            || lower.contains("unable to create target")
            || lower.contains("is not a valid target")
            || lower.contains("unsupported option"))
}

fn assert_wasm_build_succeeded_or_skip(run: &std::process::Output) -> bool {
    if run.status.success() {
        return true;
    }
    let stderr = String::from_utf8_lossy(&run.stderr);
    if wasm_target_unavailable(&stderr) {
        eprintln!("skipping wasm build test; toolchain does not support wasm32 target: {stderr}");
        return false;
    }
    panic!(
        "wasm build failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        stderr
    );
}

fn sha256_hex(path: &Path) -> String {
    use sha2::Digest;
    let payload = fs::read(path).expect("read artifact");
    let mut hasher = sha2::Sha256::new();
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

#[test]
fn build_wasm_target_emits_wasm_magic_and_manifest_target() {
    let dir = tempdir().expect("temp dir");
    let input = write_fixture_source(dir.path());
    let input_arg = input.to_string_lossy().to_string();
    let args = vec!["build", input_arg.as_str(), "--target", "wasm32"];
    let run = run_aic_in_dir(dir.path(), &args);
    if !assert_wasm_build_succeeded_or_skip(&run) {
        return;
    }

    let output = dir.path().join("hermetic_demo.wasm");
    assert!(
        output.exists(),
        "expected wasm output at {}",
        output.display()
    );
    let bytes = fs::read(&output).expect("read wasm output");
    assert!(bytes.len() >= 4, "wasm output too small");
    assert_eq!(
        &bytes[..4],
        b"\0asm",
        "missing wasm magic bytes at start of artifact"
    );

    let wasm_text = String::from_utf8_lossy(&bytes);
    assert!(
        !wasm_text.contains("aic_rt_"),
        "pure wasm program should not require runtime host imports"
    );

    let manifest_path = dir.path().join("build.json");
    let manifest_raw = fs::read_to_string(&manifest_path).expect("read manifest");
    let manifest: Value = serde_json::from_str(&manifest_raw).expect("parse manifest");
    assert_eq!(manifest["target"].as_str(), Some("wasm32"));
    assert_eq!(manifest["artifact_kind"].as_str(), Some("exe"));
    assert_eq!(manifest["output_path"].as_str(), Some("hermetic_demo.wasm"));
}

#[test]
fn build_wasm_io_program_binds_runtime_calls_as_imports() {
    let dir = tempdir().expect("temp dir");
    let input = write_wasm_io_fixture_source(dir.path());
    let output = dir.path().join("wasm-io.wasm");
    let input_arg = input.to_string_lossy().to_string();
    let output_arg = output.to_string_lossy().to_string();
    let args = vec![
        "build",
        input_arg.as_str(),
        "--target",
        "wasm32",
        "-o",
        output_arg.as_str(),
    ];
    let run = run_aic_in_dir(dir.path(), &args);
    if !assert_wasm_build_succeeded_or_skip(&run) {
        return;
    }

    let bytes = fs::read(&output).expect("read wasm io output");
    assert!(bytes.starts_with(b"\0asm"), "missing wasm magic bytes");
    let wasm_text = String::from_utf8_lossy(&bytes);
    assert!(
        wasm_text.contains("aic_rt_print_int"),
        "expected io runtime call to be host import-bound in wasm artifact"
    );
}

fn expected_default_target_label() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => "x86_64-linux",
        ("aarch64", "linux") => "aarch64-linux",
        ("x86_64", "macos") => "x86_64-macos",
        ("aarch64", "macos") => "aarch64-macos",
        ("x86_64", "windows") => "x86_64-windows",
        (_, "macos") => "macos",
        (_, "windows") => "windows",
        _ => "linux",
    }
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
    assert_eq!(
        manifest["target"].as_str(),
        Some(expected_default_target_label())
    );
    assert_eq!(manifest["static_link"].as_bool(), Some(false));

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
fn build_accepts_explicit_host_target() {
    let dir = tempdir().expect("temp dir");
    let input = write_fixture_source(dir.path());
    let output = dir.path().join("demo-target-bin");

    let input_arg = input.to_string_lossy().to_string();
    let output_arg = output.to_string_lossy().to_string();
    let target_arg = expected_default_target_label();
    let args = vec![
        "build",
        input_arg.as_str(),
        "-o",
        output_arg.as_str(),
        "--target",
        target_arg,
    ];
    let run = run_aic_in_dir(dir.path(), &args);
    assert!(
        run.status.success(),
        "build with explicit host target failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let manifest_path = dir.path().join("build.json");
    let manifest_raw = fs::read_to_string(&manifest_path).expect("read manifest");
    let manifest: Value = serde_json::from_str(&manifest_raw).expect("parse manifest");
    assert_eq!(manifest["target"].as_str(), Some(target_arg));
}

#[test]
fn build_rejects_static_link_for_non_executable_artifact() {
    let dir = tempdir().expect("temp dir");
    let input = write_fixture_source(dir.path());
    let output = dir.path().join("demo-obj.o");

    let input_arg = input.to_string_lossy().to_string();
    let output_arg = output.to_string_lossy().to_string();
    let args = vec![
        "build",
        input_arg.as_str(),
        "-o",
        output_arg.as_str(),
        "--artifact",
        "obj",
        "--static-link",
    ];
    let run = run_aic_in_dir(dir.path(), &args);
    assert_eq!(run.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("--static-link is supported only with --artifact exe"),
        "missing static-link usage diagnostic: {stderr}"
    );
}

#[test]
fn build_rejects_static_link_for_non_linux_target() {
    let dir = tempdir().expect("temp dir");
    let input = write_fixture_source(dir.path());
    let output = dir.path().join("demo-static");

    let input_arg = input.to_string_lossy().to_string();
    let output_arg = output.to_string_lossy().to_string();
    let args = vec![
        "build",
        input_arg.as_str(),
        "-o",
        output_arg.as_str(),
        "--target",
        "x86_64-windows",
        "--static-link",
    ];
    let run = run_aic_in_dir(dir.path(), &args);
    assert_eq!(run.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("--static-link currently supports linux targets only"),
        "missing static-link target diagnostic: {stderr}"
    );
}

#[test]
fn build_rejects_wasm_target_for_non_executable_artifact() {
    let dir = tempdir().expect("temp dir");
    let input = write_fixture_source(dir.path());
    let output = dir.path().join("demo-obj.o");

    let input_arg = input.to_string_lossy().to_string();
    let output_arg = output.to_string_lossy().to_string();
    let args = vec![
        "build",
        input_arg.as_str(),
        "-o",
        output_arg.as_str(),
        "--target",
        "wasm32",
        "--artifact",
        "obj",
    ];
    let run = run_aic_in_dir(dir.path(), &args);
    assert_eq!(run.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("--target wasm32 currently supports --artifact exe only"),
        "missing wasm artifact diagnostic: {stderr}"
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
