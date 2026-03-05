use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use aicore::conformance::{load_catalog, run_catalog};
use serde::Deserialize;
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

fn has_code(output: &[u8], code: &str) -> bool {
    let diagnostics: serde_json::Value =
        serde_json::from_slice(output).expect("parse diagnostics json");
    diagnostics
        .as_array()
        .expect("diagnostics array")
        .iter()
        .any(|item| item.get("code").and_then(|value| value.as_str()) == Some(code))
}

fn assert_case_has_required_fields(case: &serde_json::Value, fields: &[&str]) {
    for field in fields {
        let value = case
            .get(field)
            .and_then(|entry| entry.as_str())
            .unwrap_or("");
        assert!(
            !value.trim().is_empty(),
            "matrix case missing required field `{field}`: {case:#?}"
        );
    }
}

#[derive(Debug, Deserialize)]
struct RestConformanceMatrix {
    schema_version: i64,
    suite: String,
    scenarios: Vec<RestConformanceScenario>,
}

#[derive(Debug, Deserialize)]
struct RestConformanceScenario {
    id: String,
    category: String,
    description: String,
    program: String,
    expect_exit_code: i32,
    expect_stdout: String,
}

fn load_rest_conformance_matrix() -> RestConformanceMatrix {
    let root = repo_root();
    let matrix_path = root.join("tests/integration/rest-conformance.matrix.json");
    let raw = fs::read_to_string(&matrix_path).expect("read rest conformance matrix");
    serde_json::from_str(&raw).expect("parse rest conformance matrix")
}

fn run_rest_conformance_source(source: &str) -> std::process::Output {
    let tmp = tempdir().expect("tempdir");
    let source_path = tmp.path().join("rest_conformance_case.aic");
    fs::write(&source_path, source).expect("write rest conformance source");
    Command::new(env!("CARGO_BIN_EXE_aic"))
        .arg("run")
        .arg(
            source_path
                .to_str()
                .expect("rest conformance source path utf-8"),
        )
        .current_dir(repo_root())
        .output()
        .expect("run rest conformance source")
}

