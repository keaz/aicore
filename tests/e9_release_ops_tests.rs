use std::fs;
use std::path::PathBuf;
use std::process::Command;

use aicore::telemetry::read_events;
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

fn run_aic_in_dir(cwd: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run aic in dir")
}

fn run_aic_with_env(args: &[&str], key: &str, value: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(args)
        .current_dir(repo_root())
        .env(key, value)
        .output()
        .expect("run aic")
}

fn run_aic_with_envs(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aic"));
    command.args(args).current_dir(repo_root());
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run aic")
}

fn first_sandbox_violation(stderr: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(stderr);
    let line = text
        .lines()
        .find(|line| line.contains("\"sandbox_policy_violation\""))
        .unwrap_or_else(|| panic!("missing sandbox violation json in stderr: {text}"));
    serde_json::from_str::<serde_json::Value>(line).expect("parse sandbox violation json line")
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

#[test]
fn release_checksum_verification_command_detects_tampering() {
    let dir = tempdir().expect("tempdir");
    let artifact = dir.path().join("artifact.tar.gz");
    let checksum = dir.path().join("artifact.tar.gz.sha256");
    fs::write(&artifact, b"release-artifact").expect("write artifact");

    let digest = {
        let bytes = fs::read(&artifact).expect("read artifact");
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest;
        hasher.update(&bytes);
        format!("{:x}", hasher.finalize())
    };
    fs::write(&checksum, format!("{digest}  artifact.tar.gz\n")).expect("write checksum");

    let artifact_str = artifact.to_string_lossy().to_string();
    let checksum_str = checksum.to_string_lossy().to_string();

    let verify_ok = run_aic(&[
        "release",
        "verify-checksum",
        "--artifact",
        &artifact_str,
        "--checksum",
        &checksum_str,
    ]);
    assert_eq!(verify_ok.status.code(), Some(0));

    fs::write(&artifact, b"tampered").expect("tamper");

    let verify_fail = run_aic(&[
        "release",
        "verify-checksum",
        "--artifact",
        &artifact_str,
        "--checksum",
        &checksum_str,
        "--json",
    ]);
    assert_eq!(verify_fail.status.code(), Some(1));
    let failures: serde_json::Value =
        serde_json::from_slice(&verify_fail.stdout).expect("parse checksum failure json");
    assert!(failures.is_array());
    assert!(!failures.as_array().expect("array").is_empty());
}

#[test]
fn release_workflow_declares_cross_platform_matrix_and_verification_steps() {
    let workflow = fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .expect("read release workflow");
    for token in [
        "ubuntu-latest",
        "macos-latest",
        "windows-latest",
        "Smoke test binary (Unix)",
        "Smoke test binary (Windows)",
        "Verify archive checksum (Unix)",
        "Verify archive checksum (Windows)",
        "Verify provenance signature",
        "release-metadata.md",
    ] {
        assert!(
            workflow.contains(token),
            "release workflow missing expected token: {token}"
        );
    }
}

#[test]
fn migrate_dry_run_report_is_deterministic() {
    let dir = tempdir().expect("tempdir");
    let project = dir.path().join("migration_project");
    fs::create_dir_all(project.join("src")).expect("mkdir src");

    fs::write(
        project.join("src/main.aic"),
        r#"module migration.demo;
import std.time;
import std.io;
import std.option;

fn main() -> Int effects { io, time } {
    let stamp = std.time.now();
    let maybe: Option[Int] = null;
    let out = match maybe {
        None => stamp,
        Some(v) => v,
    };
    print_int(out);
    0
}
"#,
    )
    .expect("write source");

    fs::write(
        project.join("legacy_ir.json"),
        r#"{
  "module": null,
  "imports": [],
  "items": [],
  "symbols": [],
  "types": [],
  "span": { "start": 0, "end": 0 }
}"#,
    )
    .expect("write legacy ir");

    let first = run_aic_in_dir(&project, &["migrate", ".", "--dry-run", "--json"]);
    let second = run_aic_in_dir(&project, &["migrate", ".", "--dry-run", "--json"]);
    assert_eq!(first.status.code(), Some(0), "stderr={:?}", first.stderr);
    assert_eq!(second.status.code(), Some(0), "stderr={:?}", second.stderr);
    assert_eq!(
        first.stdout, second.stdout,
        "dry-run report must be deterministic"
    );

    let report: serde_json::Value = serde_json::from_slice(&first.stdout).expect("report json");
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["files_changed"], 2);
    assert_eq!(report["high_risk_edits"], 1);
    assert!(report["edits_planned"].as_u64().unwrap_or(0) >= 3);

    let unchanged_source = fs::read_to_string(project.join("src/main.aic")).expect("read source");
    assert!(unchanged_source.contains("std.time.now()"));
    assert!(unchanged_source.contains("null"));
}

