use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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

fn run_aic_with_env(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aic"));
    command.args(args).current_dir(repo_root());
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run aic with env")
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
    assert_eq!(
        String::from_utf8_lossy(&main_help.stdout),
        include_str!("golden/e7/help_main.txt")
    );

    let check_help = run_aic(&["check", "--help"]);
    assert!(check_help.status.success());
    assert_eq!(
        String::from_utf8_lossy(&check_help.stdout),
        include_str!("golden/e7/help_check.txt")
    );

    let test_help = run_aic(&["test", "--help"]);
    assert!(test_help.status.success());
    assert_eq!(
        String::from_utf8_lossy(&test_help.stdout),
        include_str!("golden/e7/help_test.txt")
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
        .any(|c| c["name"] == "lsp"));
    for phase in ["parse", "check", "build", "fix"] {
        assert!(contract_json["schemas"][phase]["path"].is_string());
        assert!(contract_json["examples"][phase].is_string());
    }
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
        "module util_pkg.main;\nfn value() -> Int { 42 }\n",
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
        "module inc_dep.main;\nfn base() -> Int { 40 }\n",
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
            "fn main() -> Int effects { io } {\n",
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
        "fn get() -> Int { 42 }",
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

fn main() -> Int effects { io } {
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
        "module util_pkg.main;\nfn value() -> Int { 7 }\n",
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
        "module inc_dep.main;\nfn base() -> Int { 41 }\n",
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
