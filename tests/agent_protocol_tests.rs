use std::fs;
use std::path::PathBuf;
use std::process::Command;

use jsonschema::JSONSchema;
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

fn read_json(path: &str) -> Value {
    let raw = fs::read_to_string(repo_root().join(path)).expect("read json file");
    serde_json::from_str(&raw).expect("parse json")
}

#[test]
fn protocol_examples_validate_against_published_schemas() {
    let fixtures = [
        (
            "docs/agent-tooling/schemas/parse-response.schema.json",
            "examples/agent/protocol_parse.json",
        ),
        (
            "docs/agent-tooling/schemas/check-response.schema.json",
            "examples/agent/protocol_check.json",
        ),
        (
            "docs/agent-tooling/schemas/build-response.schema.json",
            "examples/agent/protocol_build.json",
        ),
        (
            "docs/agent-tooling/schemas/fix-response.schema.json",
            "examples/agent/protocol_fix.json",
        ),
        (
            "docs/agent-tooling/schemas/parse-response.schema.json",
            "examples/agent/protocol_parse_error.json",
        ),
        (
            "docs/agent-tooling/schemas/build-response.schema.json",
            "examples/agent/protocol_build_error.json",
        ),
        (
            "docs/agent-tooling/schemas/fix-response.schema.json",
            "examples/agent/protocol_fix_conflict.json",
        ),
    ];

    for (schema_path, fixture_path) in fixtures {
        let schema = read_json(schema_path);
        let fixture = read_json(fixture_path);
        let compiled = JSONSchema::compile(&schema).expect("compile schema");
        let result = compiled.validate(&fixture);
        assert!(
            result.is_ok(),
            "fixture {} does not satisfy schema {}: {:?}",
            fixture_path,
            schema_path,
            result.err().map(|errs| errs.collect::<Vec<_>>())
        );
    }
}

#[test]
fn contract_json_exposes_protocol_schemas_and_examples() {
    let out = run_aic(&["contract", "--json"]);
    assert_eq!(out.status.code(), Some(0));

    let contract: Value = serde_json::from_slice(&out.stdout).expect("contract json");
    assert_eq!(contract["protocol"]["name"], "aic-compiler-json");
    assert_eq!(contract["protocol"]["selected_version"], "1.0");

    for phase in ["parse", "check", "build", "fix"] {
        assert!(contract["schemas"][phase]["path"].is_string());
        assert!(contract["examples"][phase].is_string());
    }

    let commands = contract["commands"].as_array().expect("command contracts");
    let coverage = commands
        .iter()
        .find(|entry| entry["name"] == "coverage")
        .expect("coverage contract");
    assert!(coverage["stable_flags"]
        .as_array()
        .expect("coverage flags")
        .iter()
        .any(|flag| flag == "--check"));
    assert!(coverage["stable_flags"]
        .as_array()
        .expect("coverage flags")
        .iter()
        .any(|flag| flag == "--min"));

    let run = commands
        .iter()
        .find(|entry| entry["name"] == "run")
        .expect("run contract");
    assert!(run["stable_flags"]
        .as_array()
        .expect("run flags")
        .iter()
        .any(|flag| flag == "--profile"));
}

#[test]
fn contract_negotiation_selects_compatible_version() {
    let out = run_aic(&["contract", "--json", "--accept-version", "1.2,2.0"]);
    assert_eq!(out.status.code(), Some(0));

    let contract: Value = serde_json::from_slice(&out.stdout).expect("contract json");
    assert_eq!(contract["protocol"]["compatible"], true);
    assert_eq!(contract["protocol"]["selected_version"], "1.0");
}

#[test]
fn contract_negotiation_reports_incompatible_major() {
    let out = run_aic(&["contract", "--json", "--accept-version", "2.0"]);
    assert_eq!(out.status.code(), Some(1));

    let contract: Value = serde_json::from_slice(&out.stdout).expect("contract json");
    assert_eq!(contract["protocol"]["compatible"], false);
    assert!(contract["protocol"]["selected_version"].is_null());
}