#[test]
fn migrate_apply_updates_files_and_writes_report() {
    let dir = tempdir().expect("tempdir");
    let project = dir.path().join("migration_project");
    fs::create_dir_all(project.join("src")).expect("mkdir src");

    fs::write(
        project.join("src/main.aic"),
        r#"module migration.demo;
import std.time;
import std.io;
import std.option;

fn main() -> Int effects { io, time } {
    let stamp = std.time.now();
    let maybe: Option[Int] = null;
    let out = match maybe {
        None => stamp,
        Some(v) => v,
    };
    print_int(out);
    0
}
"#,
    )
    .expect("write source");

    fs::write(
        project.join("legacy_ir.json"),
        r#"{
  "module": null,
  "imports": [],
  "items": [],
  "symbols": [],
  "types": [],
  "span": { "start": 0, "end": 0 }
}"#,
    )
    .expect("write legacy ir");

    let report_path = project.join("migration-report.json");
    let report_arg = report_path.to_string_lossy().to_string();
    let migrated = run_aic_in_dir(&project, &["migrate", ".", "--report", &report_arg]);
    assert_eq!(
        migrated.status.code(),
        Some(0),
        "stderr={:?}",
        migrated.stderr
    );
    assert!(report_path.is_file(), "expected migration report file");

    let rewritten_source = fs::read_to_string(project.join("src/main.aic")).expect("read source");
    assert!(rewritten_source.contains("std.time.now_ms()"));
    assert!(rewritten_source.contains("None()"));
    assert!(!rewritten_source.contains("null"));

    let migrated_ir: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(project.join("legacy_ir.json")).expect("ir"))
            .expect("migrated ir json");
    assert_eq!(migrated_ir["schema_version"], 1);

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("report")).expect("json");
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["files_changed"], 2);

    let checked = run_aic_in_dir(&project, &["check", "src/main.aic"]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "stderr={:?}",
        checked.stderr
    );
}

#[test]
fn migrate_direct_ir_file_rejects_unsupported_schema_version() {
    let dir = tempdir().expect("tempdir");
    let ir_path = dir.path().join("legacy_ir.json");
    fs::write(
        &ir_path,
        r#"{
  "schema_version": 99,
  "module": null,
  "imports": [],
  "items": [],
  "symbols": [],
  "types": [],
  "span": { "start": 0, "end": 0 }
}"#,
    )
    .expect("write unsupported ir");

    let ir_arg = ir_path.to_string_lossy().to_string();
    let out = run_aic_in_dir(dir.path(), &["migrate", &ir_arg, "--dry-run", "--json"]);
    assert_eq!(out.status.code(), Some(3), "stdout={:?}", out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unsupported IR schema_version"));
}

