use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use aicore::diagnostics::{Diagnostic, DiagnosticSpan, Severity};
use aicore::driver::sort_and_cap_diagnostics;
use aicore::telemetry::read_events;
use serde_json::{json, Value};
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

fn run_aic_in_dir_with_env(
    cwd: &std::path::Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aic"));
    command.args(args).current_dir(cwd);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run aic in dir with env")
}

fn run_aic_with_env(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aic"));
    command.args(args).current_dir(repo_root());
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run aic with env")
}

fn run_repl_session(args: &[&str], input: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(args)
        .current_dir(repo_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn repl");

    {
        let stdin = child.stdin.as_mut().expect("repl stdin");
        stdin.write_all(input.as_bytes()).expect("write repl input");
        stdin.flush().expect("flush repl input");
    }
    child.wait_with_output().expect("wait repl output")
}

fn normalize_help_snapshot(text: &str) -> String {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn write_many_check_diagnostics_fixture() -> (tempfile::TempDir, String) {
    let project = tempdir().expect("project");
    fs::create_dir_all(project.path().join("src")).expect("mkdir src");
    let source_path = project.path().join("src/main.aic");

    let mut program = String::from("module many.errors;\nfn main() -> Int {\n");
    for i in 0..25 {
        program.push_str(&format!("    let x{i} = not_defined_{i}();\n"));
    }
    program.push_str("    0\n}\n");
    fs::write(&source_path, program).expect("write many errors source");

    (project, source_path.to_string_lossy().to_string())
}

fn write_profile_demo_project(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/main.aic"),
        concat!(
            "module profile.demo;\n",
            "import std.io;\n",
            "fn main() -> Int effects { io } capabilities { io } {\n",
            "    print_int(7);\n",
            "    0\n",
            "}\n",
        ),
    )
    .expect("write profile demo source");
}

fn write_build_opt_project(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/main.aic"),
        concat!(
            "module build.opt;\n",
            "fn mix(x: Int, y: Int) -> Int {\n",
            "    ((x * 31) + y) ^ (y * 17)\n",
            "}\n",
            "fn main() -> Int {\n",
            "    let mut i = 0;\n",
            "    let mut acc = 1;\n",
            "    while i < 50000 {\n",
            "        acc = mix(acc, i);\n",
            "        i = i + 1;\n",
            "    };\n",
            "    acc\n",
            "}\n",
        ),
    )
    .expect("write build opt source");
}

fn read_clang_opt_level(telemetry_path: &std::path::Path) -> String {
    let events = read_events(telemetry_path).expect("read telemetry events");
    events
        .into_iter()
        .find_map(|event| {
            if event.kind == "phase"
                && event.command == "codegen"
                && event.phase.as_deref() == Some("clang_compile")
                && event.status.as_deref() == Some("ok")
            {
                event
                    .attrs
                    .get("opt_level")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string)
            } else {
                None
            }
        })
        .expect("clang compile telemetry event with opt_level")
}
fn write_leak_clean_project(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/main.aic"),
        concat!(
            "module leak.clean;\n",
            "fn main() -> Int {\n",
            "    0\n",
            "}\n",
        ),
    )
    .expect("write leak clean source");
}

fn write_leak_positive_project(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("src/main.aic"),
        concat!(
            "module leak.positive;\n",
            "fn main() -> Int {\n",
            "    let offset = 1;\n",
            "    let plus_offset = |x: Int| -> Int { x + offset };\n",
            "    let out = plus_offset(41);\n",
            "    if out == 42 { 0 } else { 1 }\n",
            "}\n",
        ),
    )
    .expect("write leak-positive source");
}

fn write_bench_fixture(root: &std::path::Path) -> (PathBuf, String) {
    let dataset_rel = "benchdata";
    let dataset_dir = root.join(dataset_rel);
    fs::create_dir_all(&dataset_dir).expect("mkdir bench dataset");

    let mut source = String::from("module bench.demo;\n");
    for index in 0..220 {
        source.push_str(&format!("fn value_{index}() -> Int {{ {index} }}\n"));
    }
    source.push_str("fn main() -> Int {\n    value_219()\n}\n");
    fs::write(dataset_dir.join("main.aic"), source).expect("write bench source");

    let budget_path = root.join("budget.json");
    fs::write(
        &budget_path,
        format!(
            concat!(
                "{{\n",
                "  \"dataset\": \"{dataset_rel}\",\n",
                "  \"iterations\": 1,\n",
                "  \"parser_ms_max\": 10000.0,\n",
                "  \"typecheck_ms_max\": 10000.0,\n",
                "  \"codegen_ms_max\": 10000.0,\n",
                "  \"regression_tolerance_pct\": 10.0\n",
                "}}\n"
            ),
            dataset_rel = dataset_rel
        ),
    )
    .expect("write bench budget");

    (budget_path, dataset_rel.to_string())
}

fn make_diag(code: &str, message: &str, file: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message: message.to_string(),
        spans: vec![DiagnosticSpan {
            file: file.to_string(),
            start,
            end,
            label: Some("label".to_string()),
        }],
        help: vec!["help".to_string()],
        suggested_fixes: vec![],
    }
}

struct DaemonHarness {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl DaemonHarness {
    fn spawn(cwd: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_aic"))
            .arg("daemon")
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn daemon");
        let stdin = child.stdin.take().expect("daemon stdin");
        let stdout = BufReader::new(child.stdout.take().expect("daemon stdout"));
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn request(&mut self, payload: &Value) -> Value {
        let encoded = serde_json::to_string(payload).expect("encode request");
        self.stdin
            .write_all(encoded.as_bytes())
            .expect("write request");
        self.stdin.write_all(b"\n").expect("newline");
        self.stdin.flush().expect("flush request");

        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .expect("read daemon response");
        assert!(read > 0, "daemon closed unexpectedly");
        serde_json::from_str(&line).expect("decode daemon response")
    }

    fn shutdown(&mut self) {
        let _ = self.request(&json!({
            "jsonrpc": "2.0",
            "id": 99_999,
            "method": "shutdown",
            "params": {}
        }));
        let status = self.child.wait().expect("wait daemon");
        assert!(status.success(), "daemon exited with non-zero status");
    }
}

#[test]
fn cli_help_snapshots_are_stable() {
    let main_help = run_aic(&["--help"]);
    assert!(main_help.status.success());
    let main_help_text = String::from_utf8_lossy(&main_help.stdout);
    assert!(main_help_text.contains("Usage: aic <COMMAND>"));
    for command in [
        "init",
        "setup",
        "check",
        "ast",
        "impact",
        "suggest-effects",
        "coverage",
        "metrics",
        "bench",
        "diag",
        "explain",
        "fmt",
        "ir",
        "migrate",
        "build",
        "verify-intrinsics",
        "lsp",
        "daemon",
        "repl",
        "test",
        "grammar",
        "contract",
        "diff",
        "release",
        "run",
    ] {
        assert!(
            main_help_text.contains(command),
            "missing `{command}` in help output:\n{main_help_text}"
        );
    }

    let check_help = run_aic(&["check", "--help"]);
    assert!(check_help.status.success());
    let check_help_text = String::from_utf8_lossy(&check_help.stdout);
    assert!(check_help_text.contains("Usage: aic check [OPTIONS] [INPUT]"));
    for flag in [
        "--json",
        "--sarif",
        "--show-holes",
        "--offline",
        "--warn-unused",
        "--max-errors <N>",
    ] {
        assert!(
            check_help_text.contains(flag),
            "missing `{flag}` in check help:\n{check_help_text}"
        );
    }
    assert!(
        check_help_text.contains("[default: 20]"),
        "missing default in check help:\n{check_help_text}"
    );

    let ast_help = run_aic(&["ast", "--help"]);
    assert!(ast_help.status.success());
    let ast_help_text = String::from_utf8_lossy(&ast_help.stdout);
    assert!(ast_help_text.contains("Usage: aic ast"));
    for flag in ["--json", "--offline"] {
        assert!(
            ast_help_text.contains(flag),
            "missing `{flag}` in ast help:\n{ast_help_text}"
        );
    }

    let verify_intrinsics_help = run_aic(&["verify-intrinsics", "--help"]);
    assert!(verify_intrinsics_help.status.success());
    let verify_intrinsics_help_text = String::from_utf8_lossy(&verify_intrinsics_help.stdout);
    assert!(
        verify_intrinsics_help_text.contains("Usage: aic verify-intrinsics [OPTIONS] [INPUT]"),
        "verify-intrinsics help mismatch:\n{verify_intrinsics_help_text}"
    );
    assert!(
        verify_intrinsics_help_text.contains("--json"),
        "verify-intrinsics help missing --json:\n{verify_intrinsics_help_text}"
    );

    let bench_help = run_aic(&["bench", "--help"]);
    assert!(bench_help.status.success());
    let bench_help_text = String::from_utf8_lossy(&bench_help.stdout);
    for flag in [
        "--budget <BUDGET>",
        "--output <OUTPUT>",
        "--compare <BASELINE_JSON>",
    ] {
        assert!(
            bench_help_text.contains(flag),
            "missing `{flag}` in bench help:\n{bench_help_text}"
        );
    }

    let build_help = run_aic(&["build", "--help"]);
    assert!(build_help.status.success());
    let build_help_text = String::from_utf8_lossy(&build_help.stdout);
    for flag in ["--release", "--opt-level <LEVEL>"] {
        assert!(
            build_help_text.contains(flag),
            "missing `{flag}` in build help:\n{build_help_text}"
        );
    }
    let run_help = run_aic(&["run", "--help"]);
    assert!(run_help.status.success());
    let run_help_text = String::from_utf8_lossy(&run_help.stdout);
    for flag in [
        "--profile",
        "--profile-output <PROFILE_OUTPUT>",
        "--check-leaks",
        "--asan",
    ] {
        assert!(
            run_help_text.contains(flag),
            "missing `{flag}` in run help:\n{run_help_text}"
        );
    }

    let test_help = run_aic(&["test", "--help"]);
    assert!(test_help.status.success());
    let test_help_text = String::from_utf8_lossy(&test_help.stdout);
    for flag in [
        "--mode <MODE>",
        "--filter <FILTER>",
        "--seed <N>",
        "--replay <ID_OR_ARTIFACT>",
        "--json",
        "--update-golden",
        "--check-golden",
    ] {
        assert!(
            test_help_text.contains(flag),
            "missing `{flag}` in test help:\n{test_help_text}"
        );
    }
    assert_eq!(
        normalize_help_snapshot(&test_help_text),
        normalize_help_snapshot(include_str!("golden/e7/help_test.txt"))
    );
}

#[test]
fn repl_persists_state_and_supports_meta_commands() {
    let output = run_repl_session(
        &["repl"],
        "let mut x = 41\nx + 1\n:type x + 1\n:effects print_int\nx = 99\nx\n:quit\n",
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("aic repl ready"));
    assert!(text.contains("x = 41 : Int"), "stdout:\n{text}");
    assert!(text.contains("42 : Int"), "stdout:\n{text}");
    assert!(text.contains("\nInt\n"), "stdout:\n{text}");
    assert!(text.contains("print_int effects { io }"), "stdout:\n{text}");
    assert!(text.contains("x = 99 : Int"), "stdout:\n{text}");
    assert!(text.contains("99 : Int"), "stdout:\n{text}");
    assert!(text.contains("bye"), "stdout:\n{text}");
}

#[test]
fn repl_handles_invalid_input_without_crashing() {
    let output = run_repl_session(&["repl"], "1 +\n:type\n:effects\n:unknown\n:quit\n");
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("error:"), "stdout:\n{text}");
    assert!(
        text.contains("missing expression; usage: :type <expr>"),
        "stdout:\n{text}"
    );
    assert!(
        text.contains("missing function name; usage: :effects <fn>"),
        "stdout:\n{text}"
    );
    assert!(
        text.contains("unknown command `:unknown`"),
        "stdout:\n{text}"
    );
    assert!(text.contains("bye"), "stdout:\n{text}");
}

#[test]
fn repl_non_json_history_and_line_editing_work() {
    let output = run_repl_session(
        &["repl"],
        "let x = 7\nx + 1\n!!\n!2\n4\u{8}\u{8}5\n:history\n:quit\n",
    );
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("x = 7 : Int"), "stdout:\n{text}");
    assert!(
        text.matches("8 : Int").count() >= 3,
        "expected history replay to re-run expression; stdout:\n{text}"
    );
    assert!(text.contains("5 : Int"), "stdout:\n{text}");
    assert!(text.contains("1: let x = 7"), "stdout:\n{text}");
    assert!(text.contains("2: x + 1"), "stdout:\n{text}");
    assert!(text.contains("5: 5"), "stdout:\n{text}");
    assert!(text.contains("bye"), "stdout:\n{text}");
}

#[test]
fn repl_json_mode_emits_structured_events() {
    let output = run_repl_session(
        &["repl", "--json"],
        "let n = 5\nn + 2\n:type n\n:effects print_int\nfoo + 1\n:quit\n",
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.len() >= 7,
        "expected structured json events for repl session; stdout:\n{stdout}"
    );

    let ready: Value = serde_json::from_str(lines[0]).expect("ready event json");
    assert_eq!(ready["event"], "ready");
    assert_eq!(ready["mode"], "json");

    let bind: Value = serde_json::from_str(lines[1]).expect("bind event json");
    assert_eq!(bind["event"], "result");
    assert_eq!(bind["binding"], "n");
    assert_eq!(bind["type"], "Int");
    assert_eq!(bind["value"], 5);

    let value: Value = serde_json::from_str(lines[2]).expect("value event json");
    assert_eq!(value["event"], "result");
    assert_eq!(value["type"], "Int");
    assert_eq!(value["value"], 7);

    let type_event: Value = serde_json::from_str(lines[3]).expect("type event json");
    assert_eq!(type_event["event"], "type");
    assert_eq!(type_event["type"], "Int");

    let effects_event: Value = serde_json::from_str(lines[4]).expect("effects event json");
    assert_eq!(effects_event["event"], "effects");
    assert_eq!(effects_event["function"], "print_int");
    assert_eq!(effects_event["effects"], json!(["io"]));

    let error_event: Value = serde_json::from_str(lines[5]).expect("error event json");
    assert_eq!(error_event["event"], "error");
    assert!(
        error_event["message"]
            .as_str()
            .expect("error message")
            .contains("unknown variable"),
        "error event: {error_event:#}"
    );

    let bye_event: Value = serde_json::from_str(lines[6]).expect("bye event json");
    assert_eq!(bye_event["event"], "bye");
}

#[test]
fn diagnostics_are_deduplicated_and_keep_deterministic_capped_prefix() {
    let a = make_diag("E1001", "alpha", "main.aic", 10, 12);
    let b = make_diag("E1001", "alpha", "main.aic", 10, 12);
    let c = make_diag("E1002", "beta", "main.aic", 10, 12);
    let d = make_diag("E2001", "gamma", "main.aic", 30, 33);
    let e = make_diag("E5001", "delta", "main.aic", 2, 3);

    let left = vec![a.clone(), c.clone(), d.clone(), b.clone(), e.clone()];
    let right = vec![d, b, e, c, a];

    let full_left = sort_and_cap_diagnostics(left.clone(), 64);
    let full_right = sort_and_cap_diagnostics(right, 64);
    assert_eq!(
        full_left, full_right,
        "deduplicated ordering should be deterministic across input permutations"
    );
    assert_eq!(
        full_left.len(),
        4,
        "expected one duplicate cascade diagnostic to be removed"
    );

    let capped = sort_and_cap_diagnostics(left, 2);
    assert_eq!(
        capped,
        full_left[..2].to_vec(),
        "expected --max-errors style capping to preserve deterministic sorted prefix"
    );
}