#[test]
fn diag_apply_fixes_json_validates_against_fix_schema() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("fixable.aic");
    fs::write(
        &file,
        "module proto.fix;\nfn main() -> Int {\n    let x = 1\n    x\n}\n",
    )
    .expect("write source");

    let file_str = file.to_string_lossy().to_string();
    let out = run_aic(&["diag", "apply-fixes", &file_str, "--dry-run", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let response: Value = serde_json::from_slice(&out.stdout).expect("fix response");

    let schema = read_json("docs/agent-tooling/schemas/fix-response.schema.json");
    let compiled = JSONSchema::compile(&schema).expect("compile fix schema");
    let result = compiled.validate(&response);
    assert!(
        result.is_ok(),
        "fix response does not satisfy schema: {:?}",
        result.err().map(|errs| errs.collect::<Vec<_>>())
    );
}

#[test]
fn documented_protocol_fixtures_smoke_against_cli() {
    let check_fixture = read_json("examples/agent/protocol_check.json");
    let check_input = check_fixture["input"]
        .as_str()
        .expect("check fixture input");
    let check = run_aic(&["check", check_input, "--json"]);
    assert_eq!(check.status.code(), Some(1));
    let check_json: Value = serde_json::from_slice(&check.stdout).expect("check json");
    let expected_code = check_fixture["diagnostics"][0]["code"]
        .as_str()
        .expect("fixture check code");
    assert_eq!(check_json[0]["code"], expected_code);

    let build_fixture = read_json("examples/agent/protocol_build.json");
    let build_input = build_fixture["input"]
        .as_str()
        .expect("build fixture input");
    let temp = tempdir().expect("tempdir");
    let build_output = temp.path().join("build_fixture.o");
    let build_output_str = build_output.to_string_lossy().to_string();
    let build = run_aic(&[
        "build",
        build_input,
        "--artifact",
        "obj",
        "-o",
        &build_output_str,
    ]);
    assert_eq!(build.status.code(), Some(0));
    assert!(build_output.exists(), "expected built artifact");

    let build_error_fixture = read_json("examples/agent/protocol_build_error.json");
    let build_error_input = build_error_fixture["input"]
        .as_str()
        .expect("build error fixture input");
    let temp_error = tempdir().expect("tempdir error");
    let temp_error_out = temp_error.path().join("build_error.o");
    let temp_error_out_str = temp_error_out.to_string_lossy().to_string();
    let build_error = run_aic(&[
        "build",
        build_error_input,
        "--artifact",
        "obj",
        "-o",
        &temp_error_out_str,
    ]);
    assert_eq!(build_error.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&build_error.stderr);
    assert!(
        stderr.contains("E2001") || String::from_utf8_lossy(&build_error.stdout).contains("E2001"),
        "expected build failure diagnostic E2001; stdout={} stderr={}",
        String::from_utf8_lossy(&build_error.stdout),
        stderr
    );

    let fix_fixture = read_json("examples/agent/protocol_fix.json");
    let fix_input = fix_fixture["files_changed"][0]
        .as_str()
        .expect("fix fixture input file");
    let fix = run_aic(&["diag", "apply-fixes", fix_input, "--dry-run", "--json"]);
    assert_eq!(fix.status.code(), Some(0));
    let fix_json: Value = serde_json::from_slice(&fix.stdout).expect("fix json");
    assert_eq!(fix_json["phase"], "fix");
    assert_eq!(fix_json["mode"], "dry-run");
}

#[test]
fn lsp_workflow_fixture_covers_required_methods_and_error_case() {
    let fixture = read_json("examples/agent/lsp_workflow.json");
    let flows = fixture["flows"].as_array().expect("flows array");
    let names = flows
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect::<Vec<_>>();
    for required in [
        "completion",
        "goto-definition",
        "rename",
        "code-action",
        "semantic-tokens",
    ] {
        assert!(
            names.contains(&required),
            "missing required flow '{required}' in lsp_workflow.json"
        );
    }
    for flow in flows {
        assert!(flow["request"]["method"].is_string());
        assert!(flow["response"]["jsonrpc"].is_string());
    }
    let errors = fixture["error_cases"]
        .as_array()
        .expect("error cases array");
    assert!(!errors.is_empty(), "expected at least one LSP error case");
    assert_eq!(errors[0]["response"]["error"]["code"], -32601);
}

#[test]
fn protocol_doc_references_full_agent_tooling_surface() {
    let doc = fs::read_to_string(repo_root().join("docs/agent-tooling/protocol-v1.md"))
        .expect("read protocol doc");
    for expected in [
        "docs/agent-tooling/schemas/parse-response.schema.json",
        "docs/agent-tooling/schemas/check-response.schema.json",
        "docs/agent-tooling/schemas/build-response.schema.json",
        "docs/agent-tooling/schemas/fix-response.schema.json",
        "examples/agent/lsp_workflow.json",
        "docs/agent-tooling/incremental-daemon.md",
        "docs/agent-recipes/",
    ] {
        assert!(
            doc.contains(expected),
            "protocol-v1 doc missing reference: {expected}"
        );
    }
}