#[test]
fn run_custom_sandbox_policy_blocks_multiple_domains_with_machine_readable_errors() {
    let dir = tempdir().expect("tempdir");
    let policy_path = dir.path().join("ops-policy.json");
    fs::write(
        &policy_path,
        r#"{
  "profile": "ops-test",
  "permissions": { "fs": false, "net": false, "proc": false, "time": false }
}"#,
    )
    .expect("write policy");
    let policy_arg = policy_path.to_string_lossy().to_string();

    for (example, expected_domain, expected_operation) in [
        (
            "examples/ops/sandbox_profiles/fs_blocked_demo.aic",
            "fs",
            "read_text",
        ),
        (
            "examples/ops/sandbox_profiles/net_blocked_demo.aic",
            "net",
            "dns_lookup",
        ),
        (
            "examples/ops/sandbox_profiles/proc_blocked_demo.aic",
            "proc",
            "run",
        ),
        (
            "examples/ops/sandbox_profiles/time_blocked_demo.aic",
            "time",
            "parse_rfc3339",
        ),
    ] {
        let out = run_aic(&["run", example, "--sandbox-config", &policy_arg]);
        assert_eq!(out.status.code(), Some(0), "stderr={:?}", out.stderr);
        let violation = first_sandbox_violation(&out.stderr);
        assert_eq!(violation["code"], "sandbox_policy_violation");
        assert_eq!(violation["profile"], "ops-test");
        assert_eq!(violation["domain"], expected_domain);
        assert_eq!(violation["operation"], expected_operation);
    }
}

#[test]
fn telemetry_schema_contract_is_stable() {
    let schema_path = repo_root().join("docs/security-ops/telemetry.schema.json");
    let raw = fs::read_to_string(schema_path).expect("read telemetry schema");
    let schema: serde_json::Value = serde_json::from_str(&raw).expect("parse telemetry schema");

    assert_eq!(schema["properties"]["schema_version"]["const"], "1.0");
    let required = schema["required"].as_array().expect("required array");
    for field in [
        "schema_version",
        "event_index",
        "timestamp_ms",
        "trace_id",
        "command",
        "kind",
        "attrs",
    ] {
        assert!(
            required.contains(&serde_json::json!(field)),
            "schema required fields missing {field}"
        );
    }
}

#[test]
fn telemetry_trace_id_correlates_runtime_violation_and_event_log() {
    let dir = tempdir().expect("tempdir");
    let policy_path = dir.path().join("policy.json");
    let telemetry_path = dir.path().join("telemetry.jsonl");
    fs::write(
        &policy_path,
        r#"{
  "profile": "telemetry-test",
  "permissions": { "fs": false, "net": true, "proc": true, "time": true }
}"#,
    )
    .expect("write policy");

    let policy_arg = policy_path.to_string_lossy().to_string();
    let telemetry_arg = telemetry_path.to_string_lossy().to_string();
    let trace_id = "trace-ops-123";
    let out = run_aic_with_envs(
        &[
            "run",
            "examples/ops/sandbox_profiles/fs_blocked_demo.aic",
            "--sandbox-config",
            &policy_arg,
        ],
        &[
            ("AIC_TRACE_ID", trace_id),
            ("AIC_TELEMETRY_PATH", telemetry_arg.as_str()),
        ],
    );
    assert_eq!(out.status.code(), Some(0), "stderr={:?}", out.stderr);

    let violation = first_sandbox_violation(&out.stderr);
    assert_eq!(violation["trace_id"], trace_id);

    let events = read_events(&telemetry_path).expect("read telemetry events");
    assert!(!events.is_empty(), "telemetry should contain events");
    assert!(
        events.iter().all(|event| event.trace_id == trace_id),
        "all telemetry events should carry the provided trace id"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn run_supports_ci_sandbox_profile() {
    let out = run_aic(&["run", "examples/option_match.aic", "--sandbox", "ci"]);
    assert_eq!(out.status.code(), Some(0));
}

#[cfg(target_os = "linux")]
#[test]
fn strict_profile_selection_blocks_fs_while_none_allows_it() {
    let strict = run_aic(&[
        "run",
        "examples/ops/sandbox_profiles/fs_blocked_demo.aic",
        "--sandbox",
        "strict",
    ]);
    assert_eq!(strict.status.code(), Some(0), "stderr={:?}", strict.stderr);
    let violation = first_sandbox_violation(&strict.stderr);
    assert_eq!(violation["profile"], "strict");
    assert_eq!(violation["domain"], "fs");

    let none = run_aic(&[
        "run",
        "examples/ops/sandbox_profiles/fs_blocked_demo.aic",
        "--sandbox",
        "none",
    ]);
    assert_eq!(none.status.code(), Some(1), "stderr={:?}", none.stderr);
}