#[test]
fn cli_exit_codes_are_deterministic() {
    let ok = run_aic(&["check", "examples/e7/cli_smoke.aic"]);
    assert_eq!(ok.status.code(), Some(0));

    let diag_fail = run_aic(&["check", "examples/e7/diag_errors.aic"]);
    assert_eq!(diag_fail.status.code(), Some(1));

    let usage_fail = run_aic(&["check", "examples/e7/diag_errors.aic", "--json", "--sarif"]);
    assert_eq!(usage_fail.status.code(), Some(2));
}

#[test]
fn verify_intrinsics_std_runtime_bindings_emit_stable_json() {
    let output = run_aic(&["verify-intrinsics", "std", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout).expect("verify-intrinsics json");
    assert_eq!(report["schema_version"], "1.0");
    assert_eq!(report["ok"], true);
    assert_eq!(report["issue_count"], 0);
    assert!(
        report["files_scanned"].as_u64().unwrap_or(0) >= 35,
        "report={report:#}"
    );
    assert!(
        report["intrinsic_declarations"].as_u64().unwrap_or(0) >= 120,
        "report={report:#}"
    );
    assert_eq!(
        report["verified_bindings"], report["intrinsic_declarations"],
        "report={report:#}"
    );
}

#[test]
fn verify_intrinsics_reports_mapping_and_signature_failures() {
    let fixture = tempdir().expect("fixture");
    let fixture_file = fixture.path().join("intrinsics_bad.aic");
    fs::write(
        &fixture_file,
        concat!(
            "module verify.bad;\n",
            "intrinsic fn aic_proc_spawn_intrinsic(command: Int) -> Result[Int, ProcError] effects { proc, env };\n",
            "intrinsic fn aic_missing_runtime_intrinsic() -> Result[Int, ProcError] effects { proc };\n",
        ),
    )
    .expect("write intrinsic fixture");

    let fixture_path = fixture.path().to_string_lossy().to_string();
    let output = run_aic(&["verify-intrinsics", fixture_path.as_str(), "--json"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout).expect("verify-intrinsics json");
    assert_eq!(report["schema_version"], "1.0");
    assert_eq!(report["ok"], false);
    assert_eq!(report["issue_count"], 2);

    let issues = report["issues"].as_array().expect("issues array");
    let mut kinds = issues
        .iter()
        .filter_map(|issue| issue["kind"].as_str())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    kinds.sort();
    assert_eq!(kinds, vec!["missing_lowering", "signature_mismatch"]);

    let signature_issue = issues
        .iter()
        .find(|issue| issue["kind"] == "signature_mismatch")
        .expect("signature mismatch issue");
    assert_eq!(
        signature_issue["intrinsic"],
        Value::String("aic_proc_spawn_intrinsic".to_string())
    );
    assert_eq!(
        signature_issue["runtime_symbol"],
        Value::String("aic_rt_proc_spawn".to_string())
    );
}

#[test]
fn diagnostics_json_and_sarif_outputs_are_structured() {
    let json_out = run_aic(&["check", "examples/e7/diag_errors.aic", "--json"]);
    assert_eq!(json_out.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&json_out.stdout).expect("diagnostics json");
    assert!(diagnostics.is_array());
    let first = diagnostics
        .as_array()
        .and_then(|v| v.first())
        .expect("at least one diagnostic");
    assert!(first.get("code").is_some());
    assert!(first.get("severity").is_some());
    assert!(first.get("spans").is_some());

    let sarif_out = run_aic(&["diag", "examples/e7/diag_errors.aic", "--sarif"]);
    assert_eq!(sarif_out.status.code(), Some(1));
    let sarif: serde_json::Value = serde_json::from_slice(&sarif_out.stdout).expect("sarif json");
    assert_eq!(sarif["version"], "2.1.0");
    assert!(sarif["runs"][0]["results"].is_array());
    assert!(sarif["runs"][0]["tool"]["driver"]["rules"].is_array());
    assert!(sarif["runs"][0]["results"][0]["ruleId"].is_string());
    assert!(sarif["runs"][0]["results"][0]["locations"].is_array());
}

#[test]
fn sarif_bitwise_bool_type_error_includes_logical_operator_hint() {
    let project = tempdir().expect("project");
    fs::create_dir_all(project.path().join("src")).expect("mkdir src");
    let source_path = project.path().join("src/main.aic");
    fs::write(
        &source_path,
        concat!(
            "module diag.bitwise;\n",
            "fn main() -> Int {\n",
            "    let bad = true & false;\n",
            "    if bad { 1 } else { 0 }\n",
            "}\n",
        ),
    )
    .expect("write source");

    let source = source_path.to_string_lossy().to_string();
    let sarif_out = run_aic(&["diag", &source, "--sarif"]);
    assert_eq!(
        sarif_out.status.code(),
        Some(1),
        "diag stdout={}\ndiag stderr={}",
        String::from_utf8_lossy(&sarif_out.stdout),
        String::from_utf8_lossy(&sarif_out.stderr)
    );
    let sarif: Value = serde_json::from_slice(&sarif_out.stdout).expect("sarif json");
    let results = sarif["runs"][0]["results"]
        .as_array()
        .expect("sarif results");
    let messages: Vec<&str> = results
        .iter()
        .filter_map(|entry| entry["message"]["text"].as_str())
        .collect();
    assert!(
        messages.iter().any(|text| {
            text.contains("operator '&'") && text.contains("requires integer operands")
        }),
        "missing bitwise type error in SARIF messages: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|text| text.contains("use '&&' or '||'")),
        "missing logical-op hint in SARIF messages: {messages:?}"
    );
}

#[test]
fn suggest_effects_reports_transitive_reasons_and_diag_apply_fixes_adds_effects() {
    let project = tempdir().expect("project");
    fs::create_dir_all(project.path().join("src")).expect("mkdir src");
    let source_path = project.path().join("src/main.aic");
    fs::write(
        &source_path,
        concat!(
            "module suggest.effect_inference;\n",
            "import std.io;\n",
            "fn leaf() -> () effects { io } capabilities { io } {\n",
            "    print_int(1)\n",
            "}\n",
            "fn middle() -> () {\n",
            "    leaf()\n",
            "}\n",
            "fn top() -> Int {\n",
            "    middle();\n",
            "    0\n",
            "}\n",
        ),
    )
    .expect("write source");
    let source_path_str = source_path.to_string_lossy().to_string();

    let suggest = run_aic(&["suggest-effects", &source_path_str]);
    assert_eq!(
        suggest.status.code(),
        Some(1),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&suggest.stdout),
        String::from_utf8_lossy(&suggest.stderr)
    );
    let suggest_json: Value = serde_json::from_slice(&suggest.stdout).expect("suggest json");
    let suggestions = suggest_json["suggestions"]
        .as_array()
        .expect("suggestions array");

    let middle = suggestions
        .iter()
        .find(|entry| entry["function"] == "middle")
        .expect("middle suggestion");
    assert_eq!(middle["current_effects"], json!([]));
    assert_eq!(middle["required_effects"], json!(["io"]));
    assert_eq!(middle["missing_effects"], json!(["io"]));
    assert_eq!(middle["current_capabilities"], json!([]));
    assert_eq!(middle["required_capabilities"], json!(["io"]));
    assert_eq!(middle["missing_capabilities"], json!(["io"]));
    assert_eq!(middle["reason"]["io"], "middle -> leaf");
    assert_eq!(middle["capability_reason"]["io"], "middle -> leaf");

    let top = suggestions
        .iter()
        .find(|entry| entry["function"] == "top")
        .expect("top suggestion");
    assert_eq!(top["current_effects"], json!([]));
    assert_eq!(top["required_effects"], json!(["io"]));
    assert_eq!(top["missing_effects"], json!(["io"]));
    assert_eq!(top["current_capabilities"], json!([]));
    assert_eq!(top["required_capabilities"], json!(["io"]));
    assert_eq!(top["missing_capabilities"], json!(["io"]));
    assert_eq!(top["reason"]["io"], "top -> middle -> leaf");
    assert_eq!(top["capability_reason"]["io"], "top -> middle -> leaf");

    let apply = run_aic(&["diag", "apply-fixes", &source_path_str, "--json"]);
    assert_eq!(
        apply.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&apply.stdout),
        String::from_utf8_lossy(&apply.stderr)
    );
    let apply_json: Value = serde_json::from_slice(&apply.stdout).expect("apply-fixes json");
    assert!(apply_json["conflicts"]
        .as_array()
        .expect("conflicts array")
        .is_empty());
    let applied_edits = apply_json["applied_edits"]
        .as_array()
        .expect("applied edits array");
    assert!(applied_edits.iter().any(|edit| {
        edit["message"]
            .as_str()
            .unwrap_or_default()
            .contains("function 'middle'")
    }));
    assert!(applied_edits.iter().any(|edit| {
        edit["message"]
            .as_str()
            .unwrap_or_default()
            .contains("function 'top'")
    }));

    let rewritten = fs::read_to_string(&source_path).expect("read rewritten");
    assert!(
        rewritten.contains("effects { io }"),
        "expected rewritten source to include io effect declarations: {rewritten}"
    );
    assert!(
        rewritten.contains("capabilities { io }"),
        "expected rewritten source to include io capability declarations: {rewritten}"
    );

    let suggest_after = run_aic(&["suggest-effects", &source_path_str]);
    assert_eq!(
        suggest_after.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&suggest_after.stdout),
        String::from_utf8_lossy(&suggest_after.stderr)
    );
    let suggest_after_json: Value =
        serde_json::from_slice(&suggest_after.stdout).expect("suggest-after json");
    let suggestions_after = suggest_after_json["suggestions"]
        .as_array()
        .expect("suggestions array");
    assert!(
        !suggestions_after
            .iter()
            .any(|entry| matches!(entry["function"].as_str(), Some("middle") | Some("top"))),
        "expected middle/top suggestions to be resolved, got: {suggest_after_json:#}"
    );
}

#[test]
fn check_show_holes_outputs_structured_hole_report() {
    let out = run_aic(&["check", "examples/e7/typed_holes.aic", "--show-holes"]);
    assert_eq!(out.status.code(), Some(0));
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("holes json");
    let holes = payload["holes"].as_array().expect("holes array");
    assert!(!holes.is_empty(), "payload={payload:#}");
    assert!(holes.iter().all(|hole| hole["line"].is_number()));
    assert!(holes.iter().all(|hole| hole["inferred"].is_string()));
    assert!(holes.iter().all(|hole| hole["context"].is_string()));
    assert!(holes.iter().any(|hole| {
        hole["context"]
            .as_str()
            .unwrap_or_default()
            .contains("parameter")
    }));
    assert!(holes.iter().any(|hole| {
        hole["context"]
            .as_str()
            .unwrap_or_default()
            .contains("return type")
    }));
    assert!(holes.iter().any(|hole| {
        hole["context"]
            .as_str()
            .unwrap_or_default()
            .contains("let binding")
    }));
}

#[test]
fn check_warn_unused_emits_deterministic_agent_readable_warnings() {
    let first = run_aic(&[
        "check",
        "examples/e7/unused_warnings.aic",
        "--warn-unused",
        "--json",
    ]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&first.stdout).expect("diagnostics json");
    let items = diagnostics.as_array().expect("diagnostics array");
    assert!(
        items.iter().any(|diag| diag["code"] == "E6004"),
        "missing E6004: {diagnostics:#}"
    );
    assert!(
        items.iter().any(|diag| diag["code"] == "E6005"),
        "missing E6005: {diagnostics:#}"
    );
    assert!(
        items.iter().any(|diag| diag["code"] == "E6006"),
        "missing E6006: {diagnostics:#}"
    );
    assert!(
        items
            .iter()
            .all(|diag| diag["severity"].as_str() == Some("warning")),
        "expected only warning severities: {diagnostics:#}"
    );

    let import_diag = items
        .iter()
        .find(|diag| diag["code"] == "E6004")
        .expect("missing E6004 diagnostic");
    assert!(
        import_diag["suggested_fixes"]
            .as_array()
            .expect("E6004 fixes array")
            .iter()
            .any(|fix| fix["replacement"].as_str() == Some("")),
        "expected import removal fix: {import_diag:#}"
    );

    let variable_diag = items
        .iter()
        .find(|diag| diag["code"] == "E6006")
        .expect("missing E6006 diagnostic");
    assert!(
        variable_diag["suggested_fixes"]
            .as_array()
            .expect("E6006 fixes array")
            .iter()
            .any(|fix| fix["replacement"].as_str() == Some("_scratch")),
        "expected variable prefix fix: {variable_diag:#}"
    );

    let second = run_aic(&[
        "check",
        "examples/e7/unused_warnings.aic",
        "--warn-unused",
        "--json",
    ]);
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(
        first.stdout, second.stdout,
        "expected deterministic warning json output"
    );
}