fn rest_conformance_source(program: &str) -> Option<&'static str> {
    match program {
        "http_valid_parse_route_json_response" => Some(
            r#"
import std.io;
import std.net;
import std.http_server;
import std.router;
import std.map;
import std.string;
import std.json;
import std.bytes;

fn decode_lookup_ok(value: JsonValue) -> Int {
    match json.decode_string(value) {
        Ok(text) => if string.contains(text, "lookup") && len(text) == 6 { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn lookup_field_ok(found: Option[JsonValue]) -> Int {
    match found {
        Some(value) => decode_lookup_ok(value),
        None => 0,
    }
}

fn json_object_ok(value: JsonValue) -> Int {
    match json.object_get(value, "op") {
        Ok(found) => lookup_field_ok(found),
        Err(_) => 0,
    }
}

fn json_payload_ok(body: String) -> Int {
    match json.parse(body) {
        Ok(value) => json_object_ok(value),
        Err(_) => 0,
    }
}

fn route_id(router: Router, method: String, path: String) -> Int {
    match match_route(router, method, path) {
        Ok(found) => match found {
            Some(value) => value.route_id,
            None => 0,
        },
        Err(_) => -1,
    }
}

fn route_param_or_empty(router: Router, method: String, path: String, key: String) -> String {
    match match_route(router, method, path) {
        Ok(found) => match found {
            Some(value) => match map.get(value.params, key) {
                Some(v) => v,
                None => "",
            },
            None => "",
        },
        Err(_) => "",
    }
}

fn request_ok(router: Router, req: Request) -> Int {
    let route_value = route_id(router, req.method, req.path);
    let id = route_param_or_empty(router, req.method, req.path, "id");
    let route_ok = if route_value == 200 && string.contains(id, "42") && len(id) == 2 {
        1
    } else {
        0
    };
    let json_ok = json_payload_ok(req.body);
    if route_ok == 1 && json_ok == 1 {
        1
    } else {
        0
    }
}

fn main() -> Int effects { io, net } capabilities { io, net } {
    let router0 = match new_router() {
        Ok(value) => value,
        Err(_) => Router { handle: 0 },
    };
    let router1 = match add(router0, "POST", "/v1/users/:id", 200) {
        Ok(value) => value,
        Err(_) => router0,
    };

    let listener = match listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(v) => v,
        Err(_) => "",
    };
    let client = match tcp_connect(addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let request_wire = "POST /v1/users/42 HTTP/1.1\r\nHost: localhost\r\nContent-Length: 15\r\n\r\n{\"op\":\"lookup\"}";
    let sent = match tcp_send(client, bytes.from_string(request_wire)) {
        Ok(n) => n,
        Err(_) => 0,
    };

    let parsed_ok = match read_request(server, 4096, 1000) {
        Ok(req) => request_ok(router1, req),
        Err(_) => 0,
    };

    let wrote = match write_response(server, json_response(200, "{\"ok\":true}")) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let wire = match tcp_recv(client, 1024, 1000) {
        Ok(v) => v,
        Err(_) => bytes.empty(),
    };
    let wire_text = bytes.to_string_lossy(wire);
    let response_ok = if string.contains(wire_text, "HTTP/1.1 200 OK") &&
        string.contains(wire_text, "{\"ok\":true}") {
        1
    } else {
        0
    };

    let closed_client = match tcp_close(client) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_server = match close(server) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_listener = match close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if sent > 0 && parsed_ok == 1 && wrote > 0 && response_ok == 1 &&
        closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#,
        ),
        "http_malformed_method_typed_error" => Some(
            r#"
import std.io;
import std.net;
import std.http_server;
import std.bytes;

fn err_code(err: ServerError) -> Int {
    match err {
        InvalidRequest => 1,
        InvalidMethod => 2,
        InvalidHeader => 3,
        InvalidTarget => 4,
        Timeout => 5,
        ConnectionClosed => 6,
        BodyTooLarge => 7,
        Net => 8,
        Internal => 9,
    }
}

fn main() -> Int effects { io, net } capabilities { io, net } {
    let listener = match listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(v) => v,
        Err(_) => "",
    };
    let client = match tcp_connect(addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    tcp_send(client, bytes.from_string("BREW /coffee HTTP/1.1\nHost: localhost\n\n"));
    let code = match read_request(server, 4096, 1000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let closed_client = match tcp_close(client) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_server = match close(server) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_listener = match close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if code == 2 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#,
        ),
        "router_precedence_and_param_extraction" => Some(
            r#"
import std.io;
import std.map;
import std.router;
import std.string;

fn str_eq(left: String, right: String) -> Int {
    if len(left) == len(right) && string.contains(left, right) {
        1
    } else {
        0
    }
}

fn route_id(router: Router, method: String, path: String) -> Int {
    match match_route(router, method, path) {
        Ok(found) => match found {
            Some(value) => value.route_id,
            None => 0,
        },
        Err(_) => -1,
    }
}

fn route_param_or_empty(router: Router, method: String, path: String, key: String) -> String {
    match match_route(router, method, path) {
        Ok(found) => match found {
            Some(value) => match map.get(value.params, key) {
                Some(v) => v,
                None => "",
            },
            None => "",
        },
        Err(_) => "",
    }
}

fn main() -> Int effects { io } capabilities { io } {
    let router0 = match new_router() {
        Ok(value) => value,
        Err(_) => Router { handle: 0 },
    };
    let router1 = match add(router0, "GET", "/users/me", 10) {
        Ok(value) => value,
        Err(_) => router0,
    };
    let router2 = match add(router1, "GET", "/users/:id", 20) {
        Ok(value) => value,
        Err(_) => router1,
    };
    let router3 = match add(router2, "GET", "/users/*", 30) {
        Ok(value) => value,
        Err(_) => router2,
    };
    let router4 = match add(router3, "*", "/users/*", 40) {
        Ok(value) => value,
        Err(_) => router3,
    };

    let static_ok = if route_id(router4, "GET", "/users/me") == 10 { 1 } else { 0 };
    let param_ok = if route_id(router4, "GET", "/users/42") == 20 &&
        str_eq(route_param_or_empty(router4, "GET", "/users/42", "id"), "42") == 1 {
        1
    } else {
        0
    };
    let wildcard_ok = if route_id(router4, "GET", "/users/42/profile") == 30 &&
        len(route_param_or_empty(router4, "GET", "/users/42/profile", "id")) == 0 {
        1
    } else {
        0
    };
    let method_fallback_ok = if route_id(router4, "POST", "/users/42/profile") == 40 {
        1
    } else {
        0
    };

    if static_ok + param_ok + wildcard_ok + method_fallback_ok == 4 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#,
        ),
        "json_malformed_payload_typed_error" => Some(
            r#"
import std.io;
import std.net;
import std.http_server;
import std.json;
import std.bytes;

fn json_err_code(err: JsonError) -> Int {
    match err {
        InvalidJson => 1,
        InvalidType => 2,
        MissingField => 3,
        InvalidNumber => 4,
        InvalidString => 5,
        InvalidInput => 6,
        Internal => 7,
    }
}

fn main() -> Int effects { io, net } capabilities { io, net } {
    let listener = match listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(v) => v,
        Err(_) => "",
    };
    let client = match tcp_connect(addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    tcp_send(client, bytes.from_string("POST /json HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\n{\"x\":"));
    let code = match read_request(server, 4096, 1000) {
        Ok(req) => match json.parse(req.body) {
            Ok(_) => 0,
            Err(err) => json_err_code(err),
        },
        Err(_) => 0,
    };

    let closed_client = match tcp_close(client) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_server = match close(server) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_listener = match close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if code == 1 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#,
        ),
        "async_lifecycle_timeout_and_shutdown" => Some(
            r#"
import std.io;
import std.net;

fn net_code(err: NetError) -> Int {
    match err {
        Timeout => 4,
        ConnectionClosed => 8,
        Cancelled => 9,
        InvalidInput => 6,
        _ => 7,
    }
}

async fn main() -> Int effects { io, net, concurrency } capabilities { io, net, concurrency } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };

    let accepted = await async_accept_submit(listener, 250);
    let timeout_code = match accepted {
        Ok(_) => 0,
        Err(err) => net_code(err),
    };
    let shutdown_ok = match async_shutdown() {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed = match tcp_close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if timeout_code == 4 && shutdown_ok == 1 && closed == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#,
        ),
        "typed_error_mapping_stability" => Some(
            r#"
import std.io;
import std.net;
import std.http_server;
import std.router;
import std.json;
import std.bytes;

fn server_err_code(err: ServerError) -> Int {
    match err {
        InvalidRequest => 1,
        InvalidMethod => 2,
        InvalidHeader => 3,
        InvalidTarget => 4,
        Timeout => 5,
        ConnectionClosed => 6,
        BodyTooLarge => 7,
        Net => 8,
        Internal => 9,
    }
}

fn router_err_code(err: RouterError) -> Int {
    match err {
        InvalidPattern => 1,
        InvalidMethod => 2,
        Capacity => 3,
        Internal => 4,
    }
}

fn json_err_code(err: JsonError) -> Int {
    match err {
        InvalidJson => 1,
        InvalidType => 2,
        MissingField => 3,
        InvalidNumber => 4,
        InvalidString => 5,
        InvalidInput => 6,
        Internal => 7,
    }
}

fn net_err_code(err: NetError) -> Int {
    match err {
        Timeout => 4,
        InvalidInput => 6,
        ConnectionClosed => 8,
        Cancelled => 9,
        _ => 7,
    }
}

fn main() -> Int effects { io, net, concurrency } capabilities { io, net, concurrency } {
    let listener = match listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(v) => v,
        Err(_) => "",
    };
    let client = match tcp_connect(addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    tcp_send(client, bytes.from_string("BREW /typed HTTP/1.1\nHost: localhost\n\n"));
    let http_code = match read_request(server, 4096, 1000) {
        Ok(_) => 0,
        Err(err) => server_err_code(err),
    };

    let router0 = match new_router() {
        Ok(value) => value,
        Err(_) => Router { handle: 0 },
    };
    let router_code = match add(router0, "GET ", "/x", 1) {
        Ok(_) => 0,
        Err(err) => router_err_code(err),
    };

    let json_code = match json.parse("{\"x\":") {
        Ok(_) => 0,
        Err(err) => json_err_code(err),
    };
    let net_code = match async_wait_int(AsyncIntOp { handle: 0 }, 10) {
        Ok(_) => 0,
        Err(err) => net_err_code(err),
    };

    let closed_client = match tcp_close(client) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_server = match close(server) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_listener = match close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if http_code == 2 && router_code == 2 && json_code == 1 && net_code == 6 &&
        closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#,
        ),
        _ => None,
    }
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
        "QV-T6",
        "AGX3-T3",
        "examples/verify/qv_contract_proof_fail.aic",
        "examples/verify/qv_contract_proof_fixed.aic",
        "concurrency-stress-replay.md",
        "rest-conformance-matrix.md",
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
    assert!(perf.contains("rest-runtime-soak-gate.v1.json"));
    assert!(perf.contains("rest-runtime-soak-report.json"));

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
        "Upload E8 REST runtime soak report",
        "e8-rest-runtime-soak-linux",
        "Run host REST runtime perf/soak gate suite",
        "make test-e8-rest-runtime-soak",
        "rest-runtime-soak-report.json",
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

#[test]
fn integration_harness_contract_and_wiring_are_enforced() {
    let root = repo_root();
    let matrix_path = root.join("tests/integration/protocol-harness.matrix.json");
    let matrix: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&matrix_path).expect("read protocol harness matrix"),
    )
    .expect("parse protocol harness matrix");

    assert_eq!(
        matrix
            .get("schema_version")
            .and_then(|value| value.as_i64()),
        Some(1),
        "matrix schema_version must be pinned to 1"
    );

    let offline_cases = matrix
        .get("offline_cases")
        .and_then(|value| value.as_array())
        .expect("offline_cases array");
    assert!(
        !offline_cases.is_empty(),
        "offline_cases must include at least one replay gate"
    );
    for case in offline_cases {
        assert_case_has_required_fields(
            case,
            &["id", "service", "version", "auth", "security", "command"],
        );
    }

    let live_cases = matrix
        .get("live_cases")
        .and_then(|value| value.as_array())
        .expect("live_cases array");
    assert!(
        !live_cases.is_empty(),
        "live_cases must include at least one container smoke profile"
    );
    for case in live_cases {
        assert_case_has_required_fields(
            case,
            &[
                "id",
                "service",
                "version",
                "auth",
                "security",
                "compose_file",
                "healthcheck_cmd",
                "smoke_cmd",
            ],
        );
    }

    let makefile = fs::read_to_string(root.join("Makefile")).expect("read Makefile");
    for token in [
        "integration-harness-offline",
        "integration-harness-live",
        "scripts/ci/integration-harness.py --mode offline",
        "scripts/ci/integration-harness.py --mode live",
    ] {
        assert!(makefile.contains(token), "Makefile missing token: {token}");
    }

    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci workflow");
    for token in [
        "Integration harness offline replay gate",
        "make integration-harness-offline",
        "Integration harness live smoke (opt-in)",
        "AIC_ENABLE_INTEGRATION_LIVE",
        "integration-harness-linux",
        "target/e8/integration-harness-report.json",
    ] {
        assert!(ci.contains(token), "ci workflow missing token: {token}");
    }

    let docs = fs::read_to_string(root.join("docs/io-runtime/integration-harness.md"))
        .expect("read harness doc");
    for token in [
        "offline_cases",
        "live_cases",
        "AIC_INTEGRATION_LIVE=1",
        "protocol-harness.matrix.json",
        "External Client Library Plug-In Path",
    ] {
        assert!(
            docs.contains(token),
            "integration harness doc missing token: {token}"
        );
    }
}

#[test]
fn integration_harness_live_mode_requires_explicit_opt_in() {
    let root = repo_root();
    let report_path = root.join("target/e8/integration-harness-live-optin-report.json");
    let _ = fs::remove_file(&report_path);

    let output = Command::new("python3")
        .arg("scripts/ci/integration-harness.py")
        .arg("--mode")
        .arg("live")
        .arg("--max-cases")
        .arg("1")
        .arg("--report")
        .arg(
            report_path
                .to_str()
                .expect("integration harness report path utf-8"),
        )
        .current_dir(&root)
        .env_remove("AIC_INTEGRATION_LIVE")
        .output()
        .expect("run integration harness live mode without opt-in");

    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read harness report"))
            .expect("parse harness report");
    assert_eq!(
        report.get("mode").and_then(|value| value.as_str()),
        Some("live")
    );
    assert_eq!(
        report.get("status").and_then(|value| value.as_str()),
        Some("skipped")
    );
    assert_eq!(
        report.get("reason").and_then(|value| value.as_str()),
        Some("set AIC_INTEGRATION_LIVE=1 to enable live container integration runs")
    );

    let _ = fs::remove_file(report_path);
}

#[test]
fn integration_harness_defaults_std_root_for_offline_cases() {
    let root = repo_root();
    let tmp = tempdir().expect("tempdir");
    let marker_path = tmp.path().join("std-root.txt");
    let report_path = tmp.path().join("integration-harness-report.json");
    let matrix_path = tmp.path().join("protocol-harness.matrix.json");

    let command = format!(
        "python3 -c \"import os,pathlib; pathlib.Path(r'{}').write_text(os.environ.get('AIC_STD_ROOT', ''))\"",
        marker_path.display()
    );
    let matrix = serde_json::json!({
        "schema_version": 1,
        "offline_cases": [
            {
                "id": "std-root-default-check",
                "service": "contract",
                "version": "1",
                "auth": "none",
                "security": "n/a",
                "command": command,
            }
        ],
        "live_cases": [],
    });
    fs::write(
        &matrix_path,
        serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    )
    .expect("write matrix");

    let output = Command::new("python3")
        .arg("scripts/ci/integration-harness.py")
        .arg("--mode")
        .arg("offline")
        .arg("--matrix")
        .arg(
            matrix_path
                .to_str()
                .expect("protocol harness matrix path utf-8"),
        )
        .arg("--report")
        .arg(
            report_path
                .to_str()
                .expect("integration harness report path utf-8"),
        )
        .current_dir(&root)
        .env_remove("AIC_STD_ROOT")
        .output()
        .expect("run integration harness offline mode");

    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read harness report"))
            .expect("parse harness report");
    assert_eq!(
        report.get("status").and_then(|value| value.as_str()),
        Some("passed")
    );

    let observed = fs::read_to_string(&marker_path).expect("read marker");
    assert_eq!(observed, root.join("std").to_string_lossy());
}

#[test]
fn rest_conformance_matrix_contract_is_enforced() {
    let matrix = load_rest_conformance_matrix();
    assert_eq!(
        matrix.schema_version, 1,
        "rest conformance matrix schema version must be pinned to 1"
    );
    assert_eq!(
        matrix.suite, "rest-runtime-conformance",
        "rest conformance matrix suite id changed unexpectedly"
    );
    assert!(
        !matrix.scenarios.is_empty(),
        "rest conformance matrix must include scenarios"
    );

    let required_ids = [
        "REST-CONF-001-HTTP-VALID-E2E",
        "REST-CONF-002-HTTP-MALFORMED-METHOD",
        "REST-CONF-003-ROUTER-PRECEDENCE-PARAMS",
        "REST-CONF-004-JSON-MALFORMED-PAYLOAD",
        "REST-CONF-005-ASYNC-LIFECYCLE",
        "REST-CONF-006-TYPED-ERROR-MAPPING",
    ];
    let required_categories = [
        "parse-route-json-response",
        "http-parse-negative",
        "router-precedence-params",
        "json-negative",
        "async-lifecycle",
        "typed-error-mapping",
    ];

    let mut seen_ids = HashSet::new();
    let mut seen_categories = HashSet::new();
    for scenario in &matrix.scenarios {
        assert!(
            !scenario.id.trim().is_empty(),
            "rest conformance scenario id must be non-empty: {scenario:#?}"
        );
        assert!(
            !scenario.category.trim().is_empty(),
            "rest conformance scenario category must be non-empty: {scenario:#?}"
        );
        assert!(
            !scenario.description.trim().is_empty(),
            "rest conformance scenario description must be non-empty: {scenario:#?}"
        );
        assert!(
            !scenario.program.trim().is_empty(),
            "rest conformance scenario program key must be non-empty: {scenario:#?}"
        );
        assert!(
            !scenario.expect_stdout.is_empty(),
            "rest conformance scenario expected stdout must be non-empty: {scenario:#?}"
        );
        assert!(
            seen_ids.insert(scenario.id.as_str()),
            "rest conformance scenario ids must be unique: {}",
            scenario.id
        );
        seen_categories.insert(scenario.category.as_str());
    }

    for id in required_ids {
        assert!(
            seen_ids.contains(id),
            "rest conformance matrix missing required scenario id `{id}`"
        );
    }
    for category in required_categories {
        assert!(
            seen_categories.contains(category),
            "rest conformance matrix missing required category `{category}`"
        );
    }

    let root = repo_root();
    let docs =
        fs::read_to_string(root.join("docs/verification-quality/rest-conformance-matrix.md"))
            .expect("read rest conformance matrix docs");
    for token in [
        "tests/integration/rest-conformance.matrix.json",
        "Expected deterministic outcome",
        "cargo test --locked --test e8_conformance_tests",
    ] {
        assert!(
            docs.contains(token),
            "rest conformance matrix doc missing token: {token}"
        );
    }
    for id in required_ids {
        assert!(
            docs.contains(id),
            "rest conformance matrix docs missing scenario id `{id}`"
        );
    }

    let readme = fs::read_to_string(root.join("docs/verification-quality/README.md"))
        .expect("read verification quality README");
    assert!(
        readme.contains("rest-conformance-matrix.md"),
        "verification-quality README must link rest conformance matrix runbook"
    );
}

#[cfg(not(target_os = "windows"))]
#[test]
fn rest_conformance_harness_runs_matrix_scenarios() {
    let matrix = load_rest_conformance_matrix();
    for scenario in &matrix.scenarios {
        let source = rest_conformance_source(&scenario.program).unwrap_or_else(|| {
            panic!("unknown rest conformance program key: {}", scenario.program)
        });
        let output = run_rest_conformance_source(source);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        assert_eq!(
            output.status.code(),
            Some(scenario.expect_exit_code),
            "scenario={} expected_exit={} actual_exit={:?} stdout={stdout:?} stderr={stderr}",
            scenario.id,
            scenario.expect_exit_code,
            output.status.code()
        );
        assert_eq!(
            stdout, scenario.expect_stdout,
            "scenario={} expected_stdout={:?} actual_stdout={stdout:?} stderr={stderr}",
            scenario.id, scenario.expect_stdout
        );
    }
}
