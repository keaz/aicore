use std::fs;
use std::path::PathBuf;
use std::process::Command;

use jsonschema::JSONSchema;
use serde_json::Value;

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