#[test]
fn check_without_warn_unused_preserves_existing_behavior() {
    let out = run_aic(&["check", "examples/e7/unused_warnings.aic", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("diagnostics json");
    let items = diagnostics.as_array().expect("diagnostics array");
    assert!(
        !items.iter().any(|diag| {
            matches!(
                diag["code"].as_str(),
                Some("E6004") | Some("E6005") | Some("E6006")
            )
        }),
        "unused warnings should be opt-in only; diagnostics={diagnostics:#}"
    );
}

#[test]
fn check_defaults_to_max_errors_20() {
    let (_project, source_path) = write_many_check_diagnostics_fixture();
    let out = run_aic(&["check", &source_path, "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("diagnostics json");
    let items = diagnostics.as_array().expect("diagnostics array");
    assert_eq!(
        items.len(),
        20,
        "expected diagnostics to be capped to 20 by default; diagnostics={diagnostics:#}"
    );
}

#[test]
fn check_honors_custom_max_errors_and_keeps_order() {
    let (_project, source_path) = write_many_check_diagnostics_fixture();
    let capped_out = run_aic(&["check", &source_path, "--json", "--max-errors", "7"]);
    assert_eq!(capped_out.status.code(), Some(1));
    let capped: serde_json::Value =
        serde_json::from_slice(&capped_out.stdout).expect("capped diagnostics json");
    let capped_items = capped.as_array().expect("capped diagnostics array");
    assert_eq!(capped_items.len(), 7);

    let full_out = run_aic(&["check", &source_path, "--json", "--max-errors", "200"]);
    assert_eq!(full_out.status.code(), Some(1));
    let full: serde_json::Value =
        serde_json::from_slice(&full_out.stdout).expect("full diagnostics json");
    let full_items = full.as_array().expect("full diagnostics array");
    assert!(
        full_items.len() > capped_items.len(),
        "expected uncapped result to contain more diagnostics; full={full:#}"
    );
    assert_eq!(
        capped_items.as_slice(),
        &full_items[..capped_items.len()],
        "expected capped diagnostics to preserve sorted prefix"
    );
}

#[test]
fn coverage_command_emits_deterministic_json_and_writes_report() {
    let project = tempdir().expect("project");
    let source = project.path().join("coverage_ok.aic");
    fs::write(
        &source,
        "module coverage.ok;\nfn main() -> Int {\n    0\n}\n",
    )
    .expect("write coverage source");
    let report_path = project.path().join("target/coverage/report.json");

    let source_str = source.to_string_lossy().to_string();
    let report_str = report_path.to_string_lossy().to_string();
    let first = run_aic(&["coverage", &source_str, "--report", &report_str]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "first stdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let second = run_aic(&["coverage", &source_str, "--report", &report_str]);
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(
        first.stdout, second.stdout,
        "coverage output should be deterministic"
    );

    let stdout_json: serde_json::Value =
        serde_json::from_slice(&first.stdout).expect("coverage stdout json");
    let report_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read coverage report"))
            .expect("coverage report json");

    assert_eq!(stdout_json, report_json);
    assert_eq!(stdout_json["phase"], "coverage");
    assert_eq!(stdout_json["schema_version"], "1.0");
    assert!(stdout_json["summary"]["coverage_pct"].is_number());
    assert!(stdout_json["files"].is_array());
}

#[test]
fn coverage_check_enforces_minimum_percentage() {
    let project = tempdir().expect("project");
    let source = project.path().join("coverage_bad.aic");
    fs::write(
        &source,
        "module coverage.bad;\nfn main() -> Int {\n    missing_fn()\n}\n",
    )
    .expect("write bad coverage source");
    let source_str = source.to_string_lossy().to_string();

    let fail = run_aic(&["coverage", &source_str, "--check", "--min", "75"]);
    assert_eq!(fail.status.code(), Some(1));
    let fail_json: serde_json::Value = serde_json::from_slice(&fail.stdout).expect("fail json");
    assert_eq!(fail_json["phase"], "coverage");
    assert_eq!(fail_json["check"]["min_pct"], 75.0);
    assert_eq!(fail_json["check"]["passed"], false);

    let pass = run_aic(&["coverage", &source_str, "--check", "--min", "0"]);
    assert_eq!(pass.status.code(), Some(0));
    let pass_json: serde_json::Value = serde_json::from_slice(&pass.stdout).expect("pass json");
    assert_eq!(pass_json["check"]["passed"], true);
}

#[test]
fn metrics_command_emits_deterministic_json_shape() {
    let project = tempdir().expect("project");
    let source = project.path().join("metrics_demo.aic");
    fs::write(
        &source,
        concat!(
            "module metrics.demo;\n",
            "fn beta(a: Int, b: Int) -> Int effects { io } capabilities { io } {\n",
            "    if a > 0 && b > 0 {\n",
            "        if a > b { a } else { b }\n",
            "    } else {\n",
            "        0\n",
            "    }\n",
            "}\n",
            "fn alpha(v: Int) -> Int {\n",
            "    if v > 0 { v } else { 0 }\n",
            "}\n",
        ),
    )
    .expect("write metrics source");
    let source_str = source.to_string_lossy().to_string();

    let first = run_aic(&["metrics", &source_str]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "metrics stdout={}\nmetrics stderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let second = run_aic(&["metrics", &source_str]);
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(
        first.stdout, second.stdout,
        "metrics output should be deterministic"
    );

    let payload: serde_json::Value = serde_json::from_slice(&first.stdout).expect("metrics json");
    assert_eq!(payload["phase"], "metrics");
    assert_eq!(payload["schema_version"], "1.0");
    let functions = payload["functions"].as_array().expect("functions array");
    assert_eq!(functions.len(), 2);
    assert_eq!(functions[0]["name"], "alpha");
    assert_eq!(functions[1]["name"], "beta");
    for function in functions {
        assert!(function["cyclomatic_complexity"].is_u64());
        assert!(function["cognitive_complexity"].is_u64());
        assert!(function["lines"].is_u64());
        assert!(function["params"].is_u64());
        assert!(function["effects"].is_array());
        assert!(function["max_nesting_depth"].is_u64());
        assert!(function["rating"].is_string());
    }
}

#[test]
fn metrics_check_fails_when_cyclomatic_exceeds_cli_threshold() {
    let project = tempdir().expect("project");
    let source = project.path().join("metrics_gate.aic");
    fs::write(
        &source,
        concat!(
            "module metrics.gate;\n",
            "fn complex(v: Int) -> Int {\n",
            "    if v > 0 && v < 100 {\n",
            "        if v > 10 { v } else { 10 }\n",
            "    } else {\n",
            "        0\n",
            "    }\n",
            "}\n",
        ),
    )
    .expect("write metrics gate source");
    let source_str = source.to_string_lossy().to_string();

    let fail = run_aic(&["metrics", &source_str, "--check", "--max-cyclomatic", "2"]);
    assert_eq!(fail.status.code(), Some(1));
    let fail_json: serde_json::Value = serde_json::from_slice(&fail.stdout).expect("fail json");
    assert_eq!(fail_json["phase"], "metrics");
    assert_eq!(fail_json["check"]["passed"], false);
    assert!(fail_json["check"]["violations"]
        .as_array()
        .expect("violations")
        .iter()
        .any(|entry| {
            entry["function"] == "complex" && entry["metric"] == "cyclomatic_complexity"
        }));

    let pass = run_aic(&["metrics", &source_str, "--check", "--max-cyclomatic", "50"]);
    assert_eq!(pass.status.code(), Some(0));
    let pass_json: serde_json::Value = serde_json::from_slice(&pass.stdout).expect("pass json");
    assert_eq!(pass_json["check"]["passed"], true);
}

#[test]
fn metrics_check_uses_aic_toml_thresholds_and_cli_override() {
    let project = tempdir().expect("project");
    fs::create_dir_all(project.path().join("src")).expect("mkdir src");
    fs::write(
        project.path().join("aic.toml"),
        concat!(
            "[package]\n",
            "name = \"metrics_demo\"\n",
            "main = \"src/main.aic\"\n\n",
            "[metrics]\n",
            "max_cyclomatic = 2\n",
        ),
    )
    .expect("write manifest");
    fs::write(
        project.path().join("src/main.aic"),
        concat!(
            "module metrics.cfg;\n",
            "fn main(v: Int) -> Int {\n",
            "    if v > 0 && v < 100 {\n",
            "        if v > 10 { v } else { 10 }\n",
            "    } else {\n",
            "        0\n",
            "    }\n",
            "}\n",
        ),
    )
    .expect("write source");

    let fail = run_aic_in_dir(project.path(), &["metrics", "src/main.aic", "--check"]);
    assert_eq!(fail.status.code(), Some(1));
    let fail_json: serde_json::Value = serde_json::from_slice(&fail.stdout).expect("fail json");
    assert_eq!(fail_json["check"]["thresholds"]["max_cyclomatic"], 2);
    assert_eq!(fail_json["check"]["passed"], false);

    let pass = run_aic_in_dir(
        project.path(),
        &[
            "metrics",
            "src/main.aic",
            "--check",
            "--max-cyclomatic",
            "20",
        ],
    );
    assert_eq!(pass.status.code(), Some(0));
    let pass_json: serde_json::Value = serde_json::from_slice(&pass.stdout).expect("pass json");
    assert_eq!(pass_json["check"]["thresholds"]["max_cyclomatic"], 20);
    assert_eq!(pass_json["check"]["passed"], true);
}

#[test]
fn bench_command_emits_json_and_writes_report_file() {
    let project = tempdir().expect("project");
    let (budget_path, dataset_rel) = write_bench_fixture(project.path());
    let output_path = project.path().join("target/bench/bench.json");

    let budget = budget_path.to_string_lossy().to_string();
    let output = output_path.to_string_lossy().to_string();
    let result = run_aic_in_dir(
        project.path(),
        &["bench", "--budget", &budget, "--output", &output],
    );
    assert_eq!(
        result.status.code(),
        Some(0),
        "bench stdout={}\nstderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let stdout_json: serde_json::Value =
        serde_json::from_slice(&result.stdout).expect("bench stdout json");
    let file_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output_path).expect("read bench output file"))
            .expect("bench output file json");
    assert_eq!(stdout_json, file_json);
    assert_eq!(stdout_json["phase"], "bench");
    assert_eq!(stdout_json["schema_version"], "1.0");
    assert_eq!(stdout_json["ok"], true);
    assert_eq!(stdout_json["report"]["dataset"], dataset_rel);
    assert_eq!(stdout_json["output_path"], output);
    assert!(stdout_json["compare_path"].is_null());
    assert!(stdout_json["trend"].is_null());
    assert!(stdout_json["report"]["metrics"]["dataset_fingerprint"].is_string());
    assert!(stdout_json["report"]["violations"]
        .as_array()
        .expect("violations array")
        .is_empty());
}

#[test]
fn bench_compare_reports_regressions_and_fails() {
    let project = tempdir().expect("project");
    let (budget_path, dataset_rel) = write_bench_fixture(project.path());
    let baseline_path = project.path().join("compare-baseline.json");
    fs::write(
        &baseline_path,
        format!(
            concat!(
                "{{\n",
                "  \"dataset\": \"{}\",\n",
                "  \"parser_ms\": -1.0,\n",
                "  \"typecheck_ms\": -1.0,\n",
                "  \"codegen_ms\": -1.0\n",
                "}}\n"
            ),
            dataset_rel
        ),
    )
    .expect("write compare baseline");
    let output_path = project.path().join("target/bench/bench-compare.json");

    let budget = budget_path.to_string_lossy().to_string();
    let baseline = baseline_path.to_string_lossy().to_string();
    let output = output_path.to_string_lossy().to_string();
    let result = run_aic_in_dir(
        project.path(),
        &[
            "bench",
            "--budget",
            &budget,
            "--output",
            &output,
            "--compare",
            &baseline,
        ],
    );
    assert_eq!(
        result.status.code(),
        Some(1),
        "bench compare stdout={}\nstderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let output_json: serde_json::Value =
        serde_json::from_slice(&result.stdout).expect("bench compare json");
    assert_eq!(output_json["phase"], "bench");
    assert_eq!(output_json["ok"], false);
    assert_eq!(output_json["compare_path"], baseline);
    assert_eq!(output_json["report"]["baseline"]["dataset"], dataset_rel);
    assert_eq!(
        output_json["trend"]["parser"]["within_regression_limit"],
        false
    );
    assert!(output_json["report"]["violations"]
        .as_array()
        .expect("violations array")
        .iter()
        .any(|entry| {
            entry
                .as_str()
                .unwrap_or_default()
                .contains("regression exceeded")
        }));
}

#[test]
fn static_contract_verifier_emits_discharge_and_residual_notes() {
    let out = run_aic(&["check", "examples/verify/range_proofs.aic", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("diagnostics json");
    let items = diagnostics.as_array().expect("diagnostics array");
    assert!(
        items.iter().any(|diag| diag["code"] == "E4005"),
        "expected E4005 discharge note; diagnostics={diagnostics:#}"
    );
    assert!(
        items.iter().any(|diag| diag["code"] == "E4003"),
        "expected E4003 residual note; diagnostics={diagnostics:#}"
    );
}

#[test]
fn static_contract_verifier_output_is_deterministic() {
    let first = run_aic(&["check", "examples/verify/range_proofs.aic", "--json"]);
    let second = run_aic(&["check", "examples/verify/range_proofs.aic", "--json"]);
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(
        first.stdout, second.stdout,
        "expected deterministic diagnostics output"
    );
}

#[test]
fn resource_protocol_violation_reports_e2006() {
    let out = run_aic(&[
        "check",
        "examples/verify/file_protocol_invalid.aic",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("diagnostics json");
    let items = diagnostics.as_array().expect("diagnostics array");
    assert!(
        items.iter().any(|diag| diag["code"] == "E2006"),
        "expected E2006 protocol diagnostic; diagnostics={diagnostics:#}"
    );
}

#[test]
fn resource_protocol_violation_reports_e2006_for_fs_and_net_proc_examples() {
    for input in [
        "examples/verify/fs_protocol_invalid.aic",
        "examples/verify/net_proc_protocol_invalid.aic",
    ] {
        let out = run_aic(&["check", input, "--json"]);
        assert_eq!(out.status.code(), Some(1), "input={input}");
        let diagnostics: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("diagnostics json");
        let items = diagnostics.as_array().expect("diagnostics array");
        assert!(
            items.iter().any(|diag| diag["code"] == "E2006"),
            "expected E2006 for {input}; diagnostics={diagnostics:#}"
        );
    }
}

#[test]
fn missing_capability_reports_e2009() {
    let out = run_aic(&[
        "check",
        "examples/verify/capability_missing_invalid.aic",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("diagnostics json");
    let items = diagnostics.as_array().expect("diagnostics array");
    assert!(
        items.iter().any(|diag| diag["code"] == "E2009"),
        "expected E2009 capability diagnostic; diagnostics={diagnostics:#}"
    );
}

#[test]
fn explain_and_contract_commands_work() {
    let explain_known = run_aic(&["explain", "E2001", "--json"]);
    assert_eq!(explain_known.status.code(), Some(0));
    let known: serde_json::Value =
        serde_json::from_slice(&explain_known.stdout).expect("explain json");
    assert_eq!(known["known"], true);
    assert_eq!(known["code"], "E2001");

    let unknown = format!("E{}{}{}{}", 9, 9, 9, 9);
    let explain_unknown = run_aic(&["explain", &unknown]);
    assert_eq!(explain_unknown.status.code(), Some(1));
    let text = String::from_utf8_lossy(&explain_unknown.stdout);
    assert!(text.contains("unknown diagnostic code"));

    let contract = run_aic(&["contract", "--json"]);
    assert_eq!(contract.status.code(), Some(0));
    let contract_json: serde_json::Value =
        serde_json::from_slice(&contract.stdout).expect("contract json");
    assert_eq!(contract_json["version"], "1.0");
    assert_eq!(contract_json["protocol"]["name"], "aic-compiler-json");
    assert_eq!(contract_json["protocol"]["selected_version"], "1.0");
    assert!(contract_json["commands"].is_array());
    assert!(contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .any(|c| c["name"] == "ast"));
    assert!(contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .any(|c| c["name"] == "lsp"));
    assert!(contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .any(|c| c["name"] == "coverage"));
    assert!(contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .any(|c| c["name"] == "metrics"));
    assert!(contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .any(|c| c["name"] == "bench"));
    let bench_contract = contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .find(|c| c["name"] == "bench")
        .expect("bench command contract");
    assert!(bench_contract["stable_flags"]
        .as_array()
        .expect("bench stable flags")
        .iter()
        .any(|flag| flag == "--compare"));
    assert!(bench_contract["stable_flags"]
        .as_array()
        .expect("bench stable flags")
        .iter()
        .any(|flag| flag == "--output"));
    let run_contract = contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .find(|c| c["name"] == "run")
        .expect("run command contract");
    assert!(run_contract["stable_flags"]
        .as_array()
        .expect("run stable flags")
        .iter()
        .any(|flag| flag == "--profile"));
    assert!(run_contract["stable_flags"]
        .as_array()
        .expect("run stable flags")
        .iter()
        .any(|flag| flag == "--profile-output"));
    let debug_contract = contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .find(|c| c["name"] == "debug")
        .expect("debug command contract");
    assert!(debug_contract["stable_flags"]
        .as_array()
        .expect("debug stable flags")
        .iter()
        .any(|flag| flag == "subcommands:dap"));
    assert!(debug_contract["stable_flags"]
        .as_array()
        .expect("debug stable flags")
        .iter()
        .any(|flag| flag == "dap --adapter"));
    for phase in ["parse", "ast", "check", "build", "fix"] {
        assert!(contract_json["schemas"][phase]["path"].is_string());
        assert!(contract_json["examples"][phase].is_string());
    }
}

#[test]
fn debug_dap_reports_missing_backend_when_path_is_empty() {
    let output = Command::new(env!("CARGO_BIN_EXE_aic"))
        .args(["debug", "dap"])
        .current_dir(repo_root())
        .env("PATH", "")
        .env_remove("AIC_DEBUG_ADAPTER")
        .output()
        .expect("run aic debug dap with empty PATH");
    assert_eq!(
        output.status.code(),
        Some(3),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unable to locate a debug adapter backend"),
        "expected missing-backend guidance in stderr, got:\n{stderr}"
    );
}

#[test]
fn debug_dap_accepts_explicit_adapter_path() {
    let adapter = if PathBuf::from("/usr/bin/true").is_file() {
        PathBuf::from("/usr/bin/true")
    } else {
        PathBuf::from("/bin/true")
    };
    if !adapter.is_file() {
        return;
    }

    let adapter_arg = adapter.to_string_lossy().to_string();
    let output = run_aic(&["debug", "dap", "--adapter", &adapter_arg]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ast_command_emits_deterministic_typed_json_shape() {
    let project = tempdir().expect("project");
    fs::create_dir_all(project.path().join("src")).expect("mkdir src");
    let source_path = project.path().join("src/main.aic");
    fs::write(
        &source_path,
        concat!(
            "module ast.demo;\n",
            "fn main() -> Int requires true ensures true {\n",
            "    0\n",
            "}\n",
        ),
    )
    .expect("write source");

    let source = source_path.to_string_lossy().to_string();
    let first = run_aic(&["ast", "--json", &source]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "ast stdout={}\nast stderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let second = run_aic(&["ast", "--json", &source]);
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(
        first.stdout, second.stdout,
        "aic ast --json output must be deterministic"
    );

    let payload: Value = serde_json::from_slice(&first.stdout).expect("ast json");
    assert_eq!(payload["version"], "1.0");
    assert_eq!(payload["module"], "ast.demo");
    assert!(payload["ast"].is_object());
    assert!(payload["ir"].is_object());
    assert!(payload["resolved_types"].is_array());
    assert!(payload["generic_instantiations"].is_array());
    assert!(payload["function_effects"].is_object());
    assert!(payload["contracts"].is_object());
    assert!(payload["import_graph"].is_object());
    let diagnostics = payload["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag["severity"].as_str() != Some("error")),
        "expected no error diagnostics, got: {diagnostics:#?}"
    );

    let resolved_types = payload["resolved_types"]
        .as_array()
        .expect("resolved_types array");
    assert!(
        resolved_types
            .iter()
            .all(|entry| entry["id"].is_u64() && entry["repr"].is_string()),
        "resolved_types entries missing id/repr: {resolved_types:#?}"
    );

    let contract_functions = payload["contracts"]["functions"]
        .as_array()
        .expect("contracts.functions");
    assert_eq!(contract_functions.len(), 1);
    assert_eq!(contract_functions[0]["function"], "main");
    assert!(contract_functions[0]["requires"]["span"]["start"].is_u64());
    assert!(contract_functions[0]["ensures"]["span"]["end"].is_u64());

    let import_graph = &payload["import_graph"];
    assert_eq!(import_graph["entry_module"], "ast.demo");
    assert!(import_graph["imports"]
        .as_array()
        .expect("import list")
        .is_empty());
    assert!(import_graph["edges"]
        .as_array()
        .expect("import edges")
        .is_empty());
}

#[test]
fn ast_command_emits_bitwise_operator_nodes() {
    let project = tempdir().expect("project");
    fs::create_dir_all(project.path().join("src")).expect("mkdir src");
    let source_path = project.path().join("src/main.aic");
    fs::write(
        &source_path,
        concat!(
            "module ast.bitwise;\n",
            "fn main() -> Int {\n",
            "    let mixed = ((0xF0 & 0x0F) | (0xAA ^ 0x0F)) << 1;\n",
            "    let shifted = mixed >> 2;\n",
            "    let logical = shifted >>> 1;\n",
            "    let inverted = ~logical;\n",
            "    inverted\n",
            "}\n",
        ),
    )
    .expect("write source");

    let source = source_path.to_string_lossy().to_string();
    let ast_out = run_aic(&["ast", "--json", &source]);
    assert_eq!(
        ast_out.status.code(),
        Some(0),
        "ast stdout={}\nast stderr={}",
        String::from_utf8_lossy(&ast_out.stdout),
        String::from_utf8_lossy(&ast_out.stderr)
    );
    let payload: Value = serde_json::from_slice(&ast_out.stdout).expect("ast json");
    let ast_json = serde_json::to_string(&payload["ast"]).expect("ast payload string");
    for op in ["BitAnd", "BitOr", "BitXor", "Shl", "Shr", "Ushr", "BitNot"] {
        assert!(
            ast_json.contains(op),
            "expected operator {op} to appear in AST payload: {ast_json}"
        );
    }
}

#[test]
fn ast_schema_references_are_consistent_in_contract_and_docs() {
    let contract = run_aic(&["contract", "--json"]);
    assert_eq!(contract.status.code(), Some(0));
    let contract_json: Value = serde_json::from_slice(&contract.stdout).expect("contract json");

    let ast_schema_path = contract_json["schemas"]["ast"]["path"]
        .as_str()
        .expect("ast schema path");
    let ast_example_path = contract_json["examples"]["ast"]
        .as_str()
        .expect("ast example path");

    assert_eq!(
        ast_schema_path,
        "docs/agent-tooling/schemas/ast-response.schema.json"
    );
    assert_eq!(ast_example_path, "examples/agent/protocol_ast.md");
    assert!(
        repo_root().join(ast_schema_path).exists(),
        "missing schema file at {ast_schema_path}"
    );
    assert!(
        repo_root().join(ast_example_path).exists(),
        "missing example artifact at {ast_example_path}"
    );

    let ast_command = contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .find(|command| command["name"] == "ast")
        .expect("ast command contract");
    assert!(ast_command["stable_flags"]
        .as_array()
        .expect("ast stable flags")
        .iter()
        .any(|flag| flag == "--json"));

    let tooling_readme = fs::read_to_string(repo_root().join("docs/agent-tooling/README.md"))
        .expect("read agent tooling README");
    assert!(
        tooling_readme.contains(ast_schema_path),
        "agent tooling README missing schema reference"
    );

    let protocol_doc = fs::read_to_string(repo_root().join("docs/agent-tooling/protocol-v1.md"))
        .expect("read protocol doc");
    assert!(
        protocol_doc.contains(ast_schema_path),
        "protocol doc missing schema reference"
    );
    assert!(
        protocol_doc.contains(ast_example_path),
        "protocol doc missing example reference"
    );
}

#[test]
fn grammar_command_help_and_outputs_are_contract_stable() {
    let grammar_help = run_aic(&["grammar", "--help"]);
    assert_eq!(grammar_help.status.code(), Some(0));
    let grammar_help_text = String::from_utf8_lossy(&grammar_help.stdout);
    for flag in ["--ebnf", "--json"] {
        assert!(
            grammar_help_text.contains(flag),
            "missing `{flag}` in grammar help:\n{grammar_help_text}"
        );
    }

    let no_format = run_aic(&["grammar"]);
    assert_eq!(no_format.status.code(), Some(2));

    let expected_grammar = include_str!("../docs/grammar.ebnf");
    let ebnf = run_aic(&["grammar", "--ebnf"]);
    assert_eq!(ebnf.status.code(), Some(0));
    let ebnf_text = String::from_utf8_lossy(&ebnf.stdout);
    assert_eq!(ebnf_text, expected_grammar);

    let grammar_json = run_aic(&["grammar", "--json"]);
    assert_eq!(grammar_json.status.code(), Some(0));
    let grammar_value: Value = serde_json::from_slice(&grammar_json.stdout).expect("grammar json");
    assert_eq!(grammar_value["version"], "mvp-grammar-v6");
    assert_eq!(grammar_value["format"], "ebnf");
    assert_eq!(grammar_value["source_path"], "docs/grammar.ebnf");
    assert_eq!(grammar_value["source_contract_path"], "docs/syntax.md");
    assert_eq!(grammar_value["grammar"], expected_grammar);

    let contract = run_aic(&["contract", "--json"]);
    assert_eq!(contract.status.code(), Some(0));
    let contract_json: Value = serde_json::from_slice(&contract.stdout).expect("contract json");
    let grammar_command = contract_json["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .find(|command| command["name"] == "grammar")
        .expect("grammar command contract");
    assert!(grammar_command["stable_flags"]
        .as_array()
        .expect("stable flags")
        .iter()
        .any(|flag| flag == "--ebnf"));
    assert!(grammar_command["stable_flags"]
        .as_array()
        .expect("stable flags")
        .iter()
        .any(|flag| flag == "--json"));
}

#[test]
fn diag_apply_fixes_dry_run_and_apply_are_deterministic() {
    let project = tempdir().expect("project");
    fs::create_dir_all(project.path().join("src")).expect("mkdir src");
    let source_path = project.path().join("src/main.aic");
    fs::write(
        &source_path,
        "module fixdemo.main;\nfn main() -> Int {\n    let x = 1\n    x\n}\n",
    )
    .expect("write source");

    let source_path_str = source_path.to_string_lossy().to_string();
    let original = fs::read_to_string(&source_path).expect("read original");

    let dry_run_1 = run_aic(&[
        "diag",
        "apply-fixes",
        &source_path_str,
        "--dry-run",
        "--json",
    ]);
    assert_eq!(
        dry_run_1.status.code(),
        Some(0),
        "dry-run-1 stdout={}\nstderr={}",
        String::from_utf8_lossy(&dry_run_1.stdout),
        String::from_utf8_lossy(&dry_run_1.stderr)
    );
    let dry_run_1_json: serde_json::Value =
        serde_json::from_slice(&dry_run_1.stdout).expect("dry run json");
    assert_eq!(dry_run_1_json["mode"], "dry-run");
    assert!(dry_run_1_json["conflicts"]
        .as_array()
        .expect("conflicts")
        .is_empty());
    assert!(!dry_run_1_json["applied_edits"]
        .as_array()
        .expect("applied edits")
        .is_empty());

    let dry_run_2 = run_aic(&[
        "diag",
        "apply-fixes",
        &source_path_str,
        "--dry-run",
        "--json",
    ]);
    assert_eq!(dry_run_2.status.code(), Some(0));
    let dry_run_2_json: serde_json::Value =
        serde_json::from_slice(&dry_run_2.stdout).expect("dry run json 2");
    assert_eq!(
        dry_run_1_json["applied_edits"],
        dry_run_2_json["applied_edits"]
    );

    let after_dry_run = fs::read_to_string(&source_path).expect("read after dry-run");
    assert_eq!(original, after_dry_run);

    let apply = run_aic(&["diag", "apply-fixes", &source_path_str, "--json"]);
    assert_eq!(
        apply.status.code(),
        Some(0),
        "apply stdout={}\nstderr={}",
        String::from_utf8_lossy(&apply.stdout),
        String::from_utf8_lossy(&apply.stderr)
    );
    let apply_json: serde_json::Value = serde_json::from_slice(&apply.stdout).expect("apply json");
    assert_eq!(apply_json["mode"], "apply");
    assert!(apply_json["conflicts"]
        .as_array()
        .expect("conflicts")
        .is_empty());

    let rewritten = fs::read_to_string(&source_path).expect("read rewritten");
    assert!(rewritten.contains("let x = 1;"));

    let check = run_aic(&["check", &source_path_str]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "check stdout={}\nstderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn diag_apply_fixes_supports_warn_unused_edits() {
    let project = tempdir().expect("project");
    fs::create_dir_all(project.path().join("src")).expect("mkdir src");
    let source_path = project.path().join("src/main.aic");
    fs::write(
        &source_path,
        concat!(
            "module warnunused.demo;\n",
            "import std.io;\n",
            "fn helper() -> Int {\n",
            "    1\n",
            "}\n",
            "fn main() -> Int {\n",
            "    let scratch = helper();\n",
            "    0\n",
            "}\n",
        ),
    )
    .expect("write source");

    let source_path_str = source_path.to_string_lossy().to_string();
    let dry_run = run_aic(&[
        "diag",
        "apply-fixes",
        &source_path_str,
        "--warn-unused",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(
        dry_run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&dry_run.stdout),
        String::from_utf8_lossy(&dry_run.stderr)
    );
    let dry_run_json: serde_json::Value =
        serde_json::from_slice(&dry_run.stdout).expect("dry-run json");
    let edits = dry_run_json["applied_edits"]
        .as_array()
        .expect("applied edits array");
    assert!(
        edits.len() >= 2,
        "expected at least import+variable edits, got: {dry_run_json:#}"
    );

    let apply = run_aic(&[
        "diag",
        "apply-fixes",
        &source_path_str,
        "--warn-unused",
        "--json",
    ]);
    assert_eq!(
        apply.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&apply.stdout),
        String::from_utf8_lossy(&apply.stderr)
    );

    let rewritten = fs::read_to_string(&source_path).expect("read rewritten");
    assert!(
        !rewritten.contains("import std.io;"),
        "expected unused import to be removed: {rewritten}"
    );
    assert!(
        rewritten.contains("let _scratch = helper();"),
        "expected unused variable to be prefixed: {rewritten}"
    );
}

#[test]
fn test_harness_runs_categories_and_reports_json() {
    let all = run_aic(&["test", "examples/e7/harness", "--json"]);
    assert_eq!(all.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&all.stdout).expect("harness report");
    assert_eq!(report["total"], 3);
    assert_eq!(report["failed"], 0);

    let compile_fail_mode = run_aic(&[
        "test",
        "examples/e7/harness",
        "--mode",
        "compile-fail",
        "--json",
    ]);
    assert_eq!(compile_fail_mode.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_slice(&compile_fail_mode.stdout).expect("compile-fail report");
    assert_eq!(report["total"], 1);
    assert_eq!(report["failed"], 0);
}

#[test]
fn test_command_discovers_attribute_tests_and_supports_filter() {
    let dir = tempdir().expect("tempdir");
    let test_file = dir.path().join("tests.aic");
    fs::write(
        &test_file,
        r#"
#[test]
fn test_addition() -> () {
    assert_eq(1 + 1, 2);
    assert(true);
    assert_ne(1, 2);
}

#[test]
#[should_panic]
fn test_division_by_zero() -> () {
    assert_eq(1, 2);
}
"#,
    )
    .expect("write attribute tests");

    let root = dir.path().to_string_lossy().to_string();

    let all = run_aic(&["test", &root, "--json"]);
    assert_eq!(
        all.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&all.stdout),
        String::from_utf8_lossy(&all.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&all.stdout).expect("json report");
    assert_eq!(report["total"], 2, "report={report:#}");
    assert_eq!(report["failed"], 0, "report={report:#}");

    let report_file = dir.path().join("test_results.json");
    assert!(
        report_file.exists(),
        "missing report file: {}",
        report_file.display()
    );

    let filtered = run_aic(&["test", &root, "--filter", "addition", "--json"]);
    assert_eq!(
        filtered.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&filtered.stdout),
        String::from_utf8_lossy(&filtered.stderr)
    );
    let filtered_report: serde_json::Value =
        serde_json::from_slice(&filtered.stdout).expect("filtered json report");
    assert_eq!(filtered_report["total"], 1, "report={filtered_report:#}");
    assert_eq!(filtered_report["failed"], 0, "report={filtered_report:#}");
}

fn write_pkg_project(root: &std::path::Path, name: &str, version: &str, module: &str, body: &str) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("aic.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nmain = \"src/main.aic\"\n"),
    )
    .expect("write manifest");
    fs::write(
        root.join("src/main.aic"),
        format!("module {module}.main;\n{body}\n"),
    )
    .expect("write source");
}

fn write_consumer_project(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("aic.toml"),
        "[package]\nname = \"consumer_app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write consumer manifest");
    fs::write(
        root.join("src/main.aic"),
        "module consumer_app.main;\nfn main() -> Int { 0 }\n",
    )
    .expect("write consumer source");
}

fn write_workspace_demo(root: &std::path::Path) {
    fs::create_dir_all(root.join("packages/util/src")).expect("mkdir util");
    fs::create_dir_all(root.join("packages/app/src")).expect("mkdir app");
    fs::write(
        root.join("aic.workspace.toml"),
        "[workspace]\nmembers = [\"packages/app\", \"packages/util\"]\n",
    )
    .expect("write workspace manifest");

    fs::write(
        root.join("packages/util/aic.toml"),
        "[package]\nname = \"util_pkg\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write util manifest");
    fs::write(
        root.join("packages/util/src/main.aic"),
        "module util_pkg.main;\npub fn value() -> Int { 42 }\n",
    )
    .expect("write util source");

    fs::write(
        root.join("packages/app/aic.toml"),
        concat!(
            "[package]\n",
            "name = \"app_pkg\"\n",
            "version = \"0.1.0\"\n",
            "main = \"src/main.aic\"\n\n",
            "[dependencies]\n",
            "util_pkg = { path = \"../util\" }\n",
        ),
    )
    .expect("write app manifest");
    fs::write(
        root.join("packages/app/src/main.aic"),
        "module app_pkg.main;\nimport util_pkg.main;\nfn main() -> Int { util_pkg.main.value() }\n",
    )
    .expect("write app source");
}

fn write_incremental_daemon_demo(root: &std::path::Path) {
    fs::create_dir_all(root.join("dep/src")).expect("mkdir dep");
    fs::create_dir_all(root.join("app/src")).expect("mkdir app");

    fs::write(
        root.join("dep/aic.toml"),
        "[package]\nname = \"inc_dep\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write dep manifest");
    fs::write(
        root.join("dep/src/main.aic"),
        "module inc_dep.main;\npub fn base() -> Int { 40 }\n",
    )
    .expect("write dep source");

    fs::write(
        root.join("app/aic.toml"),
        concat!(
            "[package]\n",
            "name = \"inc_app\"\n",
            "version = \"0.1.0\"\n",
            "main = \"src/main.aic\"\n\n",
            "[dependencies]\n",
            "inc_dep = { path = \"../dep\" }\n",
        ),
    )
    .expect("write app manifest");
    fs::write(
        root.join("app/src/main.aic"),
        concat!(
            "module inc_app.main;\n",
            "import std.io;\n",
            "import inc_dep.main;\n",
            "fn main() -> Int effects { io } capabilities { io } {\n",
            "  print_int(inc_dep.main.base() + 2);\n",
            "  0\n",
            "}\n",
        ),
    )
    .expect("write app source");
}

#[test]
fn pkg_publish_search_install_roundtrip() {
    let registry = tempdir().expect("registry");
    let package = tempdir().expect("package");
    let consumer = tempdir().expect("consumer");

    write_pkg_project(
        package.path(),
        "http_client",
        "1.2.0",
        "http_client",
        "pub fn get() -> Int { 42 }",
    );

    let publish = run_aic(&[
        "pkg",
        "publish",
        package.path().to_str().expect("pkg path"),
        "--registry",
        registry.path().to_str().expect("registry path"),
    ]);
    assert_eq!(publish.status.code(), Some(0));

    let search = run_aic(&[
        "pkg",
        "search",
        "http",
        "--registry",
        registry.path().to_str().expect("registry path"),
        "--json",
    ]);
    assert_eq!(search.status.code(), Some(0));
    let search_json: serde_json::Value =
        serde_json::from_slice(&search.stdout).expect("search json");
    assert!(search_json.is_array());
    assert_eq!(search_json[0]["package"], "http_client");
    assert_eq!(search_json[0]["latest"], "1.2.0");

    fs::create_dir_all(consumer.path().join("src")).expect("mkdir consumer src");
    fs::write(
        consumer.path().join("aic.toml"),
        "[package]\nname = \"consumer_app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write consumer manifest");
    fs::write(
        consumer.path().join("src/main.aic"),
        "module consumer_app.main;\nimport http_client.main;\nfn main() -> Int { http_client.main.get() }\n",
    )
    .expect("write consumer source");

    let install = run_aic(&[
        "pkg",
        "install",
        "http_client@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer path"),
        "--registry",
        registry.path().to_str().expect("registry path"),
    ]);
    assert_eq!(install.status.code(), Some(0));

    assert!(consumer.path().join("deps/http_client/aic.toml").exists());
    assert!(consumer.path().join("aic.lock").exists());

    let check = run_aic(&["check", consumer.path().to_str().expect("consumer path")]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "check stdout={}\nstderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn build_links_native_c_library_from_manifest_native_section() {
    let project = tempdir().expect("project");
    fs::create_dir_all(project.path().join("src")).expect("mkdir src");
    fs::create_dir_all(project.path().join("native")).expect("mkdir native");

    fs::write(
        project.path().join("aic.toml"),
        r#"[package]
name = "ffi_demo"
version = "0.1.0"
main = "src/main.aic"

[native]
libs = ["ffiadd"]
search_paths = ["native"]
"#,
    )
    .expect("write manifest");
    fs::write(
        project.path().join("src/main.aic"),
        r#"module ffi_demo.main;
import std.io;

extern "C" fn ffi_add42(x: Int) -> Int;

fn add42(x: Int) -> Int {
    unsafe { ffi_add42(x) }
}

fn main() -> Int effects { io } capabilities { io } {
    print_int(add42(0));
    0
}
"#,
    )
    .expect("write source");
    fs::write(
        project.path().join("native/add.c"),
        r#"long ffi_add42(long x) { return x + 42; }"#,
    )
    .expect("write c source");

    let compile_obj = Command::new("clang")
        .args(["-O0", "-c", "native/add.c", "-o", "native/add.o"])
        .current_dir(project.path())
        .output()
        .expect("compile c object");
    assert!(
        compile_obj.status.success(),
        "clang stderr={}",
        String::from_utf8_lossy(&compile_obj.stderr)
    );
    let archive = Command::new("ar")
        .args(["rcs", "native/libffiadd.a", "native/add.o"])
        .current_dir(project.path())
        .output()
        .expect("archive static lib");
    assert!(
        archive.status.success(),
        "ar stderr={}",
        String::from_utf8_lossy(&archive.stderr)
    );

    let run = run_aic_in_dir(project.path(), &["run", "src/main.aic"]);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "42\n");
}

#[test]
fn build_release_defaults_to_o2_and_allows_opt_level_override() {
    let project = tempdir().expect("project");
    write_build_opt_project(project.path());

    let release_output = project.path().join("release-default-bin");
    let release_output_str = release_output.to_string_lossy().to_string();
    let release_telemetry = project.path().join("telemetry-release.jsonl");
    let release_telemetry_str = release_telemetry.to_string_lossy().to_string();

    let release = run_aic_in_dir_with_env(
        project.path(),
        &[
            "build",
            "src/main.aic",
            "--release",
            "--output",
            &release_output_str,
        ],
        &[("AIC_TELEMETRY_PATH", &release_telemetry_str)],
    );
    assert_eq!(
        release.status.code(),
        Some(0),
        "release build stdout={}\nstderr={}",
        String::from_utf8_lossy(&release.stdout),
        String::from_utf8_lossy(&release.stderr)
    );
    assert_eq!(read_clang_opt_level(&release_telemetry), "O2");

    let override_output = project.path().join("release-override-bin");
    let override_output_str = override_output.to_string_lossy().to_string();
    let override_telemetry = project.path().join("telemetry-override.jsonl");
    let override_telemetry_str = override_telemetry.to_string_lossy().to_string();

    let overridden = run_aic_in_dir_with_env(
        project.path(),
        &[
            "build",
            "src/main.aic",
            "--release",
            "--opt-level",
            "O3",
            "--output",
            &override_output_str,
        ],
        &[("AIC_TELEMETRY_PATH", &override_telemetry_str)],
    );
    assert_eq!(
        overridden.status.code(),
        Some(0),
        "override build stdout={}\nstderr={}",
        String::from_utf8_lossy(&overridden.stdout),
        String::from_utf8_lossy(&overridden.stderr)
    );
    assert_eq!(read_clang_opt_level(&override_telemetry), "O3");
}

#[test]
fn build_short_o_flag_propagates_and_invalid_opt_level_is_rejected() {
    let project = tempdir().expect("project");
    write_build_opt_project(project.path());

    let output_path = project.path().join("short-o-bin");
    let output_path_str = output_path.to_string_lossy().to_string();
    let telemetry_path = project.path().join("telemetry-short-o.jsonl");
    let telemetry_path_str = telemetry_path.to_string_lossy().to_string();

    let short_o = run_aic_in_dir_with_env(
        project.path(),
        &["build", "src/main.aic", "-O1", "--output", &output_path_str],
        &[("AIC_TELEMETRY_PATH", &telemetry_path_str)],
    );
    assert_eq!(
        short_o.status.code(),
        Some(0),
        "short -O build stdout={}\nstderr={}",
        String::from_utf8_lossy(&short_o.stdout),
        String::from_utf8_lossy(&short_o.stderr)
    );
    assert_eq!(read_clang_opt_level(&telemetry_path), "O1");

    let invalid = run_aic_in_dir(
        project.path(),
        &["build", "src/main.aic", "--opt-level", "O9"],
    );
    assert_eq!(invalid.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&invalid.stderr);
    assert!(
        stderr.contains("invalid optimization level"),
        "stderr={stderr}"
    );
}

#[test]
fn run_profile_writes_profile_json_with_top_functions() {
    let project = tempdir().expect("project");
    write_profile_demo_project(project.path());

    let run = run_aic_in_dir(project.path(), &["run", "src/main.aic", "--profile"]);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "7\n");

    let profile_path = project.path().join("profile.json");
    assert!(profile_path.exists(), "expected profile.json to be written");
    let profile: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(profile_path).expect("read profile"))
            .expect("profile json");

    assert_eq!(profile["phase"], "profile");
    assert_eq!(profile["schema_version"], "1.0");
    let top = profile["top_functions"].as_array().expect("top functions");
    assert!(!top.is_empty(), "expected non-empty top_functions");
    assert!(
        top.iter().any(|entry| entry["function"] == "run.execute"),
        "top_functions={profile:#}"
    );
    for entry in top {
        assert!(entry["function"].is_string());
        assert!(entry["self_time_ms"].is_number());
        assert!(entry["total_time_ms"].is_number());
    }

    let mut previous = f64::INFINITY;
    for entry in top {
        let total = entry["total_time_ms"].as_f64().expect("total_time_ms");
        assert!(
            total <= previous + f64::EPSILON,
            "top_functions should be sorted by total_time_ms desc: {profile:#}"
        );
        previous = total;
    }
}

#[test]
fn run_profile_output_flag_writes_custom_profile_path() {
    let project = tempdir().expect("project");
    write_profile_demo_project(project.path());
    let custom = project.path().join("reports/custom-profile.json");
    let custom_str = custom.to_string_lossy().to_string();

    let run = run_aic_in_dir(
        project.path(),
        &[
            "run",
            "src/main.aic",
            "--profile",
            "--profile-output",
            &custom_str,
        ],
    );
    assert_eq!(run.status.code(), Some(0));
    assert!(custom.exists(), "expected custom profile output path");

    let profile: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(custom).expect("read profile"))
            .expect("profile json");
    assert_eq!(profile["phase"], "profile");
    assert!(profile["top_functions"].is_array());
}

#[test]
fn run_check_leaks_reports_clean_exit_without_leak_diagnostic() {
    let project = tempdir().expect("project");
    write_leak_clean_project(project.path());

    let run = run_aic_in_dir(project.path(), &["run", "src/main.aic", "--check-leaks"]);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        !stderr.contains("memory_leak_detected"),
        "unexpected leak report for clean program: {stderr}"
    );
}

#[test]
fn run_check_leaks_reports_detected_leaks_and_fails() {
    let project = tempdir().expect("project");
    write_leak_positive_project(project.path());

    let run = run_aic_in_dir(project.path(), &["run", "src/main.aic", "--check-leaks"]);
    assert_eq!(
        run.status.code(),
        Some(1),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    let leak_line = stderr
        .lines()
        .find(|line| line.contains("\"memory_leak_detected\""))
        .unwrap_or_else(|| panic!("missing leak report in stderr: {stderr}"));
    let payload: serde_json::Value = serde_json::from_str(leak_line).expect("leak json");
    assert_eq!(payload["code"], "memory_leak_detected");
    assert!(
        payload["count"].as_u64().unwrap_or(0) > 0,
        "expected positive leaked allocation count: {payload:#}"
    );
    assert!(
        payload["bytes"].as_u64().unwrap_or(0) > 0,
        "expected positive leaked bytes: {payload:#}"
    );
    assert!(payload["first_allocation"]["site"].is_string());
    assert!(payload["first_allocation"]["line"].is_number());
}

#[test]
fn workspace_check_and_build_execute_in_deterministic_order() {
    let workspace = tempdir().expect("workspace");
    write_workspace_demo(workspace.path());

    let check = run_aic(&["check", workspace.path().to_str().expect("workspace path")]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "check stdout={}\nstderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let build = run_aic(&["build", workspace.path().to_str().expect("workspace path")]);
    assert_eq!(
        build.status.code(),
        Some(0),
        "build stdout={}\nstderr={}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let stdout = String::from_utf8_lossy(&build.stdout);
    let util_pos = stdout
        .find("target/workspace/util_pkg/libmain.a")
        .expect("util build line");
    let app_pos = stdout
        .find("target/workspace/app_pkg/libmain.a")
        .expect("app build line");
    assert!(
        util_pos < app_pos,
        "workspace build order must be deterministic"
    );

    let root = fs::canonicalize(workspace.path()).expect("canonical workspace root");
    assert!(root.join("target/workspace/util_pkg/libmain.a").exists());
    assert!(root.join("target/workspace/app_pkg/libmain.a").exists());
}

#[test]
fn workspace_lockfile_is_shared_and_offline_check_works() {
    let workspace = tempdir().expect("workspace");
    write_workspace_demo(workspace.path());

    let lock = run_aic(&["lock", workspace.path().to_str().expect("workspace path")]);
    assert_eq!(
        lock.status.code(),
        Some(0),
        "lock stdout={}\nstderr={}",
        String::from_utf8_lossy(&lock.stdout),
        String::from_utf8_lossy(&lock.stderr)
    );

    assert!(workspace.path().join("aic.lock").exists());
    assert!(!workspace.path().join("packages/app/aic.lock").exists());

    let offline = run_aic(&[
        "check",
        workspace.path().to_str().expect("workspace path"),
        "--offline",
    ]);
    assert_eq!(
        offline.status.code(),
        Some(0),
        "offline stdout={}\nstderr={}",
        String::from_utf8_lossy(&offline.stdout),
        String::from_utf8_lossy(&offline.stderr)
    );
}

#[test]
fn workspace_cycle_is_reported_as_diagnostic() {
    let workspace = tempdir().expect("workspace");
    fs::create_dir_all(workspace.path().join("packages/a/src")).expect("mkdir a");
    fs::create_dir_all(workspace.path().join("packages/b/src")).expect("mkdir b");
    fs::write(
        workspace.path().join("aic.workspace.toml"),
        "[workspace]\nmembers = [\"packages/a\", \"packages/b\"]\n",
    )
    .expect("write workspace manifest");
    fs::write(
        workspace.path().join("packages/a/aic.toml"),
        concat!(
            "[package]\n",
            "name = \"a_pkg\"\n",
            "version = \"0.1.0\"\n",
            "main = \"src/main.aic\"\n\n",
            "[dependencies]\n",
            "b_pkg = { path = \"../b\" }\n",
        ),
    )
    .expect("write a manifest");
    fs::write(
        workspace.path().join("packages/a/src/main.aic"),
        "module a_pkg.main;\nfn main() -> Int { 0 }\n",
    )
    .expect("write a source");
    fs::write(
        workspace.path().join("packages/b/aic.toml"),
        concat!(
            "[package]\n",
            "name = \"b_pkg\"\n",
            "version = \"0.1.0\"\n",
            "main = \"src/main.aic\"\n\n",
            "[dependencies]\n",
            "a_pkg = { path = \"../a\" }\n",
        ),
    )
    .expect("write b manifest");
    fs::write(
        workspace.path().join("packages/b/src/main.aic"),
        "module b_pkg.main;\nfn main() -> Int { 0 }\n",
    )
    .expect("write b source");

    let check = run_aic(&[
        "check",
        workspace.path().to_str().expect("workspace path"),
        "--json",
    ]);
    assert_eq!(check.status.code(), Some(1));
    let diagnostics: serde_json::Value = serde_json::from_slice(&check.stdout).expect("json diags");
    assert!(diagnostics.is_array());
    assert_eq!(diagnostics[0]["code"], "E2126");
}

#[test]
fn workspace_build_is_incremental_for_unchanged_members() {
    let workspace = tempdir().expect("workspace");
    write_workspace_demo(workspace.path());

    let first = run_aic(&["build", workspace.path().to_str().expect("workspace path")]);
    assert_eq!(first.status.code(), Some(0));

    let second = run_aic(&["build", workspace.path().to_str().expect("workspace path")]);
    assert_eq!(second.status.code(), Some(0));
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(second_stdout.contains("up-to-date"));

    fs::write(
        workspace.path().join("packages/util/src/main.aic"),
        "module util_pkg.main;\npub fn value() -> Int { 7 }\n",
    )
    .expect("rewrite util source");

    let third = run_aic(&["build", workspace.path().to_str().expect("workspace path")]);
    assert_eq!(third.status.code(), Some(0));
    let third_stdout = String::from_utf8_lossy(&third.stdout);
    assert!(third_stdout.contains("target/workspace/util_pkg/libmain.a"));
    assert!(third_stdout.contains("target/workspace/app_pkg/libmain.a"));
    assert!(
        third_stdout.matches("built ").count() >= 2,
        "expected rebuild of changed package and dependents; stdout={third_stdout}"
    );
}

#[test]
fn daemon_cache_invalidation_tracks_dependency_content_hashes() {
    let demo = tempdir().expect("demo");
    write_incremental_daemon_demo(demo.path());
    let app_entry = demo.path().join("app/src/main.aic");
    let dep_source = demo.path().join("dep/src/main.aic");
    let app_entry_str = app_entry.to_string_lossy().to_string();

    let mut daemon = DaemonHarness::spawn(&repo_root());
    let first = daemon.request(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "check",
        "params": { "input": app_entry_str }
    }));
    assert!(first.get("error").is_none(), "first={first:#}");
    assert_eq!(first["result"]["cache_hit"], false);
    assert_eq!(first["result"]["has_errors"], false);
    assert_eq!(first["result"]["diagnostics"], json!([]));
    let fingerprint_a = first["result"]["fingerprint"]
        .as_str()
        .expect("fingerprint a")
        .to_string();

    let second = daemon.request(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "check",
        "params": { "input": app_entry.to_string_lossy() }
    }));
    assert!(second.get("error").is_none(), "second={second:#}");
    assert_eq!(second["result"]["cache_hit"], true);
    assert_eq!(second["result"]["fingerprint"], fingerprint_a);

    fs::write(
        dep_source,
        "module inc_dep.main;\npub fn base() -> Int { 41 }\n",
    )
    .expect("rewrite dep source");

    let third = daemon.request(&json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "check",
        "params": { "input": app_entry.to_string_lossy() }
    }));
    assert!(third.get("error").is_none(), "third={third:#}");
    assert_eq!(third["result"]["cache_hit"], false);
    assert_eq!(third["result"]["has_errors"], false);
    assert_ne!(
        third["result"]["fingerprint"]
            .as_str()
            .expect("fingerprint b"),
        fingerprint_a
    );

    let stats = daemon.request(&json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "stats",
        "params": {}
    }));
    assert!(stats.get("error").is_none(), "stats={stats:#}");
    assert!(stats["result"]["frontend_cache_hits"].as_u64().unwrap_or(0) >= 1);
    assert!(
        stats["result"]["frontend_cache_misses"]
            .as_u64()
            .unwrap_or(0)
            >= 2
    );
    daemon.shutdown();
}

#[test]
fn daemon_warm_and_cold_builds_are_deterministic() {
    let demo = tempdir().expect("demo");
    write_incremental_daemon_demo(demo.path());
    let app_entry = demo.path().join("app/src/main.aic");
    let out_a = demo.path().join("app_bin_a");

    let mut daemon = DaemonHarness::spawn(&repo_root());
    let check = daemon.request(&json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "check",
        "params": { "input": app_entry.to_string_lossy() }
    }));
    assert!(check.get("error").is_none(), "check={check:#}");
    assert_eq!(check["result"]["cache_hit"], false);
    assert_eq!(check["result"]["has_errors"], false);

    let cold_build = daemon.request(&json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "build",
        "params": {
            "input": app_entry.to_string_lossy(),
            "output": out_a.to_string_lossy(),
            "artifact": "exe"
        }
    }));
    assert!(cold_build.get("error").is_none(), "cold={cold_build:#}");
    assert_eq!(cold_build["result"]["cache_hit"], false);
    assert_eq!(cold_build["result"]["frontend_cache_hit"], true);
    assert_eq!(cold_build["result"]["has_errors"], false);
    let cold_hash = cold_build["result"]["output_sha256"]
        .as_str()
        .expect("cold hash")
        .to_string();
    let cold_ms = cold_build["result"]["duration_ms"].as_u64().unwrap_or(0);

    let warm_build = daemon.request(&json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "build",
        "params": {
            "input": app_entry.to_string_lossy(),
            "output": out_a.to_string_lossy(),
            "artifact": "exe"
        }
    }));
    assert!(warm_build.get("error").is_none(), "warm={warm_build:#}");
    assert_eq!(warm_build["result"]["cache_hit"], true);
    assert_eq!(warm_build["result"]["output_sha256"], cold_hash);
    let warm_ms = warm_build["result"]["duration_ms"].as_u64().unwrap_or(0);
    assert!(
        warm_ms <= cold_ms.saturating_add(5),
        "expected warm build to be no slower than cold: warm={warm_ms} cold={cold_ms}"
    );
    daemon.shutdown();

    let mut second_daemon = DaemonHarness::spawn(&repo_root());
    let cold_again = second_daemon.request(&json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "build",
        "params": {
            "input": app_entry.to_string_lossy(),
            "output": out_a.to_string_lossy(),
            "artifact": "exe"
        }
    }));
    assert!(
        cold_again.get("error").is_none(),
        "cold_again={cold_again:#}"
    );
    assert_eq!(cold_again["result"]["cache_hit"], false);
    assert_eq!(cold_again["result"]["output_sha256"], cold_hash);
    second_daemon.shutdown();
}

