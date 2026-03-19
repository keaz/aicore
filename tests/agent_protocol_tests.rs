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
            "docs/agent-tooling/schemas/testgen-response.schema.json",
            "examples/agent/protocol_testgen.json",
        ),
        (
            "docs/agent-tooling/schemas/session-response.schema.json",
            "examples/agent/protocol_session.json",
        ),
        (
            "docs/agent-tooling/schemas/patch-response.schema.json",
            "examples/agent/protocol_patch.json",
        ),
        (
            "docs/agent-tooling/schemas/validate-call-response.schema.json",
            "examples/agent/protocol_validate_call.json",
        ),
        (
            "docs/agent-tooling/schemas/validate-type-response.schema.json",
            "examples/agent/protocol_validate_type.json",
        ),
        (
            "docs/agent-tooling/schemas/suggest-response.schema.json",
            "examples/agent/protocol_suggest.json",
        ),
        (
            "docs/agent-tooling/schemas/context-response.schema.json",
            "examples/agent/protocol_context.json",
        ),
        (
            "docs/agent-tooling/schemas/query-response.schema.json",
            "examples/agent/protocol_query.json",
        ),
        (
            "docs/agent-tooling/schemas/query-response.schema.json",
            "examples/agent/protocol_query_partial.json",
        ),
        (
            "docs/agent-tooling/schemas/symbols-response.schema.json",
            "examples/agent/protocol_symbols.json",
        ),
        (
            "docs/agent-tooling/schemas/symbols-response.schema.json",
            "examples/agent/protocol_symbols_partial.json",
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

    for phase in [
        "parse",
        "check",
        "build",
        "fix",
        "testgen",
        "session",
        "patch",
        "validate-call",
        "validate-type",
        "suggest",
        "context",
        "query",
        "symbols",
    ] {
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
fn patch_request_fixture_validates_against_published_schema() {
    let schema = read_json("docs/agent-tooling/schemas/patch-request.schema.json");
    let fixture = read_json("examples/e7/patch_protocol/patches/valid_patch.json");
    let compiled = JSONSchema::compile(&schema).expect("compile patch request schema");
    let result = compiled.validate(&fixture);
    assert!(
        result.is_ok(),
        "patch request fixture does not satisfy schema: {:?}",
        result.err().map(|errs| errs.collect::<Vec<_>>())
    );
}

#[test]
fn patch_preview_json_validates_against_published_schema() {
    let out = run_aic(&[
        "patch",
        "--preview",
        "examples/e7/patch_protocol/patches/valid_patch.json",
        "--project",
        "examples/e7/patch_protocol",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0));

    let response: Value = serde_json::from_slice(&out.stdout).expect("patch preview response");
    let schema = read_json("docs/agent-tooling/schemas/patch-response.schema.json");
    let compiled = JSONSchema::compile(&schema).expect("compile patch response schema");
    let result = compiled.validate(&response);
    assert!(
        result.is_ok(),
        "patch response does not satisfy schema: {:?}",
        result.err().map(|errs| errs.collect::<Vec<_>>())
    );
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
fn session_lock_json_validates_against_session_schema() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
    fs::write(
        dir.path().join("aic.toml"),
        "[package]\nname = \"session_schema\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write manifest");
    fs::write(
        dir.path().join("src/main.aic"),
        concat!(
            "module session.schema;\n",
            "fn helper(x: Int) -> Int {\n",
            "    x\n",
            "}\n",
            "fn main() -> Int {\n",
            "    helper(1)\n",
            "}\n",
        ),
    )
    .expect("write source");

    let create = run_aic(&[
        "session",
        "create",
        "--project",
        dir.path().to_str().expect("project path"),
        "--json",
    ]);
    assert_eq!(create.status.code(), Some(0));

    let lock = run_aic(&[
        "session",
        "lock",
        "acquire",
        "sess-0001",
        "--for",
        "function",
        "main",
        "--project",
        dir.path().to_str().expect("project path"),
        "--now-ms",
        "1000",
        "--json",
    ]);
    assert_eq!(lock.status.code(), Some(0));
    let response: Value = serde_json::from_slice(&lock.stdout).expect("session lock response");

    let schema = read_json("docs/agent-tooling/schemas/session-response.schema.json");
    let compiled = JSONSchema::compile(&schema).expect("compile session schema");
    let result = compiled.validate(&response);
    assert!(
        result.is_ok(),
        "session response does not satisfy schema: {:?}",
        result.err().map(|errs| errs.collect::<Vec<_>>())
    );
}

#[test]
fn validate_and_suggest_json_validate_against_published_schemas() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
    fs::write(
        dir.path().join("aic.toml"),
        "[package]\nname = \"api_conformance\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write manifest");
    fs::write(
        dir.path().join("src/main.aic"),
        concat!(
            "module api_conformance.main;\n",
            "import api_conformance.math;\n",
            "import api_conformance.models;\n",
            "\n",
            "fn handle_result(user: User, amount: Int) -> Int {\n",
            "    math.add(40, amount)\n",
            "}\n",
        ),
    )
    .expect("write main");
    fs::write(
        dir.path().join("src/math.aic"),
        concat!(
            "module api_conformance.math;\n",
            "\n",
            "pub fn add(x: Int, y: Int) -> Int {\n",
            "    x + y\n",
            "}\n",
        ),
    )
    .expect("write math");
    fs::write(
        dir.path().join("src/models.aic"),
        concat!(
            "module api_conformance.models;\n",
            "\n",
            "pub struct User {\n",
            "    id: Int,\n",
            "}\n",
            "\n",
            "pub enum AppError {\n",
            "    NotFound,\n",
            "}\n",
        ),
    )
    .expect("write models");

    let project_path = dir.path().to_str().expect("project path");

    let validate_call = run_aic(&[
        "validate-call",
        "math.add",
        "--arg",
        "Int",
        "--arg",
        "Int",
        "--project",
        project_path,
    ]);
    assert_eq!(validate_call.status.code(), Some(0));
    let validate_call_json: Value =
        serde_json::from_slice(&validate_call.stdout).expect("validate-call response");
    let validate_call_schema =
        read_json("docs/agent-tooling/schemas/validate-call-response.schema.json");
    let validate_call_compiled =
        JSONSchema::compile(&validate_call_schema).expect("compile validate-call schema");
    assert!(
        validate_call_compiled.validate(&validate_call_json).is_ok(),
        "validate-call response does not satisfy schema"
    );

    let validate_type = run_aic(&[
        "validate-type",
        "Result[User, AppError]",
        "--project",
        project_path,
    ]);
    assert_eq!(validate_type.status.code(), Some(0));
    let validate_type_json: Value =
        serde_json::from_slice(&validate_type.stdout).expect("validate-type response");
    let validate_type_schema =
        read_json("docs/agent-tooling/schemas/validate-type-response.schema.json");
    let validate_type_compiled =
        JSONSchema::compile(&validate_type_schema).expect("compile validate-type schema");
    assert!(
        validate_type_compiled.validate(&validate_type_json).is_ok(),
        "validate-type response does not satisfy schema"
    );

    let suggest = run_aic(&[
        "suggest",
        "--partial",
        "add",
        "--project",
        project_path,
        "--limit",
        "5",
    ]);
    assert_eq!(suggest.status.code(), Some(0));
    let suggest_json: Value = serde_json::from_slice(&suggest.stdout).expect("suggest response");
    let suggest_schema = read_json("docs/agent-tooling/schemas/suggest-response.schema.json");
    let suggest_compiled = JSONSchema::compile(&suggest_schema).expect("compile suggest schema");
    assert!(
        suggest_compiled.validate(&suggest_json).is_ok(),
        "suggest response does not satisfy schema"
    );
}

#[test]
fn query_and_symbols_json_validate_against_published_schemas() {
    let project_path = repo_root().join("examples/e7/symbol_query");
    let project_str = project_path.to_str().expect("project path");

    let query = run_aic(&[
        "query",
        "--project",
        project_str,
        "--kind",
        "function",
        "--name",
        "validate*",
        "--module",
        "demo.search",
        "--effects",
        "io",
        "--has-contract",
        "--generic-over",
        "T",
        "--limit",
        "10",
        "--json",
    ]);
    assert_eq!(query.status.code(), Some(0));
    let query_json: Value = serde_json::from_slice(&query.stdout).expect("query response");
    let query_schema = read_json("docs/agent-tooling/schemas/query-response.schema.json");
    let query_compiled = JSONSchema::compile(&query_schema).expect("compile query schema");
    assert!(
        query_compiled.validate(&query_json).is_ok(),
        "query response does not satisfy schema"
    );

    let symbols = run_aic(&["symbols", "--project", project_str, "--json"]);
    assert_eq!(symbols.status.code(), Some(0));
    let symbols_json: Value = serde_json::from_slice(&symbols.stdout).expect("symbols response");
    let symbols_schema = read_json("docs/agent-tooling/schemas/symbols-response.schema.json");
    let symbols_compiled = JSONSchema::compile(&symbols_schema).expect("compile symbols schema");
    assert!(
        symbols_compiled.validate(&symbols_json).is_ok(),
        "symbols response does not satisfy schema"
    );

    let invalid = run_aic(&[
        "query",
        "--project",
        project_str,
        "--kind",
        "function",
        "--has-invariant",
        "--json",
    ]);
    assert_eq!(invalid.status.code(), Some(2));
    let invalid_json: Value = serde_json::from_slice(&invalid.stdout).expect("invalid query json");
    assert!(
        query_compiled.validate(&invalid_json).is_ok(),
        "invalid query response does not satisfy schema"
    );
    assert_eq!(invalid_json["ok"], false);
    assert_eq!(
        invalid_json["error"]["code"],
        "unsupported_filter_combination"
    );
}

#[test]
fn context_json_validates_against_published_schema() {
    let project_path = repo_root().join("examples/e7/context_query");
    let project_str = project_path.to_str().expect("project path");

    let context = run_aic(&[
        "context",
        "--project",
        project_str,
        "--for",
        "function",
        "process_user",
        "--depth",
        "2",
        "--limit",
        "3",
        "--json",
    ]);
    assert_eq!(context.status.code(), Some(0));
    let context_json: Value = serde_json::from_slice(&context.stdout).expect("context response");
    let context_schema = read_json("docs/agent-tooling/schemas/context-response.schema.json");
    let context_compiled = JSONSchema::compile(&context_schema).expect("compile context schema");
    assert!(
        context_compiled.validate(&context_json).is_ok(),
        "context response does not satisfy schema"
    );
}

#[test]
fn documented_protocol_fixtures_smoke_against_cli() {
    let check_fixture = read_json("examples/agent/protocol_check.json");
    assert_eq!(
        check_fixture["diagnostics"][0]["reasoning"]["strategy"],
        "parser-missing-semicolon"
    );
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
    assert_eq!(check_json[0]["reasoning"]["schema_version"], "1.0");

    let context_fixture = read_json("examples/agent/protocol_context.json");
    let context = run_aic(&[
        "context",
        "--project",
        "examples/e7/context_query",
        "--for",
        "function",
        "process_user",
        "--depth",
        "2",
        "--limit",
        "3",
        "--json",
    ]);
    assert_eq!(context.status.code(), Some(0));
    let context_json: Value = serde_json::from_slice(&context.stdout).expect("context json");
    assert_eq!(context_json["phase"], "context");
    assert_eq!(context_json["signature"], context_fixture["signature"]);
    assert_eq!(
        context_json["target"]["name"],
        context_fixture["target"]["name"]
    );
    assert_eq!(context_json["limit"], context_fixture["limit"]);
    assert_eq!(
        context_json["dependencies"][0]["name"],
        context_fixture["dependencies"][0]["name"]
    );

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
    assert_eq!(
        build_error_fixture["diagnostics"][0]["reasoning"]["strategy"],
        "missing-effects"
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

    let testgen_fixture = read_json("examples/agent/protocol_testgen.json");
    let testgen = run_aic(&[
        "testgen",
        "--strategy",
        testgen_fixture["strategy"]
            .as_str()
            .expect("testgen strategy"),
        "--for",
        testgen_fixture["target"]["kind"]
            .as_str()
            .expect("testgen target kind"),
        testgen_fixture["target"]["name"]
            .as_str()
            .expect("testgen target name"),
        "--project",
        "examples/e7/spec_first",
        "--json",
    ]);
    assert_eq!(testgen.status.code(), Some(0));
    let testgen_json: Value = serde_json::from_slice(&testgen.stdout).expect("testgen json");
    assert_eq!(testgen_json["phase"], "testgen");
    assert_eq!(testgen_json["strategy"], testgen_fixture["strategy"]);
    assert_eq!(
        testgen_json["target"]["name"],
        testgen_fixture["target"]["name"]
    );
}

#[test]
fn documented_query_and_symbols_fixtures_smoke_against_cli() {
    let query_fixture = read_json("examples/agent/protocol_query.json");
    let query = run_aic(&[
        "query",
        "--project",
        query_fixture["project_root"]
            .as_str()
            .expect("query fixture root"),
        "--kind",
        query_fixture["filters"]["kind"]
            .as_str()
            .expect("query fixture kind"),
        "--name",
        query_fixture["filters"]["name"]
            .as_str()
            .expect("query fixture name"),
        "--module",
        query_fixture["filters"]["module"]
            .as_str()
            .expect("query fixture module"),
        "--effects",
        query_fixture["filters"]["effects"][0]
            .as_str()
            .expect("query fixture effect"),
        "--has-contract",
        "--generic-over",
        query_fixture["filters"]["generic_over"]
            .as_str()
            .expect("query fixture generic"),
        "--limit",
        &query_fixture["filters"]["limit"].to_string(),
        "--json",
    ]);
    assert_eq!(query.status.code(), Some(0));
    let query_json: Value = serde_json::from_slice(&query.stdout).expect("query json");
    assert_eq!(
        query_json["symbols"][0]["name"],
        query_fixture["symbols"][0]["name"]
    );
    assert_eq!(
        query_json["symbols"][0]["contracts"]["requires"],
        query_fixture["symbols"][0]["contracts"]["requires"]
    );

    let symbols_fixture = read_json("examples/agent/protocol_symbols.json");
    let symbols = run_aic(&[
        "symbols",
        "--project",
        symbols_fixture["project_root"]
            .as_str()
            .expect("symbols fixture root"),
        "--json",
    ]);
    assert_eq!(symbols.status.code(), Some(0));
    let symbols_json: Value = serde_json::from_slice(&symbols.stdout).expect("symbols json");
    assert_eq!(
        symbols_json["symbol_count"],
        symbols_fixture["symbol_count"]
    );
    assert_eq!(
        symbols_json["symbols"][0]["name"],
        symbols_fixture["symbols"][0]["name"]
    );
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
        "docs/agent-tooling/schemas/testgen-response.schema.json",
        "docs/agent-tooling/schemas/validate-call-response.schema.json",
        "docs/agent-tooling/schemas/validate-type-response.schema.json",
        "docs/agent-tooling/schemas/suggest-response.schema.json",
        "examples/agent/protocol_testgen.json",
        "examples/agent/protocol_validate_call.json",
        "examples/agent/protocol_validate_type.json",
        "examples/agent/protocol_suggest.json",
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