#[test]
fn pkg_trust_policy_enforces_signatures_and_emits_audit_records() {
    let registry = tempdir().expect("registry");
    let package = tempdir().expect("package");
    let consumer_ok = tempdir().expect("consumer ok");
    let consumer_bad = tempdir().expect("consumer bad");

    write_pkg_project(
        package.path(),
        "signed_pkg",
        "1.0.0",
        "signed_pkg",
        "fn value() -> Int { 7 }",
    );

    let publish = run_aic_with_env(
        &[
            "pkg",
            "publish",
            package.path().to_str().expect("package"),
            "--registry",
            registry.path().to_str().expect("registry"),
        ],
        &[
            ("AIC_PKG_SIGNING_KEY", "pkg-secret"),
            ("AIC_PKG_SIGNING_KEY_ID", "corp"),
        ],
    );
    assert_eq!(
        publish.status.code(),
        Some(0),
        "publish stdout={}\nstderr={}",
        String::from_utf8_lossy(&publish.stdout),
        String::from_utf8_lossy(&publish.stderr)
    );

    for consumer in [consumer_ok.path(), consumer_bad.path()] {
        write_consumer_project(consumer);
        fs::write(
            consumer.join("aic.registry.json"),
            format!(
                concat!(
                    "{{\n",
                    "  \"default\": \"local\",\n",
                    "  \"registries\": {{\n",
                    "    \"local\": {{\n",
                    "      \"path\": \"{}\",\n",
                    "      \"trust\": {{\n",
                    "        \"default\": \"allow\",\n",
                    "        \"require_signed\": true,\n",
                    "        \"trusted_keys\": {{ \"corp\": \"AIC_TRUSTED_CORP_KEY\" }}\n",
                    "      }}\n",
                    "    }}\n",
                    "  }}\n",
                    "}}\n"
                ),
                registry.path().display()
            ),
        )
        .expect("registry config");
    }

    let install_ok = run_aic_with_env(
        &[
            "pkg",
            "install",
            "signed_pkg@^1.0.0",
            "--path",
            consumer_ok.path().to_str().expect("consumer ok"),
            "--json",
        ],
        &[("AIC_TRUSTED_CORP_KEY", "pkg-secret")],
    );
    assert_eq!(
        install_ok.status.code(),
        Some(0),
        "install ok stdout={}\nstderr={}",
        String::from_utf8_lossy(&install_ok.stdout),
        String::from_utf8_lossy(&install_ok.stderr)
    );
    let ok_json: serde_json::Value = serde_json::from_slice(&install_ok.stdout).expect("ok json");
    assert_eq!(ok_json["installed"][0]["package"], "signed_pkg");
    assert_eq!(ok_json["audit"][0]["decision"], "allow");
    assert_eq!(ok_json["audit"][0]["signature_verified"], true);

    let install_bad = run_aic_with_env(
        &[
            "pkg",
            "install",
            "signed_pkg@^1.0.0",
            "--path",
            consumer_bad.path().to_str().expect("consumer bad"),
            "--json",
        ],
        &[("AIC_TRUSTED_CORP_KEY", "wrong-secret")],
    );
    assert_eq!(install_bad.status.code(), Some(1));
    let diags: serde_json::Value = serde_json::from_slice(&install_bad.stdout).expect("diag json");
    assert!(diags.is_array());
    assert_eq!(diags[0]["code"], "E2124");
}

#[test]
fn pkg_install_conflict_is_diagnostic_and_json_structured() {
    let registry = tempdir().expect("registry");
    let package_v1 = tempdir().expect("package v1");
    let package_v2 = tempdir().expect("package v2");
    let consumer = tempdir().expect("consumer");

    write_pkg_project(
        package_v1.path(),
        "netlib",
        "1.0.0",
        "netlib",
        "fn v() -> Int { 1 }",
    );
    write_pkg_project(
        package_v2.path(),
        "netlib",
        "2.0.0",
        "netlib",
        "fn v() -> Int { 2 }",
    );

    let publish_v1 = run_aic(&[
        "pkg",
        "publish",
        package_v1.path().to_str().expect("pkg v1"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(publish_v1.status.code(), Some(0));

    let publish_v2 = run_aic(&[
        "pkg",
        "publish",
        package_v2.path().to_str().expect("pkg v2"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(publish_v2.status.code(), Some(0));

    fs::create_dir_all(consumer.path().join("src")).expect("mkdir consumer src");
    fs::write(
        consumer.path().join("aic.toml"),
        "[package]\nname = \"consumer_app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write consumer manifest");
    fs::write(
        consumer.path().join("src/main.aic"),
        "module consumer_app.main;\nfn main() -> Int { 0 }\n",
    )
    .expect("write consumer source");

    let install = run_aic(&[
        "pkg",
        "install",
        "netlib@^1.0.0",
        "netlib@^2.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
        "--registry",
        registry.path().to_str().expect("registry"),
        "--json",
    ]);
    assert_eq!(install.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&install.stdout).expect("diagnostics");
    assert!(diagnostics.is_array());
    assert_eq!(diagnostics[0]["code"], "E2114");
}

#[test]
fn pkg_install_lockfile_is_deterministic() {
    let registry = tempdir().expect("registry");
    let package_v1 = tempdir().expect("package v1");
    let package_v2 = tempdir().expect("package v2");
    let consumer = tempdir().expect("consumer");

    write_pkg_project(
        package_v1.path(),
        "utilpkg",
        "1.0.0",
        "utilpkg",
        "fn v() -> Int { 1 }",
    );
    write_pkg_project(
        package_v2.path(),
        "utilpkg",
        "1.1.0",
        "utilpkg",
        "fn v() -> Int { 2 }",
    );

    let publish_v1 = run_aic(&[
        "pkg",
        "publish",
        package_v1.path().to_str().expect("pkg v1"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(publish_v1.status.code(), Some(0));

    let publish_v2 = run_aic(&[
        "pkg",
        "publish",
        package_v2.path().to_str().expect("pkg v2"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(publish_v2.status.code(), Some(0));

    fs::create_dir_all(consumer.path().join("src")).expect("mkdir consumer src");
    fs::write(
        consumer.path().join("aic.toml"),
        "[package]\nname = \"consumer_app\"\nversion = \"0.1.0\"\nmain = \"src/main.aic\"\n",
    )
    .expect("write consumer manifest");
    fs::write(
        consumer.path().join("src/main.aic"),
        "module consumer_app.main;\nfn main() -> Int { 0 }\n",
    )
    .expect("write consumer source");

    let install_1 = run_aic(&[
        "pkg",
        "install",
        "utilpkg@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(install_1.status.code(), Some(0));
    let first_lock = fs::read_to_string(consumer.path().join("aic.lock")).expect("lock 1");
    let first_manifest = fs::read_to_string(consumer.path().join("aic.toml")).expect("manifest 1");
    assert!(
        first_manifest.contains("resolved_version = \"1.1.0\""),
        "manifest missing resolved_version metadata: {first_manifest}"
    );
    assert!(
        first_manifest.contains("source_provenance = \"registry_root="),
        "manifest missing source_provenance metadata: {first_manifest}"
    );
    let first_lock_json: serde_json::Value =
        serde_json::from_str(&first_lock).expect("parse lock 1 json");
    assert_eq!(first_lock_json["schema_version"], 2);
    let first_dep = first_lock_json["dependencies"]
        .as_array()
        .expect("dependencies array")
        .iter()
        .find(|dep| dep["name"] == "utilpkg")
        .expect("utilpkg lock dependency");
    assert_eq!(first_dep["resolved_version"], "1.1.0");
    assert!(
        first_dep["source_provenance"]
            .as_str()
            .unwrap_or_default()
            .contains("registry_root="),
        "lock missing source_provenance metadata: {first_dep:#?}"
    );

    let install_2 = run_aic(&[
        "pkg",
        "install",
        "utilpkg@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
        "--registry",
        registry.path().to_str().expect("registry"),
    ]);
    assert_eq!(install_2.status.code(), Some(0));
    let second_lock = fs::read_to_string(consumer.path().join("aic.lock")).expect("lock 2");

    assert_eq!(first_lock, second_lock);
}

#[test]
fn pkg_example_consumes_local_http_client_module() {
    let check = run_aic(&["check", "examples/pkg/consume_http_client.aic"]);
    assert_eq!(check.status.code(), Some(0));
}

#[test]
fn pkg_workspace_demo_example_checks_and_builds() {
    let check = run_aic(&["check", "examples/pkg/workspace_demo"]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "check stdout={}\nstderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let build = run_aic(&["build", "examples/pkg/workspace_demo"]);
    assert_eq!(
        build.status.code(),
        Some(0),
        "build stdout={}\nstderr={}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
}

#[test]
fn pkg_private_registry_auth_and_scope_workflow() {
    let public_registry = tempdir().expect("public");
    let private_registry = tempdir().expect("private");
    let public_pkg = tempdir().expect("public package");
    let private_pkg = tempdir().expect("private package");
    let consumer = tempdir().expect("consumer");

    write_pkg_project(
        public_pkg.path(),
        "utilpkg",
        "1.0.0",
        "utilpkg",
        "fn v() -> Int { 1 }",
    );
    write_pkg_project(
        private_pkg.path(),
        "corp/http_client",
        "1.2.0",
        "corp_http_client",
        "fn v() -> Int { 2 }",
    );

    assert_eq!(
        run_aic(&[
            "pkg",
            "publish",
            public_pkg.path().to_str().expect("public pkg"),
            "--registry",
            public_registry.path().to_str().expect("public registry"),
        ])
        .status
        .code(),
        Some(0)
    );
    assert_eq!(
        run_aic(&[
            "pkg",
            "publish",
            private_pkg.path().to_str().expect("private pkg"),
            "--registry",
            private_registry.path().to_str().expect("private registry"),
        ])
        .status
        .code(),
        Some(0)
    );

    write_consumer_project(consumer.path());
    let token_file = consumer.path().join("private.token");
    fs::write(&token_file, "super-secret\n").expect("write token file");
    fs::write(
        consumer.path().join("aic.registry.json"),
        format!(
            concat!(
                "{{\n",
                "  \"default\": \"public\",\n",
                "  \"registries\": {{\n",
                "    \"public\": {{ \"path\": \"{}\" }},\n",
                "    \"private\": {{\n",
                "      \"path\": \"{}\",\n",
                "      \"private\": true,\n",
                "      \"token_env\": \"AIC_PRIVATE_TOKEN\",\n",
                "      \"token_file\": \"{}\"\n",
                "    }}\n",
                "  }},\n",
                "  \"scopes\": {{ \"corp/\": \"private\" }}\n",
                "}}\n"
            ),
            public_registry.path().display(),
            private_registry.path().display(),
            token_file.display()
        ),
    )
    .expect("write registry config");

    let unauthorized = run_aic(&[
        "pkg",
        "install",
        "corp/http_client@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
        "--json",
    ]);
    assert_eq!(unauthorized.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&unauthorized.stdout).expect("unauthorized diagnostics");
    assert_eq!(diagnostics[0]["code"], "E2117");

    let install = run_aic_with_env(
        &[
            "pkg",
            "install",
            "utilpkg@^1.0.0",
            "corp/http_client@^1.0.0",
            "--path",
            consumer.path().to_str().expect("consumer"),
            "--json",
        ],
        &[("AIC_PRIVATE_TOKEN", "super-secret")],
    );
    assert_eq!(install.status.code(), Some(0));
    assert!(consumer.path().join("deps/utilpkg/aic.toml").exists());
    assert!(consumer
        .path()
        .join("deps/corp/http_client/aic.toml")
        .exists());
}

#[test]
fn pkg_mirror_fallback_and_misconfigured_credentials_are_diagnostic() {
    let primary_registry = tempdir().expect("primary");
    let mirror_registry = tempdir().expect("mirror");
    let package = tempdir().expect("package");
    let consumer = tempdir().expect("consumer");

    write_pkg_project(
        package.path(),
        "mirror_only",
        "1.0.0",
        "mirror_only",
        "fn v() -> Int { 1 }",
    );
    assert_eq!(
        run_aic(&[
            "pkg",
            "publish",
            package.path().to_str().expect("package"),
            "--registry",
            mirror_registry.path().to_str().expect("mirror"),
        ])
        .status
        .code(),
        Some(0)
    );

    write_consumer_project(consumer.path());
    fs::write(
        consumer.path().join("aic.registry.json"),
        format!(
            concat!(
                "{{\n",
                "  \"default\": \"corp\",\n",
                "  \"registries\": {{\n",
                "    \"corp\": {{\n",
                "      \"path\": \"{}\",\n",
                "      \"mirrors\": [\"{}\"]\n",
                "    }}\n",
                "  }}\n",
                "}}\n"
            ),
            primary_registry.path().display(),
            mirror_registry.path().display()
        ),
    )
    .expect("write mirror config");

    let install = run_aic(&[
        "pkg",
        "install",
        "mirror_only@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
    ]);
    assert_eq!(install.status.code(), Some(0));
    assert!(consumer.path().join("deps/mirror_only/aic.toml").exists());

    fs::write(
        consumer.path().join("aic.registry.json"),
        format!(
            concat!(
                "{{\n",
                "  \"default\": \"private\",\n",
                "  \"registries\": {{\n",
                "    \"private\": {{\n",
                "      \"path\": \"{}\",\n",
                "      \"private\": true,\n",
                "      \"token_file\": \"missing.token\"\n",
                "    }}\n",
                "  }}\n",
                "}}\n"
            ),
            mirror_registry.path().display()
        ),
    )
    .expect("write broken private config");

    let misconfigured = run_aic(&[
        "pkg",
        "install",
        "mirror_only@^1.0.0",
        "--path",
        consumer.path().to_str().expect("consumer"),
        "--token",
        "whatever",
        "--json",
    ]);
    assert_eq!(misconfigured.status.code(), Some(1));
    let diagnostics: serde_json::Value =
        serde_json::from_slice(&misconfigured.stdout).expect("misconfigured diagnostics");
    assert_eq!(diagnostics[0]["code"], "E2118");
}

fn write_golden_harness_fixture(root: &std::path::Path, source: &str) -> (PathBuf, PathBuf) {
    let harness_root = root.join("harness");
    let golden_dir = harness_root.join("golden");
    fs::create_dir_all(&golden_dir).expect("mkdir golden dir");

    let case_path = golden_dir.join("snapshot_case.aic");
    fs::write(&case_path, source).expect("write golden case");

    let snapshot_path = golden_dir.join("snapshot_case.aic.golden");
    (harness_root, snapshot_path)
}

#[test]
fn test_harness_update_golden_writes_snapshot_file() {
    let project = tempdir().expect("project");
    let (harness_root, snapshot_path) =
        write_golden_harness_fixture(project.path(), "fn main() -> Int {\n    1\n}\n");
    assert!(
        !snapshot_path.exists(),
        "snapshot should not exist before update"
    );

    let harness_arg = harness_root.to_string_lossy().to_string();
    let result = run_aic(&["test", &harness_arg, "--mode", "golden", "--update-golden"]);

    assert_eq!(
        result.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(snapshot_path.exists(), "expected snapshot to be written");
    assert_eq!(
        fs::read_to_string(snapshot_path).expect("read snapshot"),
        "fn main() -> Int {\n    1\n}\n"
    );
}

#[test]
fn test_harness_check_golden_passes_for_matching_snapshot() {
    let project = tempdir().expect("project");
    let (harness_root, snapshot_path) =
        write_golden_harness_fixture(project.path(), "fn main() -> Int {\n    1\n}\n");
    fs::write(&snapshot_path, "fn main() -> Int {\n    1\n}\n").expect("write snapshot");

    let harness_arg = harness_root.to_string_lossy().to_string();
    let result = run_aic(&["test", &harness_arg, "--mode", "golden", "--check-golden"]);

    assert_eq!(
        result.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("failed=0"), "stdout:\n{stdout}");
}

#[test]
fn test_harness_check_golden_reports_readable_diff_on_mismatch() {
    let project = tempdir().expect("project");
    let (harness_root, snapshot_path) =
        write_golden_harness_fixture(project.path(), "fn main() -> Int {\n    1\n}\n");
    fs::write(&snapshot_path, "fn main() -> Int {\n    2\n}\n").expect("write stale snapshot");

    let harness_arg = harness_root.to_string_lossy().to_string();
    let result = run_aic(&["test", &harness_arg, "--mode", "golden", "--check-golden"]);

    assert_eq!(result.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("golden snapshot mismatch"),
        "expected mismatch label in output:\n{stdout}"
    );
    assert!(
        stdout.contains("--- expected") && stdout.contains("+++ actual"),
        "expected diff headers in output:\n{stdout}"
    );
    assert!(
        stdout.contains("@@ line"),
        "expected line-oriented diff hunk in output:\n{stdout}"
    );
}

#[test]
fn test_command_runs_property_tests_with_seed_and_reports_counterexample() {
    let dir = tempdir().expect("tempdir");
    let test_file = dir.path().join("properties.aic");
    fs::write(
        &test_file,
        r#"
#[property(iterations = 4)]
fn prop_generators_cover_all(
    i: Int,
    f: Float,
    b: Bool,
    s: String
) -> () {
    assert_eq(i, i);
    assert(b || !b);
}

#[property(iterations = 6)]
fn prop_fails(x: Int) -> () {
    assert_eq(x + 1, x);
}
"#,
    )
    .expect("write property tests");

    let root = dir.path().to_string_lossy().to_string();

    let all = run_aic(&["test", &root, "--seed", "123", "--json"]);
    assert_eq!(
        all.status.code(),
        Some(1),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&all.stdout),
        String::from_utf8_lossy(&all.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&all.stdout).expect("json report");
    assert_eq!(report["total"], 2, "report={report:#}");
    assert_eq!(report["failed"], 1, "report={report:#}");
    assert_eq!(
        report["by_category"]["property-test"], 2,
        "report={report:#}"
    );

    let cases = report["cases"].as_array().expect("cases array");
    let failed_case = cases
        .iter()
        .find(|entry| {
            entry["file"]
                .as_str()
                .map(|name| name.ends_with("::prop_fails"))
                .unwrap_or(false)
        })
        .expect("prop_fails case");
    assert_eq!(failed_case["passed"], false, "case={failed_case:#}");
    let details = failed_case["details"].as_str().expect("details string");
    assert!(details.contains("seed="), "details={details}");
    assert!(details.contains("counterexample="), "details={details}");
    assert!(details.contains("shrunk="), "details={details}");

    let report_file = dir.path().join("test_results.json");
    assert!(
        report_file.exists(),
        "missing report file: {}",
        report_file.display()
    );

    let filtered = run_aic(&[
        "test",
        &root,
        "--filter",
        "generators_cover_all",
        "--seed",
        "123",
        "--json",
    ]);
    assert_eq!(
        filtered.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&filtered.stdout),
        String::from_utf8_lossy(&filtered.stderr)
    );
    let filtered_report: serde_json::Value =
        serde_json::from_slice(&filtered.stdout).expect("filtered json report");
    assert_eq!(filtered_report["total"], 1, "report={filtered_report:#}");
    assert_eq!(filtered_report["failed"], 0, "report={filtered_report:#}");
}

#[test]
fn test_command_runs_mock_io_tests_with_deterministic_rand_and_time() {
    let dir = tempdir().expect("tempdir");
    let test_file = dir.path().join("mock_io.aic");
    fs::write(
        &test_file,
        r#"
import std.io;
import std.rand;
import std.time;
import std.vec;
import std.string;

#[test]
fn test_mock_reader_writer_and_no_real_io() -> () effects { io, rand, time } capabilities { io, rand, time } {
    let reader = mock_reader_from_lines(append(vec_of("hello"), vec_of("world")));
    let install_ok = match install_mock_reader(reader) {
        Ok(_) => true,
        Err(_) => false,
    };
    assert(install_ok);

    let first = match read_line() {
        Ok(value) => value,
        Err(_) => "",
    };
    let second = match read_line() {
        Ok(value) => value,
        Err(_) => "",
    };
    let eof_ok = match read_line() {
        Ok(_) => false,
        Err(err) => io_is_end_of_input(err),
    };

    assert(byte_length(first) == 5);
    assert(string.starts_with(first, "hello"));
    assert(byte_length(second) == 5);
    assert(string.starts_with(second, "world"));
    assert(eof_ok);

    print_str("A");
    println_int(7);

    let writer_for_write = mock_stdout_writer();
    let wrote_ok = match mock_write(writer_for_write, "B") {
        Ok(_) => true,
        Err(_) => false,
    };
    assert(wrote_ok);

    let writer_for_take = mock_stdout_writer();
    let captured = match mock_writer_take(writer_for_take) {
        Ok(value) => value,
        Err(_) => "",
    };
    let captured_len = byte_length(captured);
    assert(captured_len >= 3);
}

#[test]
fn test_rand_and_time_are_deterministic() -> () effects { io, rand, time } capabilities { io, rand, time } {
    seed(42);
    let first = random_int();
    seed(42);
    let second = random_int();

    assert_eq(first, second);
    assert_eq(now_ms(), 1767225600000);
}
"#,
    )
    .expect("write mock io tests");

    let root = dir.path().to_string_lossy().to_string();

    let first_run = run_aic(&["test", &root, "--seed", "123", "--json"]);
    assert_eq!(
        first_run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&first_run.stdout),
        String::from_utf8_lossy(&first_run.stderr)
    );
    let first_report: serde_json::Value =
        serde_json::from_slice(&first_run.stdout).expect("first json report");
    assert_eq!(first_report["total"], 2, "report={first_report:#}");
    assert_eq!(first_report["failed"], 0, "report={first_report:#}");
    assert_eq!(
        first_report["by_category"]["attribute-test"], 2,
        "report={first_report:#}"
    );

    let second_run = run_aic(&["test", &root, "--seed", "123", "--json"]);
    assert_eq!(
        second_run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&second_run.stdout),
        String::from_utf8_lossy(&second_run.stderr)
    );
    let second_report: serde_json::Value =
        serde_json::from_slice(&second_run.stdout).expect("second json report");

    assert_eq!(first_report, second_report, "reports must be deterministic");

    let report_file = dir.path().join("test_results.json");
    assert!(
        report_file.exists(),
        "missing report file: {}",
        report_file.display()
    );
}

#[test]
fn test_command_emits_replay_metadata_and_replays_failure() {
    let dir = tempdir().expect("tempdir");
    let test_file = dir.path().join("replay_failure.aic");
    fs::write(
        &test_file,
        r#"
#[property(iterations = 4)]
fn prop_replay_failure(x: Int) -> () {
    assert_eq(x + 1, x);
}
"#,
    )
    .expect("write replay fixture");

    let root = dir.path().to_string_lossy().to_string();
    let first = run_aic(&["test", &root, "--seed", "777", "--json"]);
    assert_eq!(
        first.status.code(),
        Some(1),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let first_report: serde_json::Value = serde_json::from_slice(&first.stdout).expect("json");
    let replay = first_report["replay"].as_object().expect("replay object");
    let replay_id = replay["replay_id"].as_str().expect("replay id");
    let artifact_path = replay["artifact_path"].as_str().expect("artifact path");
    assert_eq!(replay["seed"], 777, "replay={replay:#?}");
    assert!(
        std::path::Path::new(artifact_path).exists(),
        "missing replay artifact: {artifact_path}"
    );

    let artifact_text = fs::read_to_string(artifact_path).expect("read replay artifact");
    let artifact_json: serde_json::Value = serde_json::from_str(&artifact_text).expect("json");
    assert_eq!(
        artifact_json["schema"].as_str(),
        Some("aic-test-replay-v1"),
        "artifact={artifact_json:#?}"
    );
    assert_eq!(artifact_json["seed"], 777, "artifact={artifact_json:#?}");

    let replay_run = run_aic(&["test", &root, "--replay", replay_id, "--json"]);
    assert_eq!(
        replay_run.status.code(),
        Some(1),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&replay_run.stdout),
        String::from_utf8_lossy(&replay_run.stderr)
    );
    let replay_report: serde_json::Value =
        serde_json::from_slice(&replay_run.stdout).expect("replay json");
    assert_eq!(replay_report["failed"], 1, "report={replay_report:#}");
    assert_eq!(
        replay_report["cases"][0]["file"], first_report["cases"][0]["file"],
        "replay report should target same failing case"
    );
}

#[test]
fn test_command_mock_isolation_blocks_real_net_and_proc_side_effects() {
    let dir = tempdir().expect("tempdir");
    let test_file = dir.path().join("mock_isolation_violation.aic");
    fs::write(
        &test_file,
        r#"
import std.net;
import std.proc;

#[test]
fn test_real_net_side_effect_is_blocked() -> () effects { io, net } capabilities { io, net } {
    let connected = match tcp_connect("127.0.0.1:1", 5) {
        Ok(_) => true,
        Err(_) => false,
    };
    assert(connected);
}

#[test]
fn test_real_proc_side_effect_is_blocked() -> () effects { io, env, proc } capabilities { io, env, proc } {
    let launched = match run("echo hi") {
        Ok(_) => true,
        Err(_) => false,
    };
    assert(launched);
}
"#,
    )
    .expect("write mock isolation fixture");

    let root = dir.path().to_string_lossy().to_string();
    let result = run_aic(&["test", &root, "--json"]);
    assert_eq!(
        result.status.code(),
        Some(1),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).expect("json report");
    assert_eq!(report["total"], 2, "report={report:#}");
    assert_eq!(report["failed"], 2, "report={report:#}");
    let cases = report["cases"].as_array().expect("cases array");
    let details = cases
        .iter()
        .map(|entry| entry["details"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        details.contains("sandbox_policy_violation"),
        "expected structured isolation diagnostic in details:\n{details}"
    );
    assert!(
        details.contains("\"domain\":\"net\""),
        "expected net-domain isolation marker in details:\n{details}"
    );
    assert!(
        details.contains("\"domain\":\"proc\""),
        "expected proc-domain isolation marker in details:\n{details}"
    );
}

#[test]
fn intrinsic_placeholder_guard_policy_passes_and_rejects_stubs() {
    let root = repo_root();
    let script = root.join("scripts/ci/intrinsic_placeholder_guard.py");

    let ok = Command::new("python3")
        .arg(&script)
        .current_dir(&root)
        .output()
        .expect("run intrinsic placeholder guard");
    assert!(
        ok.status.success(),
        "expected guard success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&ok.stdout),
        String::from_utf8_lossy(&ok.stderr)
    );

    let fixture = tempdir().expect("fixture");
    let bad_file = fixture.path().join("stub_intrinsic.aic");
    fs::write(
        &bad_file,
        concat!(
            "module guard.bad;\n",
            "fn aic_net_tcp_connect_intrinsic(addr: String, timeout_ms: Int) -> Result[Int, NetError] effects { net } {\n",
            "    0\n",
            "}\n",
        ),
    )
    .expect("write guard fixture");

    let bad_path = bad_file.to_string_lossy().to_string();
    let fail = Command::new("python3")
        .arg(&script)
        .arg("--path")
        .arg(&bad_path)
        .current_dir(&root)
        .output()
        .expect("run intrinsic placeholder guard failure case");
    assert_eq!(fail.status.code(), Some(1));
    let fail_stderr = String::from_utf8_lossy(&fail.stderr);
    assert!(
        fail_stderr.contains("AGX1P001"),
        "expected AGX1P001 in stderr:\n{}",
        fail_stderr
    );
    assert!(
        fail_stderr.contains("declaration-only"),
        "expected declaration-only guidance in stderr:\n{}",
        fail_stderr
    );
    assert!(
        fail_stderr.contains("remediation:"),
        "expected remediation guidance in stderr:\n{}",
        fail_stderr
    );

    let exempt = Command::new("python3")
        .arg(&script)
        .arg("--path")
        .arg(&bad_path)
        .arg("--exempt")
        .arg(&bad_path)
        .current_dir(&root)
        .output()
        .expect("run intrinsic placeholder guard exemption case");
    assert!(
        exempt.status.success(),
        "expected exemption success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&exempt.stdout),
        String::from_utf8_lossy(&exempt.stderr)
    );
}

#[test]
fn intrinsic_declaration_examples_are_ci_wired_and_checkable() {
    let root = repo_root();
    let examples_ci = fs::read_to_string(root.join("scripts/ci/examples.sh"))
        .expect("read scripts/ci/examples.sh");

    for rel in [
        "examples/core/intrinsic_declaration_demo.aic",
        "examples/core/intrinsic_declaration_invalid_body.aic",
        "examples/verify/intrinsics/valid_bindings.aic",
        "examples/verify/intrinsics/invalid_bindings.aic",
    ] {
        assert!(root.join(rel).is_file(), "missing intrinsic example: {rel}");
        assert!(
            examples_ci.contains(&format!("\"{rel}\"")),
            "examples.sh missing intrinsic example wiring: {rel}"
        );
    }

    let ok = run_aic(&["check", "examples/core/intrinsic_declaration_demo.aic"]);
    assert_eq!(
        ok.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&ok.stdout),
        String::from_utf8_lossy(&ok.stderr)
    );

    let bad = run_aic(&[
        "check",
        "examples/core/intrinsic_declaration_invalid_body.aic",
        "--json",
    ]);
    assert_eq!(bad.status.code(), Some(1));
    let diagnostics: Value = serde_json::from_slice(&bad.stdout).expect("diagnostics json");
    let has_e1093 = diagnostics.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item.get("code").and_then(Value::as_str) == Some("E1093"))
    });
    assert!(
        has_e1093,
        "expected E1093 from invalid intrinsic example\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&bad.stdout),
        String::from_utf8_lossy(&bad.stderr)
    );
}

#[test]
fn template_literal_example_supports_double_braces_and_is_ci_wired() {
    let root = repo_root();
    let examples_ci = fs::read_to_string(root.join("scripts/ci/examples.sh"))
        .expect("read scripts/ci/examples.sh");
    let rel = "examples/data/template_literals.aic";
    assert!(root.join(rel).is_file(), "missing template literal example");
    assert!(
        examples_ci.contains(&format!("\"{rel}\"")),
        "examples.sh missing template literal example wiring"
    );

    let check = run_aic(&["check", rel]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = run_aic(&["run", rel]);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "42\n");
}

#[test]
fn prod_t3_t7_t9_examples_are_ci_wired_and_run_with_expected_outputs() {
    let root = repo_root();
    let examples_ci = fs::read_to_string(root.join("scripts/ci/examples.sh"))
        .expect("read scripts/ci/examples.sh");

    for (rel, expected) in [
        ("examples/io/raii_file_cleanup.aic", "42\n"),
        ("examples/io/drop_trait_cleanup.aic", "42\n"),
        ("examples/core/tuple_types.aic", "42\n"),
        ("examples/core/borrow_checker_completeness.aic", "2\n"),
        ("examples/core/dyn_trait_objects.aic", "51\n"),
    ] {
        assert!(root.join(rel).is_file(), "missing example: {rel}");
        assert!(
            examples_ci.contains(&format!("\"{rel}\"")),
            "examples.sh missing wiring for {rel}"
        );
        let run = run_aic(&["run", rel]);
        assert_eq!(
            run.status.code(),
            Some(0),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{rel}");
    }
}

#[test]
fn doc_command_supports_format_flags_and_doc_comment_metadata() {
    let project = tempdir().expect("project");
    let source = project.path().join("docs_demo.aic");
    fs::write(
        &source,
        r#"module docs.demo;

/// A typed metric value.
///
/// ## Example
/// ```aic
/// Metric { value: 1 }
/// ```
struct Metric {
    value: Int
}

/// Build a metric from an integer.
///
/// ## Example
/// ```aic
/// make_metric(1)
/// ```
fn make_metric(v: Int) -> Metric {
    Metric { value: v }
}

fn main() -> Int {
    0
}
"#,
    )
    .expect("write docs demo source");

    let source_arg = source.to_string_lossy().to_string();

    let json_out = project.path().join("target/docs-json");
    let json_out_arg = json_out.to_string_lossy().to_string();
    let json_run = run_aic_in_dir(
        project.path(),
        &[
            "doc",
            &source_arg,
            "--output",
            &json_out_arg,
            "--format",
            "json",
        ],
    );
    assert_eq!(
        json_run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&json_run.stdout),
        String::from_utf8_lossy(&json_run.stderr)
    );
    let api_path = json_out.join("api.json");
    assert!(api_path.is_file(), "missing api.json output");
    let payload: Value =
        serde_json::from_str(&fs::read_to_string(&api_path).expect("read api json")).expect("json");
    let modules = payload["modules"].as_array().expect("modules array");
    let docs_demo = modules
        .iter()
        .find(|module| module["module"].as_str() == Some("docs.demo"))
        .expect("docs.demo module");
    let items = docs_demo["items"].as_array().expect("items array");
    let make_metric = items
        .iter()
        .find(|item| item["name"].as_str() == Some("make_metric"))
        .expect("make_metric item");
    assert_eq!(
        make_metric["summary"].as_str(),
        Some("Build a metric from an integer.")
    );
    assert_eq!(make_metric["return_type"].as_str(), Some("Metric"));
    assert_eq!(
        make_metric["return_type_link"].as_str(),
        Some("#type-metric")
    );
    assert!(
        make_metric["examples"]
            .as_array()
            .is_some_and(|examples| !examples.is_empty()),
        "expected extracted doc examples"
    );
    assert!(
        make_metric["source_path"]
            .as_str()
            .is_some_and(|path| path.ends_with("docs_demo.aic")),
        "expected source_path to reference docs_demo.aic"
    );
    assert!(
        make_metric["source_line"]
            .as_u64()
            .is_some_and(|line| line > 0),
        "expected positive source line for make_metric"
    );

    let md_out = project.path().join("target/docs-md");
    let md_out_arg = md_out.to_string_lossy().to_string();
    let md_run = run_aic_in_dir(
        project.path(),
        &[
            "doc",
            &source_arg,
            "--output",
            &md_out_arg,
            "--format",
            "markdown",
        ],
    );
    assert_eq!(
        md_run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&md_run.stdout),
        String::from_utf8_lossy(&md_run.stderr)
    );
    let index_md = fs::read_to_string(md_out.join("index.md")).expect("read markdown docs");
    assert!(index_md.contains("- Returns: [Metric](#type-metric)"));

    let html_out = project.path().join("target/docs-html");
    let html_out_arg = html_out.to_string_lossy().to_string();
    let html_run = run_aic_in_dir(
        project.path(),
        &[
            "doc",
            &source_arg,
            "--output",
            &html_out_arg,
            "--format",
            "html",
        ],
    );
    assert_eq!(
        html_run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&html_run.stdout),
        String::from_utf8_lossy(&html_run.stderr)
    );
    let index_html = fs::read_to_string(html_out.join("index.html")).expect("read html docs");
    assert!(index_html.contains("searchBox"));
    assert!(index_html.contains("type-metric"));

    let all_out = project.path().join("target/docs-all");
    let all_out_arg = all_out.to_string_lossy().to_string();
    let all_run = run_aic_in_dir(
        project.path(),
        &["doc", &source_arg, "--output", &all_out_arg],
    );
    assert_eq!(
        all_run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&all_run.stdout),
        String::from_utf8_lossy(&all_run.stderr)
    );
    let all_stdout = String::from_utf8_lossy(&all_run.stdout);
    assert!(
        all_stdout
            .trim()
            .ends_with(&format!("{}/index.html", all_out_arg)),
        "expected default doc output to report index.html\nstdout={all_stdout}"
    );
    assert!(
        all_out.join("index.html").is_file(),
        "missing default html output"
    );
    assert!(
        all_out.join("index.md").is_file(),
        "missing default markdown output"
    );
    assert!(
        all_out.join("api.json").is_file(),
        "missing default json output"
    );
}

#[test]
fn doc_command_std_net_json_includes_all_declared_functions() {
    let root = repo_root();
    let source = root.join("std/net.aic");
    let project = tempdir().expect("project");
    let output_dir = project.path().join("target/std-net-docs");
    let output_arg = output_dir.to_string_lossy().to_string();
    let source_arg = source.to_string_lossy().to_string();

    let run = run_aic_in_dir(
        project.path(),
        &[
            "doc",
            &source_arg,
            "--output",
            &output_arg,
            "--format",
            "json",
        ],
    );
    assert_eq!(
        run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let source_text = fs::read_to_string(&source).expect("read std/net.aic");
    let mut declared_functions = source_text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed
                .strip_prefix("fn ")
                .or_else(|| trimmed.strip_prefix("intrinsic fn "))?;
            let (name, _) = rest.split_once('(')?;
            Some(name.trim().to_string())
        })
        .collect::<Vec<_>>();
    declared_functions.sort();
    declared_functions.dedup();

    let payload: Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("api.json")).expect("read api.json"),
    )
    .expect("parse api json");
    let modules = payload["modules"].as_array().expect("modules array");
    let std_net = modules
        .iter()
        .find(|module| module["module"].as_str() == Some("std.net"))
        .expect("std.net module in docs json");
    let mut documented_functions = std_net["items"]
        .as_array()
        .expect("items array")
        .iter()
        .filter(|item| item["kind"].as_str() == Some("fn"))
        .filter_map(|item| item["name"].as_str().map(ToString::to_string))
        .collect::<Vec<_>>();
    documented_functions.sort();
    documented_functions.dedup();

    assert_eq!(
        documented_functions, declared_functions,
        "std.net function coverage mismatch in aic doc json"
    );
}
