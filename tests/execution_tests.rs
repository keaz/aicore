use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use aicore::codegen::{
    compile_with_clang, compile_with_clang_artifact, compile_with_clang_artifact_with_options,
    emit_llvm, emit_llvm_with_options, ArtifactKind, CodegenOptions, CompileOptions,
    OptimizationLevel,
};
use aicore::contracts::lower_runtime_asserts;
use aicore::driver::{has_errors, run_frontend};
use serde_json::Value;
use tempfile::tempdir;

fn compile_and_run_with_setup_and_args_and_input<F>(
    source: &str,
    args: &[&str],
    stdin_input: &str,
    setup: F,
) -> (i32, String, String)
where
    F: FnOnce(&Path),
{
    compile_and_run_with_setup_and_args_and_input_and_env(source, args, stdin_input, &[], setup)
}

fn compile_and_run_with_setup_and_args_and_input_and_env<F>(
    source: &str,
    args: &[&str],
    stdin_input: &str,
    envs: &[(&str, &str)],
    setup: F,
) -> (i32, String, String)
where
    F: FnOnce(&Path),
{
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("main.aic");
    fs::write(&src, source).expect("write source");

    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &src.to_string_lossy()).expect("emit llvm");

    let exe = dir.path().join("app");
    compile_with_clang(&llvm.llvm_ir, &exe, dir.path()).expect("clang build");

    setup(dir.path());
    let mut command = Command::new(exe);
    command
        .current_dir(dir.path())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("run exe");
    {
        let mut stdin = child.stdin.take().expect("child stdin");
        stdin
            .write_all(stdin_input.as_bytes())
            .expect("write stdin");
    }
    let output = child.wait_with_output().expect("run exe");
    (
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

struct ChildGuard(Option<Child>);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn compile_and_run_with_server_setup_and_args_and_input_and_env<F>(
    source: &str,
    args: &[&str],
    stdin_input: &str,
    envs: &[(&str, &str)],
    setup: F,
) -> (i32, String, String)
where
    F: FnOnce(&Path) -> Option<Child>,
{
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("main.aic");
    fs::write(&src, source).expect("write source");

    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &src.to_string_lossy()).expect("emit llvm");

    let exe = dir.path().join("app");
    compile_with_clang(&llvm.llvm_ir, &exe, dir.path()).expect("clang build");

    let _server_guard = ChildGuard(setup(dir.path()));
    let mut command = Command::new(exe);
    command
        .current_dir(dir.path())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("run exe");
    {
        let mut stdin = child.stdin.take().expect("child stdin");
        stdin
            .write_all(stdin_input.as_bytes())
            .expect("write stdin");
    }
    let output = child.wait_with_output().expect("run exe");
    (
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn compile_and_run_with_setup<F>(source: &str, setup: F) -> (i32, String, String)
where
    F: FnOnce(&Path),
{
    compile_and_run_with_setup_and_args_and_input(source, &[], "", setup)
}

fn compile_and_run_with_setup_and_args<F>(
    source: &str,
    args: &[&str],
    setup: F,
) -> (i32, String, String)
where
    F: FnOnce(&Path),
{
    compile_and_run_with_setup_and_args_and_input(source, args, "", setup)
}

fn compile_and_run(source: &str) -> (i32, String, String) {
    compile_and_run_with_setup(source, |_| {})
}

fn compile_and_run_or_backend_diags(
    source: &str,
) -> Result<(i32, String, String), Vec<aicore::diagnostics::Diagnostic>> {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("main.aic");
    fs::write(&src, source).expect("write source");

    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = match emit_llvm(&lowered, &src.to_string_lossy()) {
        Ok(llvm) => llvm,
        Err(diags) => return Err(diags),
    };

    let exe = dir.path().join("app");
    compile_with_clang(&llvm.llvm_ir, &exe, dir.path()).expect("clang build");

    let output = Command::new(exe)
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run exe");

    Ok((
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

fn assert_program_prints_42(source: &str) {
    let (code, stdout, stderr) = compile_and_run(source);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

fn assert_unsupported_map_key_diagnostic(source: &str) {
    match compile_and_run_or_backend_diags(source) {
        Ok((code, stdout, stderr)) => panic!(
            "expected backend key-type diagnostic, got success: code={code} stdout={stdout:?} stderr={stderr:?}"
        ),
        Err(diags) => {
            assert!(
                diags.iter().any(|d| {
                    d.code == "E5011"
                        && d.message
                            .contains("supports only map keys String, Int, and Bool")
                }),
                "diags={diags:#?}"
            );
        }
    }
}

fn compile_and_run_with_args(source: &str, args: &[&str]) -> (i32, String, String) {
    compile_and_run_with_setup_and_args(source, args, |_| {})
}

fn compile_and_run_with_input(source: &str, stdin_input: &str) -> (i32, String, String) {
    compile_and_run_with_setup_and_args_and_input(source, &[], stdin_input, |_| {})
}

fn tls_backend_enabled_for_tests() -> bool {
    Command::new("pkg-config")
        .arg("--exists")
        .arg("openssl")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn openssl_cli_available_for_tests() -> bool {
    Command::new("openssl")
        .arg("version")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn generate_local_tls_cert(root: &Path) {
    let cert_path = root.join("tls_cert.pem");
    let key_path = root.join("tls_key.pem");
    let req_status = Command::new("openssl")
        .current_dir(root)
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-sha256",
            "-nodes",
            "-days",
            "8",
            "-subj",
            "/CN=localhost",
            "-keyout",
            key_path.to_str().expect("key path"),
            "-out",
            cert_path.to_str().expect("cert path"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("generate tls cert");
    assert!(req_status.success(), "openssl req failed");
}

fn spawn_local_tls_server(root: &Path, port: u16, www: bool) -> Child {
    let mut args = vec![
        "s_server".to_string(),
        "-accept".to_string(),
        port.to_string(),
        "-cert".to_string(),
        "tls_cert.pem".to_string(),
        "-key".to_string(),
        "tls_key.pem".to_string(),
    ];
    if www {
        args.push("-www".to_string());
    }
    args.push("-quiet".to_string());

    Command::new("openssl")
        .current_dir(root)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start tls server")
}

fn wait_for_local_tls_server(port: u16, server: &mut Child) {
    let addr = format!("127.0.0.1:{port}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(&addr).is_ok() {
            return;
        }
        if let Some(status) = server.try_wait().expect("poll tls server") {
            panic!("openssl s_server exited early: {status}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("openssl s_server did not start listening on {addr}");
}

fn compile_to_llvm(path: &std::path::Path, source: &str) -> String {
    fs::write(path, source).expect("write source");
    let front = run_frontend(path).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &path.to_string_lossy()).expect("emit llvm");
    llvm.llvm_ir
}

fn run_binary_best_of(exe: &Path, cwd: &Path, repeats: usize) -> (Duration, String, String) {
    let mut best = Duration::MAX;
    let mut best_stdout = String::new();
    let mut best_stderr = String::new();

    for _ in 0..repeats {
        let started = Instant::now();
        let output = Command::new(exe)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("run benchmark exe");
        let elapsed = started.elapsed();
        assert!(
            output.status.success(),
            "benchmark exe failed: code={:?} stderr={} stdout={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        if elapsed < best {
            best = elapsed;
            best_stdout = String::from_utf8_lossy(&output.stdout).to_string();
            best_stderr = String::from_utf8_lossy(&output.stderr).to_string();
        }
    }

    (best, best_stdout, best_stderr)
}

#[test]
fn exec_option_match() {
    let src = r#"
import std.io;

fn maybe_even(x: Int) -> Option[Int] {
    if x % 2 == 0 { Some(x) } else { None() }
}

fn main() -> Int effects { io } capabilities { io  } {
    let out = match maybe_even(42) {
        None => 0,
        Some(v) => v,
    };
    print_int(out);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_panic_emits_backtrace_when_enabled() {
    let src = r#"
import std.io;

fn crash() -> Int effects { io } capabilities { io  } {
    panic("trace-check");
    0
}

fn main() -> Int effects { io } capabilities { io  } {
    crash()
}
"#;
    let (code, stdout, stderr) = compile_and_run_with_setup_and_args_and_input_and_env(
        src,
        &[],
        "",
        &[("AIC_BACKTRACE", "1")],
        |_| {},
    );
    assert_ne!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("AICore panic"), "stderr={stderr}");
    assert!(stderr.contains("trace-check"), "stderr={stderr}");
    assert!(stderr.contains("stack backtrace:"), "stderr={stderr}");
}

#[test]
fn exec_async_ping_flow() {
    let src = r#"
import std.io;

async fn ping(x: Int) -> Int {
    x + 1
}

async fn main() -> Int effects { io } capabilities { io  } {
    let value = await ping(41);
    print_int(value);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_async_await_submit_bridge_drives_reactor_without_task_spawn() {
    let src = r#"
import std.io;
import std.net;
import std.string;

fn err_code(err: NetError) -> Int {
    match err {
        Timeout => 4,
        ConnectionClosed => 8,
        Cancelled => 9,
        InvalidInput => 6,
        _ => 7,
    }
}

async fn main() -> Int effects { io, net, concurrency } capabilities { io, net, concurrency  } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };

    let accepted = await async_accept_submit(listener, 2000);
    let timeout_code = match accepted {
        Ok(_) => 0,
        Err(err) => err_code(err),
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
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_async_accept_1000_connections_single_thread() {
    let src = r#"
import std.io;
import std.net;
import std.vec;

fn main() -> Int effects { io, net, concurrency, env } capabilities { io, net, concurrency, env } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };

    let mut pending: Vec[AsyncIntOp] = vec.new_vec();
    let mut submitted = 0;
    let mut resolved = 0;
    while submitted < 1000 {
        let handle = match async_accept_submit(listener, 50) {
            Ok(op) => op.handle,
            Err(_) => 0,
        };
        if handle > 0 {
            pending = vec.push(pending, AsyncIntOp { handle: handle });
            submitted = submitted + 1;
        } else {
            if pending.len > 0 {
                let op = match vec.get(pending, 0) {
                    Some(value) => value,
                    None => AsyncIntOp { handle: 0 },
                };
                pending = vec.remove_at(pending, 0);
                async_wait_int(op, 1000);
                resolved = resolved + 1;
            } else {
                resolved = resolved;
            };
        };
    };

    while pending.len > 0 {
        let op = match vec.get(pending, 0) {
            Some(value) => value,
            None => AsyncIntOp { handle: 0 },
        };
        pending = vec.remove_at(pending, 0);
        async_wait_int(op, 1000);
        resolved = resolved + 1;
    };

    let shutdown_ok = match async_shutdown() {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let listener_closed = match tcp_close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if submitted == 1000 && resolved == 1000 && shutdown_ok == 1 && listener_closed == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_first_class_fn_value_from_named_function() {
    let src = r#"
import std.io;

fn add2(x: Int) -> Int {
    x + 2
}

fn apply(f: Fn(Int) -> Int, value: Int) -> Int {
    f(value)
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    print_int(apply(add2, 40));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_closure_literal_invocation() {
    let src = r#"
import std.io;

fn apply(f: Fn(Int) -> Int, value: Int) -> Int {
    f(value)
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let inc = |x: Int| -> Int { x + 1 };
    print_int(apply(inc, 41));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_closure_capture_from_outer_scope() {
    let src = r#"
import std.io;

fn main() -> Int effects { io, env } capabilities { io, env } {
    let base = 41;
    let add = |x: Int| -> Int { x + base };
    print_int(add(1));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_while_and_continue_flow() {
    let src = r#"
import std.io;

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut i = 5;
    let mut total = 0;
    while i > 0 {
        if i == 3 {
            i = i - 1;
            continue;
        } else {
            ()
        };
        total = total + i;
        i = i - 1;
    };
    print_int(total);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "12\n");
}

#[test]
fn exec_loop_break_value() {
    let src = r#"
import std.io;

fn main() -> Int effects { io, env } capabilities { io, env } {
    let value = loop {
        break 42
    };
    print_int(value);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_for_range_literal_with_continue_and_break() {
    let src = r#"
import std.io;

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut total = 0;
    for i in 0..8 {
        if i == 2 {
            continue;
        } else {
            ()
        };
        if i == 6 {
            break;
        } else {
            ()
        };
        total = total + i;
    };
    print_int(total);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "13\n");
}

#[test]
fn exec_for_range_function_and_vec_iteration() {
    let src = r#"
import std.io;
import std.vec;

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut from_range = 0;
    for i in range(1, 5) {
        from_range = from_range + i;
    };

    let mut v: Vec[Int] = vec.new_vec();
    v = vec.push(v, 4);
    v = vec.push(v, 7);

    let mut from_vec = 0;
    for item in v {
        from_vec = from_vec + item;
    };

    print_int(from_range + from_vec);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "21\n");
}

#[test]
fn exec_for_in_map_and_set_use_iterator_trait() {
    let src = r#"
import std.io;
import std.map;
import std.set;

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut total = 0;

    let mut m: Map[String, Int] = map.new_map();
    m = map.insert(m, "a", 20);
    m = map.insert(m, "b", 22);
    for entry in m {
        total = total + entry.value;
    };

    let mut s: Set[Int] = set.new_set();
    s = set.add(s, 3);
    s = set.add(s, 3);
    s = set.add(s, 4);
    for value in s {
        total = total + value;
    };

    print_int(total);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "49\n");
}

#[test]
fn exec_iterator_adapters_map_filter_take_skip_enumerate_zip_chain() {
    let src = r#"
import std.io;
import std.vec;
import std.iterator;

fn gt_two(x: Int) -> Bool { x > 2 }
fn double(x: Int) -> Int { x * 2 }

fn vec1234() -> Vec[Int] {
    let mut v: Vec[Int] = vec.new_vec();
    v = vec.push(v, 1);
    v = vec.push(v, 2);
    v = vec.push(v, 3);
    v = vec.push(v, 4);
    v
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let piped = vec1234().iter().map(double).filter(gt_two).skip(1).take(2).collect();
    let enumed = vec1234().iter().enumerate().take(2).collect();
    let zipped = vec1234().iter().zip(vec1234().iter().skip(2)).collect();
    let chained = vec1234().iter().take(2).chain(vec1234().iter().skip(3)).collect();

    let mut piped_total = 0;
    for value in piped {
        piped_total = piped_total + value;
    };

    let mut enum_total = 0;
    for entry in enumed {
        enum_total = enum_total + entry.index + entry.value;
    };

    let mut zip_total = 0;
    for pair in zipped {
        zip_total = zip_total + pair.left + pair.right;
    };

    let mut chain_total = 0;
    for value in chained {
        chain_total = chain_total + value;
    };

    print_int(piped_total + enum_total + zip_total + chain_total);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "35\n");
}

#[test]
fn exec_custom_iterator_trait_impl_works_with_for_in() {
    let src = r#"
import std.io;
import std.iterator;

struct Counter {
    current: Int,
    limit: Int,
}

impl Iterator[Counter, Int] {
    fn next[T, U](self: Counter) -> IterStep[Int, Counter] {
        if self.current < self.limit {
            IterStep {
                item: Some(self.current),
                iter: Counter {
                    current: self.current + 1,
                    limit: self.limit,
                },
            }
        } else {
            IterStep {
                item: None(),
                iter: self,
            }
        }
    }
}

fn make_counter(limit: Int) -> Counter {
    Counter {
        current: 0,
        limit: limit,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut total = 0;
    for value in make_counter(5) {
        total = total + value;
    };
    print_int(total);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "10\n");
}

#[test]
fn exec_for_map_entries_iterates_deterministically() {
    let src = r#"
import std.io;
import std.map;
import std.vec;

fn main() -> Int effects { io } capabilities { io  } {
    let mut m: Map[String, Int] = map.new_map();
    m = map.insert(m, "b", 2);
    m = map.insert(m, "a", 40);

    let mut total = 0;
    for entry in map.entries(m) {
        total = total + entry.value;
    };

    print_int(total);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_trait_bounded_generic_dispatch() {
    let src = r#"
import std.io;

trait Order[T];
impl Order[Int];

fn pick[T: Order](a: T, b: T) -> T {
    a
}

fn main() -> Int effects { io } capabilities { io  } {
    print_int(pick(42, 7));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_trait_method_static_dispatch_returns_deterministic_value() {
    let src = r#"
import std.io;

trait Score[T] {
    fn score(self: T) -> Int;
}

struct Meter { value: Int }

impl Score[Meter] {
    fn score(self: Meter) -> Int {
        self.value + 1
    }
}

fn eval[T: Score](x: T) -> Int {
    x.score()
}

fn main() -> Int effects { io } capabilities { io  } {
    print_int(eval(Meter { value: 41 }));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_dyn_trait_direct_dispatch_vtable_call() {
    let src = r#"
import std.io;

trait Handler {
    fn value(self: Self) -> Int;
}

struct PlusOne { base: Int }

impl Handler[PlusOne] {
    fn value(self: PlusOne) -> Int {
        self.base + 1
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let h: dyn Handler = PlusOne { base: 41 };
    print_int(h.value());
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_dyn_trait_vec_and_option_dispatch() {
    let src = r#"
import std.io;
import std.vec;

trait Handler {
    fn value(self: Self) -> Int;
}

struct PlusOne { base: Int }
struct PlusTwo { base: Int }

impl Handler[PlusOne] {
    fn value(self: PlusOne) -> Int {
        self.base + 1
    }
}

impl Handler[PlusTwo] {
    fn value(self: PlusTwo) -> Int {
        self.base + 2
    }
}

fn score_opt(v: Option[dyn Handler]) -> Int {
    match v {
        Some(h) => h.value(),
        None => 0,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut hs: Vec[dyn Handler] = vec.new_vec();
    hs = vec.push(hs, PlusOne { base: 40 });
    hs = vec.push(hs, PlusTwo { base: 40 });

    let a = match vec.get(hs, 0) {
        Some(h) => h.value(),
        None => 0,
    };
    let b = score_opt(Some(PlusTwo { base: 8 }));
    print_int(a + b);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "51\n");
}

#[test]
fn exec_borrow_checker_reinitialize_after_move() {
    let src = r#"
import std.io;

struct BoxedInt { value: Int }

fn main() -> Int effects { io } capabilities { io  } {
    let mut b = BoxedInt { value: 1 };
    let moved = b;
    let first = moved.value;
    b = BoxedInt { value: first + 1 };
    print_int(b.value);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "2\n");
}

#[test]
fn exec_tuple_types_destructure_match_and_field_access() {
    let src = r#"
import std.io;

fn swap(a: Int, b: Int) -> (Int, Int) {
    (b, a)
}

fn tuple_first[T, U](pair: (T, U)) -> T {
    pair.0
}

fn main() -> Int effects { io } capabilities { io  } {
    let pair = swap(2, 40);
    let (left, right) = pair;
    let matched = match pair {
        (40, value) => value,
        _ => 0,
    };
    print_int(tuple_first((left + matched, right)));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_struct_methods_associated_and_instance_calls() {
    let src = r#"
import std.io;

struct User { age: Int }

impl User {
    fn new(age: Int) -> User {
        User { age: age }
    }

    fn age_plus(self) -> Int {
        self.age + 12
    }

    fn is_adult(self) -> Bool {
        self.age >= 18
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let user = User::new(30);
    let score = if user.is_adult() { user.age_plus() } else { 0 };
    print_int(score);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_struct_defaults_fill_missing_fields_and_support_default_method() {
    let src = r#"
import std.io;

struct ServerConfig {
    host: String = "0.0.0.0",
    port: Int = 8080,
    max_connections: Int = 1000,
    timeout_ms: Int = 30000,
}

fn main() -> Int effects { io } capabilities { io  } {
    let cfg = ServerConfig { port: 9090 };
    let defaults = ServerConfig::default();
    let score = cfg.port + defaults.max_connections / 1000 + defaults.timeout_ms / 10000;
    print_int(score);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "9094\n");
}

#[test]
fn exec_struct_defaults_allow_const_arithmetic() {
    let src = r#"
import std.io;

const BASE: Int = 40;

struct Config {
    port: Int = BASE + 2,
    retries: Int = 1 + 2 * 3,
}

fn main() -> Int effects { io } capabilities { io  } {
    let cfg = Config { };
    print_int(cfg.port + cfg.retries);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "49\n");
}

#[test]
fn exec_option_methods_map_and_then_unwrap_or_chain() {
    let src = r#"
import std.io;
import std.option;

fn add_one(x: Int) -> Int {
    x + 1
}

fn keep_even(x: Int) -> Option[Int] {
    if x % 2 == 0 { Some(x) } else { None() }
}

fn main() -> Int effects { io } capabilities { io  } {
    let a = Some(41).map(add_one).and_then(keep_even).unwrap_or(0);
    let b = Some(20).map(add_one).and_then(keep_even).unwrap_or(4);
    let c = None().map(add_one).unwrap_or(5);
    print_int(a + b + c);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "51\n");
}

#[test]
fn exec_result_methods_map_and_then_unwrap_or_chain() {
    let src = r#"
import std.io;
import std.result;

fn add_one(x: Int) -> Int {
    x + 1
}

fn keep_even(x: Int) -> Result[Int, Int] {
    if x % 2 == 0 { Ok(x) } else { Err(0 - x) }
}

fn main() -> Int effects { io } capabilities { io  } {
    let ok_value: Result[Int, Int] = Ok(41);
    let odd_value: Result[Int, Int] = Ok(3);
    let err_value: Result[Int, Int] = Err(7);
    let a = ok_value.map(add_one).and_then(keep_even).unwrap_or(0);
    let b = odd_value.and_then(keep_even).unwrap_or(8);
    let c = err_value.map(add_one).unwrap_or(5);
    print_int(a + b + c);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "55\n");
}

#[test]
fn exec_result_propagation_short_circuits_err() {
    let src = r#"
import std.io;

fn ensure_non_negative(x: Int) -> Result[Int, Int] {
    if x >= 0 { Ok(x) } else { Err(0 - x) }
}

fn bump_checked(x: Int) -> Result[Int, Int] {
    let base = ensure_non_negative(x)?;
    if true { Ok(base + 1) } else { Err(0) }
}

fn unwrap_or_neg(v: Result[Int, Int]) -> Int {
    match v {
        Ok(value) => value,
        Err(err) => 0 - err,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    print_int(unwrap_or_neg(bump_checked(-42)));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "-42\n");
}

#[test]
fn exec_mutable_vec_update_flow() {
    let src = r#"
import std.io;
import std.vec;

fn grow(v: Vec[Int]) -> Vec[Int] {
    let next: Vec[Int] = Vec { ptr: v.ptr, len: v.len + 1, cap: v.cap };
    next
}

fn main() -> Int effects { io } capabilities { io  } {
    let mut v: Vec[Int] = Vec { ptr: 0, len: 1, cap: 4 };
    v = grow(v);
    print_int(vec_len(v));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "2\n");
}

#[test]
fn exec_vec_push_pop_lifecycle_and_bounds() {
    let src = r#"
import std.io;
import std.vec;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut v: Vec[Int] = vec.new_vec();
    v = vec.push(v, 10);
    v = vec.push(v, 20);

    let len_after_push_ok = if vec.vec_len(v) == 2 { 1 } else { 0 };
    let first_ok = if opt_int_or(vec.first(v), -1) == 10 { 1 } else { 0 };
    let last_ok = if opt_int_or(vec.last(v), -1) == 20 { 1 } else { 0 };
    let oob_get_ok = match vec.get(v, 5) {
        None => 1,
        Some(_) => 0,
    };

    v = vec.pop(v);
    let after_pop_len_ok = if vec.vec_len(v) == 1 { 1 } else { 0 };
    let after_pop_value_ok = if opt_int_or(vec.get(v, 0), -1) == 10 { 1 } else { 0 };

    v = vec.pop(v);
    v = vec.pop(v);
    let empty_ok = if vec.is_empty(v) { 1 } else { 0 };
    let empty_first_ok = match vec.first(v) {
        None => 1,
        Some(_) => 0,
    };
    let empty_last_ok = match vec.last(v) {
        None => 1,
        Some(_) => 0,
    };

    let score = len_after_push_ok + first_ok + last_ok + oob_get_ok +
        after_pop_len_ok + after_pop_value_ok + empty_ok + empty_first_ok + empty_last_ok;
    print_int(score);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "9\n");
}

#[test]
fn exec_vec_set_insert_remove_reverse_slice_append() {
    let src = r#"
import std.io;
import std.vec;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut v: Vec[Int] = vec.vec_of(2);
    v = vec.insert(v, 0, 1);
    v = vec.push(v, 4);
    v = vec.insert(v, 2, 3);
    v = vec.set(v, 3, 40);
    v = vec.remove_at(v, 1);
    v = vec.reverse(v);

    let s = vec.slice(v, 1, 3);
    let a = vec.append(s, vec.vec_of(9));
    let set_oob = vec.set(a, 8, 0);
    let insert_oob = vec.insert(set_oob, 9, 5);
    let remove_oob = vec.remove_at(insert_oob, 9);

    let len_ok = if vec.vec_len(remove_oob) == 3 { 1 } else { 0 };
    let e0_ok = if opt_int_or(vec.get(remove_oob, 0), -1) == 3 { 1 } else { 0 };
    let e1_ok = if opt_int_or(vec.get(remove_oob, 1), -1) == 1 { 1 } else { 0 };
    let e2_ok = if opt_int_or(vec.get(remove_oob, 2), -1) == 9 { 1 } else { 0 };
    let oob_len_ok = if vec.vec_len(remove_oob) == vec.vec_len(a) { 1 } else { 0 };
    let head_ok = if opt_int_or(vec.get(remove_oob, 0), -1) == 3 { 1 } else { 0 };
    let tail_ok = if opt_int_or(vec.last(remove_oob), -1) == 9 { 1 } else { 0 };

    let score = len_ok + e0_ok + e1_ok + e2_ok + oob_len_ok + head_ok + tail_ok;
    print_int(score);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "7\n");
}

#[test]
fn exec_vec_contains_index_of_monomorphized_types() {
    let src = r#"
import std.io;
import std.vec;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let mut vs: Vec[String] = vec.new_vec();
    vs = vec.push(vs, "red");
    vs = vec.push(vs, "blue");
    let str_contains_ok = if vec.contains(vs, "blue") { 1 } else { 0 };
    let str_index_ok = if opt_int_or(vec.index_of(vs, "red"), -1) == 0 { 1 } else { 0 };
    let str_missing_ok = if opt_int_or(vec.index_of(vs, "green"), -1) == -1 { 1 } else { 0 };

    let mut vb: Vec[Bool] = vec.new_vec();
    vb = vec.push(vb, true);
    vb = vec.push(vb, false);
    let bool_contains_ok = if vec.contains(vb, false) { 1 } else { 0 };
    let bool_index_ok = if opt_int_or(vec.index_of(vb, false), -1) == 1 { 1 } else { 0 };

    let mut vo: Vec[Option[Int]] = vec.new_vec();
    vo = vec.push(vo, None());
    vo = vec.push(vo, Some(7));
    let opt_contains_none_ok = if vec.contains(vo, None()) { 1 } else { 0 };
    let opt_contains_some_ok = if vec.contains(vo, Some(7)) { 1 } else { 0 };
    let opt_index_ok = if opt_int_or(vec.index_of(vo, Some(7)), -1) == 1 { 1 } else { 0 };

    let score = str_contains_ok + str_index_ok + str_missing_ok +
        bool_contains_ok + bool_index_ok +
        opt_contains_none_ok + opt_contains_some_ok + opt_index_ok;
    print_int(score);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "8\n");
}

#[test]
fn exec_vec_empty_edge_cases() {
    let src = r#"
import std.io;
import std.vec;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut v: Vec[Int] = vec.new_vec();
    let get_empty_ok = match vec.get(v, 0) {
        None => 1,
        Some(_) => 0,
    };
    let first_empty_ok = match vec.first(v) {
        None => 1,
        Some(_) => 0,
    };
    let last_empty_ok = match vec.last(v) {
        None => 1,
        Some(_) => 0,
    };
    let contains_empty_ok = if vec.contains(v, 1) { 0 } else { 1 };
    let index_empty_ok = if opt_int_or(vec.index_of(v, 1), -1) == -1 { 1 } else { 0 };

    let reversed = vec.reverse(v);
    let reversed_ok = if vec.vec_len(reversed) == 0 { 1 } else { 0 };
    let sliced = vec.slice(v, 0, 4);
    let sliced_ok = if vec.vec_len(sliced) == 0 { 1 } else { 0 };
    let empty2: Vec[Int] = vec.new_vec();
    let appended = vec.append(v, empty2);
    let appended_ok = if vec.vec_len(appended) == 0 { 1 } else { 0 };

    v = vec.push(v, 5);
    v = vec.clear(v);
    let clear_ok = if vec.is_empty(v) { 1 } else { 0 };
    v = vec.remove_at(v, 0);
    v = vec.pop(v);
    v = vec.set(v, 0, 1);
    let stable_empty_ok = if vec.vec_len(v) == 0 { 1 } else { 0 };

    let score = get_empty_ok + first_empty_ok + last_empty_ok + contains_empty_ok + index_empty_ok +
        reversed_ok + sliced_ok + appended_ok + clear_ok + stable_empty_ok;
    print_int(score);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "10\n");
}

#[test]
fn exec_vec_capacity_preallocation_reserve_shrink_and_growth() {
    let src = r#"
import std.io;
import std.vec;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut v: Vec[Int] = vec.new_vec_with_capacity(5);
    let init_cap = vec.vec_cap(v);
    let init_cap_ok = if init_cap == 5 { 1 } else { 0 };

    let mut i = 0;
    while i < 5 {
        v = vec.push(v, i);
        i = i + 1;
    };
    let prealloc_ok = if vec.vec_cap(v) == init_cap { 1 } else { 0 };

    v = vec.push(v, 5);
    let growth_2x_ok = if vec.vec_cap(v) == init_cap * 2 { 1 } else { 0 };

    v = vec.reserve(v, 15);
    let reserved_cap = vec.vec_cap(v);
    let reserve_need_ok = if reserved_cap >= vec.vec_len(v) + 15 { 1 } else { 0 };

    let mut j = 6;
    while j < 21 {
        v = vec.push(v, j);
        j = j + 1;
    };
    let reserve_no_realloc_ok = if vec.vec_cap(v) == reserved_cap { 1 } else { 0 };

    v = vec.shrink_to_fit(v);
    let shrink_len_ok = if vec.vec_cap(v) == vec.vec_len(v) { 1 } else { 0 };
    let value_ok = if opt_int_or(vec.get(v, 20), -1) == 20 { 1 } else { 0 };

    let cap_after_first_shrink = vec.vec_cap(v);
    v = vec.shrink_to_fit(v);
    let shrink_idempotent_ok = if vec.vec_cap(v) == cap_after_first_shrink { 1 } else { 0 };

    let score = init_cap_ok + prealloc_ok + growth_2x_ok + reserve_need_ok +
        reserve_no_realloc_ok + shrink_len_ok + value_ok + shrink_idempotent_ok;
    print_int(score);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "8\n");
}

#[test]
fn exec_abs_if_expression() {
    let src = r#"
import std.io;

fn abs(x: Int) -> Int {
    if x >= 0 { x } else { 0 - x }
}

fn main() -> Int effects { io } capabilities { io  } {
    print_int(abs(-7));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "7\n");
}

#[test]
fn exec_bool_match() {
    let src = r#"
import std.io;

fn as_int(b: Bool) -> Int {
    match b {
        true => 1,
        false => 0,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    print_int(as_int(true));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "1\n");
}

#[test]
fn exec_bool_or_pattern() {
    let src = r#"
import std.io;

fn collapse(b: Bool) -> Int {
    match b {
        true | false => 42,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    print_int(collapse(false));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_enum_or_pattern() {
    let src = r#"
import std.io;

fn collapse(x: Option[Int]) -> Int {
    match x {
        None | Some(_) => 42,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    print_int(collapse(Some(0)));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_match_guard_backend_reports_e5023() {
    let source = r#"
import std.io;

fn f(x: Bool, allow: Bool) -> Int {
    match x {
        true if allow => 1,
        false => 0,
        _ => 2,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    print_int(f(true, true));
    0
}
"#;

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("main.aic");
    fs::write(&src, source).expect("write source");

    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let diags = match emit_llvm(&lowered, &src.to_string_lossy()) {
        Ok(_) => panic!("expected backend error"),
        Err(diags) => diags,
    };
    assert!(diags.iter().any(|d| d.code == "E5023"), "diags={diags:#?}");
}

#[test]
fn exec_nested_adt_match() {
    let src = r#"
import std.io;

struct Pair {
    left: Int,
    right: Int,
}

enum Wrap[T] {
    Empty,
    Full(T),
}

fn fold(x: Wrap[Pair]) -> Int {
    match x {
        Empty => 0,
        Full(p) => p.left + p.right,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    print_int(fold(Full(Pair { left: 20, right: 22 })));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_generic_monomorphization_multiple_concrete_types() {
    let src = r#"
import std.io;

fn id[T](x: T) -> T {
    x
}

fn as_int(b: Bool) -> Int {
    match b {
        true => 1,
        false => 0,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let a = id(41);
    let b = id(true);
    print_int(a + as_int(b));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_string_len() {
    let src = r#"
import std.io;
import std.string;

fn main() -> Int effects { io } capabilities { io  } {
    print_int(len("abc"));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "3\n");
}

#[test]
fn exec_string_format_positional_placeholders_and_composition() {
    let src = r#"
import std.io;
import std.string;
import std.vec;

fn main() -> Int effects { io } capabilities { io  } {
    let empty_args: Vec[String] = Vec {
        ptr: 0,
        len: 0,
        cap: 0,
    };

    let out0 = format("plain-text", empty_args);
    let ok0 = if len(out0) == 10 && string.starts_with(out0, "plain") { 1 } else { 0 };

    let out1 = format("x{0}y", split("A", ","));
    let ok1 = if len(out1) == 3 && string.contains(out1, "A") { 1 } else { 0 };

    let out2 = format("{0}-{1}", split("left,right", ","));
    let ok2 = if string.starts_with(out2, "left-") && string.ends_with(out2, "right") { 1 } else { 0 };

    let out5 = format("{0}{1}{2}{3}{4}", split("a,b,c,d,e", ","));
    let ok5 = if len(out5) == 5 && string.starts_with(out5, "ab") && string.ends_with(out5, "de") {
        1
    } else {
        0
    };

    let missing = format("x{0}-{2}-z", split("left,right", ","));
    let missing_ok =
        if string.starts_with(missing, "xleft-") && string.contains(missing, "{2}") && string.ends_with(missing, "-z") {
            1
        } else {
            0
        };

    let int_text = int_to_string(-2048);
    let int_direct_ok = if len(int_text) == 5 && string.starts_with(int_text, "-") { 1 } else { 0 };
    let int_args = split("left7right", int_to_string(7));
    let int_compose_ok = if len(format("{0}:{1}", int_args)) == 10 { 1 } else { 0 };

    let bool_true_text = bool_to_string(true);
    let bool_false_text = bool_to_string(false);
    let bool_direct_ok =
        if len(bool_true_text) == 4 && len(bool_false_text) == 5 {
            1
        } else {
            0
        };
    let bool_args = split("uptruedown", bool_to_string(true));
    let bool_compose_ok = if string.starts_with(format("{0}|{1}", bool_args), "up|") { 1 } else { 0 };

    let score =
        ok0 +
        ok1 +
        ok2 +
        ok5 +
        missing_ok +
        int_direct_ok +
        int_compose_ok +
        bool_direct_ok +
        bool_compose_ok;

    if score == 9 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_template_literals_match_concat_and_support_escapes() {
    let src = r#"
import std.io;
import std.string;

fn main() -> Int effects { io } capabilities { io  } {
    let name = "Ada";

    let basic = f"Hello, {name}";
    let basic_ok =
        if len(basic) == 10 && string.starts_with(basic, "Hello,") && string.ends_with(basic, "Ada") {
            1
        } else {
            0
        };

    let nested = f"sum={int_to_string(20 + 22)}";
    let nested_ok = if len(nested) == 6 && string.starts_with(nested, "sum=") && string.ends_with(nested, "42") {
        1
    } else {
        0
    };

    let escaped = f"left \{literal\} right";
    let escaped_ok =
        if len(escaped) == 20 && string.starts_with(escaped, "left {") && string.ends_with(escaped, "} right") {
            1
        } else {
            0
        };

    let escaped_doubled = f"left {{literal}} right";
    let escaped_doubled_ok =
        if len(escaped_doubled) == 20
            && string.starts_with(escaped_doubled, "left {")
            && string.ends_with(escaped_doubled, "} right") {
            1
        } else {
            0
        };

    let mixed = $"<{name}:{int_to_string(7)}>";
    let mixed_ok = if len(mixed) == 7 && string.starts_with(mixed, "<Ada:") && string.ends_with(mixed, "7>") {
        1
    } else {
        0
    };
    if basic_ok + nested_ok + escaped_ok + escaped_doubled_ok + mixed_ok == 5 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}
#[test]
fn exec_string_ops_full_surface_and_edge_cases() {
    let src = r#"
import std.io;
import std.option;
import std.result;
import std.string;
import std.vec;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn opt_string_len(v: Option[String]) -> Int {
    match v {
        Some(value) => len(value),
        None => 0,
    }
}

fn opt_vec_len(v: Option[Vec[String]]) -> Int {
    match v {
        Some(value) => vec_len(value),
        None => 0,
    }
}

fn result_int_or(v: Result[Int, String], fallback: Int) -> Int {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn result_err_non_empty(v: Result[Int, String]) -> Int {
    match v {
        Ok(_) => 0,
        Err(message) => if len(message) > 0 { 1 } else { 0 },
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let contains_ok = if string.contains("alpha beta", "pha") { 1 } else { 0 };
    let starts_ok = if string.starts_with("alpha beta", "alpha") { 1 } else { 0 };
    let ends_ok = if string.ends_with("alpha beta", "beta") { 1 } else { 0 };

    let idx_first_ok = if opt_int_or(string.index_of("banana", "na"), -1) == 2 { 1 } else { 0 };
    let idx_last_ok = if opt_int_or(last_index_of("banana", "na"), -1) == 4 { 1 } else { 0 };
    let idx_none_ok = if opt_int_or(string.index_of("banana", "zz"), -1) == -1 { 1 } else { 0 };

    let sub_ok = if len(substring("header", 1, 4)) == 3 { 1 } else { 0 };
    let sub_oob_ok = if len(substring("abc", 9, 12)) == 0 { 1 } else { 0 };
    let char_ok = if opt_string_len(char_at("abc", 1)) == 1 { 1 } else { 0 };
    let char_oob_ok = if opt_string_len(char_at("abc", 9)) == 0 { 1 } else { 0 };

    let split_ok = if vec_len(split("GET /api/users HTTP/1.1", " ")) == 3 { 1 } else { 0 };
    let split_first_ok =
        if opt_vec_len(split_first("Content-Type: application/json", ":")) == 2 {
            1
        } else {
            0
        };
    let split_first_none_ok =
        if opt_vec_len(split_first("Content-Type application/json", ":")) == 0 {
            1
        } else {
            0
        };

    let trim_ok = if len(trim(" \t  hello \n")) == 5 { 1 } else { 0 };
    let trim_start_ok = if len(trim_start("   hello ")) == 6 { 1 } else { 0 };
    let trim_end_ok = if len(trim_end(" hello   ")) == 6 { 1 } else { 0 };

    let upper_ok = if len(to_upper("abcXYZ")) == 6 { 1 } else { 0 };
    let lower_ok = if len(to_lower("ABCxyz")) == 6 { 1 } else { 0 };
    let replace_ok = if len(replace("a-b-c", "-", "/")) == 5 { 1 } else { 0 };
    let repeat_ok = if len(repeat("ab", 3)) == 6 { 1 } else { 0 };
    let repeat_neg_ok = if len(repeat("ab", -2)) == 0 { 1 } else { 0 };

    let parse_ok = if result_int_or(parse_int("  -42 "), 0) == -42 { 1 } else { 0 };
    let parse_bad_ok = result_err_non_empty(parse_int("12x"));
    let parse_overflow_ok = result_err_non_empty(parse_int("999999999999999999999999"));

    let int_to_string_ok = if len(int_to_string(-2048)) == 5 { 1 } else { 0 };
    let bool_true_ok = if len(bool_to_string(true)) == 4 { 1 } else { 0 };
    let bool_false_ok = if len(bool_to_string(false)) == 5 { 1 } else { 0 };

    let joined = join(split("a,b,c", ","), "|");
    let join_ok = if len(joined) == 5 { 1 } else { 0 };
    let join_empty_ok = if len(join(split("", ","), "|")) == 0 { 1 } else { 0 };

    let score =
        contains_ok +
        starts_ok +
        ends_ok +
        idx_first_ok +
        idx_last_ok +
        idx_none_ok +
        sub_ok +
        sub_oob_ok +
        char_ok +
        char_oob_ok +
        split_ok +
        split_first_ok +
        split_first_none_ok +
        trim_ok +
        trim_start_ok +
        trim_end_ok +
        upper_ok +
        lower_ok +
        replace_ok +
        repeat_ok +
        repeat_neg_ok +
        parse_ok +
        parse_bad_ok +
        parse_overflow_ok +
        int_to_string_ok +
        bool_true_ok +
        bool_false_ok +
        join_ok +
        join_empty_ok;

    if score == 29 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_char_ops_cover_ascii_and_unicode() {
    let src = r#"
import std.char;
import std.io;
import std.option;
import std.vec;

fn opt_char_code(v: Option[Char], fallback: Int) -> Int {
    match v {
        Some(value) => char_to_int(value),
        None => fallback,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let digit_ok = if is_digit('5') { 1 } else { 0 };
    let alpha_ok = if is_alpha('A') { 1 } else { 0 };
    let whitespace_ok = if is_whitespace('\n') { 1 } else { 0 };
    let to_int_ok = if char_to_int('A') == 65 { 1 } else { 0 };
    let emoji_ok = if char_to_int('😀') == 128512 { 1 } else { 0 };

    let from_int_ok = if opt_char_code(int_to_char(128512), -1) == 128512 { 1 } else { 0 };
    let invalid_low_ok = if opt_char_code(int_to_char(-1), -1) == -1 { 1 } else { 0 };
    let invalid_surrogate_ok = if opt_char_code(int_to_char(55296), -1) == -1 { 1 } else { 0 };

    let ascii_chars_ok = if vec_len(chars("hello")) == 5 { 1 } else { 0 };
    let unicode_chars_ok = if vec_len(chars("hé😀")) == 3 { 1 } else { 0 };
    let roundtrip_ok = if vec_len(chars(from_chars(chars("hé😀")))) == 3 { 1 } else { 0 };

    let score =
        digit_ok +
        alpha_ok +
        whitespace_ok +
        to_int_ok +
        emoji_ok +
        from_int_ok +
        invalid_low_ok +
        invalid_surrogate_ok +
        ascii_chars_ok +
        unicode_chars_ok +
        roundtrip_ok;

    if score == 11 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_string_encoding_conversions_cover_ascii_utf8_invalid_and_empty() {
    let src = r#"
import std.io;
import std.fs;
import std.string;

fn encoding_err_code(err: EncodingError) -> Int {
    match err {
        InvalidSequence => 1,
        UnsupportedEncoding => 2,
        BufferTooSmall => 3,
    }
}

fn load_invalid() -> Bytes effects { fs } capabilities { fs  } {
    match read_bytes("invalid.bin") {
        Ok(value) => Bytes { data: value.data },
        Err(_) => string_to_bytes(""),
    }
}

fn decode_len(v: Result[String, EncodingError]) -> Int {
    match v {
        Ok(text) => len(text),
        Err(_) => -1,
    }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let ascii_len_ok = if byte_length("hello") == 5 { 1 } else { 0 };
    let ascii_flag_ok = if is_ascii("hello") { 1 } else { 0 };
    let ascii_valid_ok = if string.is_valid_utf8(string_to_bytes("hello")) { 1 } else { 0 };
    let ascii_roundtrip_ok = if decode_len(bytes_to_string(string_to_bytes("hello"))) == 5 { 1 } else { 0 };
    let ascii_lossy_ok = if len(bytes_to_string_lossy(string_to_bytes("hello"))) == 5 { 1 } else { 0 };

    let multi = "hé";
    let multi_len_ok = if byte_length(multi) == 3 { 1 } else { 0 };
    let multi_ascii_ok = if is_ascii(multi) { 0 } else { 1 };
    let multi_valid_ok = if string.is_valid_utf8(string_to_bytes(multi)) { 1 } else { 0 };
    let multi_roundtrip_ok = if decode_len(bytes_to_string(string_to_bytes(multi))) == 3 { 1 } else { 0 };
    let multi_lossy_ok = if len(bytes_to_string_lossy(string_to_bytes(multi))) == 3 { 1 } else { 0 };

    let empty_ok =
        if decode_len(bytes_to_string(string_to_bytes(""))) == 0 &&
            len(bytes_to_string_lossy(string_to_bytes(""))) == 0 &&
            string.is_valid_utf8(string_to_bytes("")) {
            1
        } else {
            0
        };

    let invalid_valid_ok = if string.is_valid_utf8(load_invalid()) { 0 } else { 1 };
    let invalid_decode_ok = match bytes_to_string(load_invalid()) {
        Ok(_) => 0,
        Err(err) => if encoding_err_code(err) == 1 { 1 } else { 0 },
    };
    let invalid_lossy = bytes_to_string_lossy(load_invalid());
    let invalid_lossy_ok =
        if len(invalid_lossy) == 6 && string.starts_with(invalid_lossy, "fo") && string.ends_with(invalid_lossy, "o") {
            1
        } else {
            0
        };

    let score =
        ascii_len_ok +
        ascii_flag_ok +
        ascii_valid_ok +
        ascii_roundtrip_ok +
        ascii_lossy_ok +
        multi_len_ok +
        multi_ascii_ok +
        multi_valid_ok +
        multi_roundtrip_ok +
        multi_lossy_ok +
        empty_ok +
        invalid_valid_ok +
        invalid_decode_ok +
        invalid_lossy_ok;

    if score == 14 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run_with_setup(src, |root| {
        fs::write(
            root.join("invalid.bin"),
            [0x66_u8, 0x6f_u8, 0x80_u8, 0x6f_u8],
        )
        .expect("write invalid bytes");
    });
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_math_ops_full_surface_and_edge_cases() {
    let src = r#"
import std.io;
import std.math;

fn approx_eq(a: Float, b: Float, epsilon: Float) -> Bool {
    abs_float(a - b) <= epsilon
}

fn main() -> Int effects { io } capabilities { io  } {
    let abs_ok = if abs(-42) == 42 { 1 } else { 0 };
    let abs_float_ok = if approx_eq(abs_float(-3.5), 3.5, 0.000000001) { 1 } else { 0 };
    let min_ok = if min(7, -2) == -2 { 1 } else { 0 };
    let max_ok = if max(7, -2) == 7 { 1 } else { 0 };

    let pow_ok = if approx_eq(pow(2.0, 3.0), 8.0, 0.000000001) { 1 } else { 0 };
    let sqrt_ok = if approx_eq(sqrt(81.0), 9.0, 0.000000001) { 1 } else { 0 };

    let floor_pos_ok = if floor(3.9) == 3 { 1 } else { 0 };
    let floor_neg_ok = if floor(-3.1) == -4 { 1 } else { 0 };
    let ceil_pos_ok = if ceil(3.1) == 4 { 1 } else { 0 };
    let ceil_neg_ok = if ceil(-3.9) == -3 { 1 } else { 0 };
    let round_down_ok = if round(3.49) == 3 { 1 } else { 0 };
    let round_half_ok = if round(3.5) == 4 { 1 } else { 0 };
    let round_neg_half_ok = if round(-3.5) == -4 { 1 } else { 0 };

    let log_ok = if approx_eq(log(E), 1.0, 0.0000001) { 1 } else { 0 };
    let sin_ok = if approx_eq(sin(PI / 2.0), 1.0, 0.0000001) { 1 } else { 0 };
    let cos_ok = if approx_eq(cos(PI), -1.0, 0.0000001) { 1 } else { 0 };

    let constants_ok =
        if PI > 3.14 && PI < 3.15 && E > 2.71 && E < 2.72 {
            1
        } else {
            0
        };

    let sqrt_nan = sqrt(-1.0);
    let nan_ok = if sqrt_nan != sqrt_nan { 1 } else { 0 };

    let score =
        abs_ok +
        abs_float_ok +
        min_ok +
        max_ok +
        pow_ok +
        sqrt_ok +
        floor_pos_ok +
        floor_neg_ok +
        ceil_pos_ok +
        ceil_neg_ok +
        round_down_ok +
        round_half_ok +
        round_neg_half_ok +
        log_ok +
        sin_ok +
        cos_ok +
        constants_ok +
        nan_ok;

    if score == 18 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_math_ops_are_deterministic() {
    let src = r#"
import std.io;
import std.math;

fn main() -> Int effects { io } capabilities { io  } {
    let signal_a = round(pow(2.0, 8.0) + sqrt(81.0) + sin(PI / 2.0) * 10.0);
    let signal_b = round(pow(2.0, 8.0) + sqrt(81.0) + sin(PI / 2.0) * 10.0);
    let slope_a = round(log(E) * 10.0 + cos(PI) * 10.0);
    let slope_b = round(log(E) * 10.0 + cos(PI) * 10.0);

    if signal_a == signal_b && slope_a == slope_b {
        print_int(signal_a + slope_a);
    } else {
        print_int(0);
    };
    0
}
"#;

    let (code_a, stdout_a, stderr_a) = compile_and_run(src);
    assert_eq!(code_a, 0, "stderr={stderr_a}");
    assert_eq!(stdout_a, "275\n");

    let (code_b, stdout_b, stderr_b) = compile_and_run(src);
    assert_eq!(code_b, 0, "stderr={stderr_b}");
    assert_eq!(stdout_b, "275\n");
    assert_eq!(
        stdout_a, stdout_b,
        "math program output must be deterministic"
    );
}

#[test]
fn exec_string_http_like_request_and_header_workflow() {
    let src = r#"
import std.io;
import std.option;
import std.result;
import std.string;
import std.vec;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn opt_vec_len(v: Option[Vec[String]]) -> Int {
    match v {
        Some(value) => vec_len(value),
        None => 0,
    }
}

fn parse_or_zero(v: Result[Int, String]) -> Int {
    match v {
        Ok(value) => value,
        Err(_) => 0,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let request_line = "GET /api/users HTTP/1.1";
    let line_parts = split(request_line, " ");
    let parts_ok = if vec_len(line_parts) == 3 { 1 } else { 0 };
    let line_roundtrip_ok = if len(join(line_parts, " ")) == len(request_line) { 1 } else { 0 };
    let method_ok = if string.starts_with(request_line, "GET ") { 1 } else { 0 };
    let target_ok = if string.contains(request_line, "/api/users") { 1 } else { 0 };
    let version_ok = if string.ends_with(request_line, "HTTP/1.1") { 1 } else { 0 };

    let header = "Content-Length: 12";
    let header_parts = split_first(header, ":");
    let header_pair_ok = if opt_vec_len(header_parts) == 2 { 1 } else { 0 };
    let header_name_ok = if len(trim("  Content-Length  ")) == 14 { 1 } else { 0 };
    let parsed_length_ok = if parse_or_zero(parse_int(trim(" 12 "))) == 12 { 1 } else { 0 };

    let query = "page=12";
    let query_parts_ok = if opt_vec_len(split_first(query, "=")) == 2 { 1 } else { 0 };
    let page_idx_ok = if opt_int_or(string.index_of(query, "="), -1) == 4 { 1 } else { 0 };

    let score =
        parts_ok +
        line_roundtrip_ok +
        method_ok +
        target_ok +
        version_ok +
        header_pair_ok +
        header_name_ok +
        parsed_length_ok +
        query_parts_ok +
        page_idx_ok;

    if score == 10 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_regex_compile_match_find_replace() {
    let src = r#"
import std.io;
import std.regex;
import std.string;
import std.vec;

fn bool_result(v: Result[Bool, RegexError]) -> Int {
    match v {
        Ok(flag) => if flag { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn string_len(v: Result[String, RegexError]) -> Int {
    match v {
        Ok(text) => len(text),
        Err(_) => 0,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let re = match compile_with_flags("^error [0-9]+$", flag_case_insensitive() + flag_multiline()) {
        Ok(value) => value,
        Err(_) => Regex { pattern: "", flags: 0 },
    };

    let match_yes = bool_result(is_match(re, "ERROR 42"));
    let match_no = bool_result(is_match(re, "info 42"));
    let found_len = string_len(regex.find(re, "warn\nerror 17\nok"));
    let replaced_len = string_len(regex.replace(re, "warn\nERROR 17\nok", "<redacted>"));
    let replace_nomatch_len = string_len(regex.replace(re, "all good", "<x>"));

    if match_yes == 1
        && match_no == 0
        && found_len == 8
        && replaced_len == 18
        && replace_nomatch_len == 8
    {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_regex_reports_structured_errors() {
    let src = r#"
import std.io;
import std.regex;
import std.string;
import std.vec;

fn regex_error_code(err: RegexError) -> Int {
    match err {
        InvalidPattern => 1,
        InvalidInput => 2,
        NoMatch => 3,
        UnsupportedFeature => 4,
        TooComplex => 5,
        Internal => 6,
    }
}

fn invalid_pattern(v: Result[Regex, RegexError]) -> Int {
    match v {
        Err(err) => if regex_error_code(err) == 1 { 1 } else { 0 },
        _ => 0,
    }
}

fn unsupported_flags(v: Result[Regex, RegexError]) -> Int {
    match v {
        Err(err) => if regex_error_code(err) == 4 { 1 } else { 0 },
        _ => 0,
    }
}

fn no_match(v: Result[String, RegexError]) -> Int {
    match v {
        Err(err) => if regex_error_code(err) == 3 { 1 } else { 0 },
        _ => 0,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let bad = invalid_pattern(compile_with_flags("[unterminated", no_flags()));
    let unsupported = unsupported_flags(
        compile_with_flags("a.*b", flag_multiline() + flag_dot_matches_newline())
    );
    let re = match compile("error") {
        Ok(value) => value,
        Err(_) => Regex { pattern: "error", flags: 0 },
    };
    let miss = no_match(regex.find(re, "all good"));

    if bad + unsupported + miss == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_regex_captures_and_find_all() {
    let src = r#"
import std.io;
import std.regex;
import std.string;
import std.vec;

fn parse_or(v: Result[Int, String], fallback: Int) -> Int {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn capture_from_match(m: RegexMatch) -> Int effects { env } capabilities { env  } {
    let g1 = match vec.get(m.groups, 0) {
        Some(value) => value,
        None => "",
    };
    let g2 = match vec.get(m.groups, 1) {
        Some(value) => value,
        None => "",
    };
    if len(m.full) == 5
        && parse_or(parse_int(g1), -1) == 12
        && parse_or(parse_int(g2), -1) == 34
        && m.start == 3
        && m.end == 8
    { 1 } else { 0 }
}

fn capture_ok(v: Result[Option[RegexMatch], RegexError]) -> Int effects { env } capabilities { env  } {
    match v {
        Ok(found) => match found {
            Some(m) => capture_from_match(m),
            None => 0,
        },
        Err(_) => 0,
    }
}

fn captures_none(v: Result[Option[RegexMatch], RegexError]) -> Int {
    match v {
        Ok(found) => match found {
            None => 1,
            Some(_) => 0,
        },
        Err(_) => 0,
    }
}

fn empty_groups() -> Vec[String] effects { env } capabilities { env  } {
    let groups: Vec[String] = vec.new_vec();
    groups
}

fn all_ok_items(items: Vec[RegexMatch]) -> Int effects { env } capabilities { env  } {
    if items.len != 3 {
        0
    } else {
        let first = match vec.get(items, 0) {
            Some(value) => value,
            None => RegexMatch { full: "", groups: empty_groups(), start: 0, end: 0 },
        };
        let second = match vec.get(items, 1) {
            Some(value) => value,
            None => RegexMatch { full: "", groups: empty_groups(), start: 0, end: 0 },
        };
        let third = match vec.get(items, 2) {
            Some(value) => value,
            None => RegexMatch { full: "", groups: empty_groups(), start: 0, end: 0 },
        };
        let second_g1 = match vec.get(second.groups, 0) {
            Some(value) => value,
            None => "",
        };
        let third_g2 = match vec.get(third.groups, 1) {
            Some(value) => value,
            None => "",
        };

        if len(first.full) == 3
            && first.start == 1
            && first.end == 4
            && len(second.full) == 5
            && second.start == 6
            && second.end == 11
            && len(third.full) == 7
            && third.start == 13
            && third.end == 20
            && parse_or(parse_int(second_g1), -1) == 33
            && parse_or(parse_int(third_g2), -1) == 666
        {
            1
        } else {
            0
        }
    }
}

fn all_ok(v: Result[Vec[RegexMatch], RegexError]) -> Int effects { env } capabilities { env  } {
    match v {
        Ok(items) => all_ok_items(items),
        Err(_) => 0,
    }
}

fn all_none(v: Result[Vec[RegexMatch], RegexError]) -> Int {
    match v {
        Ok(items) => if items.len == 0 { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn main() -> Int effects { env, io } capabilities { env, io  } {
    let re = match compile("([0-9]+)-([0-9]+)") {
        Ok(value) => value,
        Err(_) => Regex { pattern: "", flags: 0 },
    };

    let first = capture_ok(captures(re, "id=12-34 status=ok"));
    let none_capture = captures_none(captures(re, "no numbers"));
    let all = all_ok(find_all(re, "a1-2 b33-44 c555-666"));
    let none_all = all_none(find_all(re, "no numbers"));

    if first + none_capture + all + none_all == 4 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_fs_roundtrip_and_metadata() {
    let src = r#"
import std.io;
import std.fs;

fn unwrap_bool(v: Result[Bool, FsError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, fs, env } capabilities { io, fs, env  } {
    let wrote = unwrap_bool(write_text("a.txt", "ab"));
    let appended = unwrap_bool(append_text("a.txt", "cd"));
    let copied = unwrap_bool(copy("a.txt", "b.txt"));
    let moved = unwrap_bool(move("b.txt", "c.txt"));
    let size = match metadata("a.txt") {
        Ok(m) => m.size,
        Err(_) => 0,
    };
    let c_exists = if exists("c.txt") { 1 } else { 0 };
    let deleted_a = unwrap_bool(delete("a.txt"));
    let deleted_c = unwrap_bool(delete("c.txt"));
    let score = wrote + appended + copied + moved + deleted_a + deleted_c + size + c_exists;
    print_int(score);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "11\n");
}

#[test]
fn exec_fs_walk_and_temp_utilities() {
    let src = r#"
import std.io;
import std.fs;
import std.vec;

fn main() -> Int effects { io, fs, env } capabilities { io, fs, env  } {
    let tmp_file = match fs.temp_file("aic_io_test_") {
        Ok(path) => path,
        Err(_) => "",
    };
    let tmp_dir = match fs.temp_dir("aic_io_test_") {
        Ok(path) => path,
        Err(_) => "",
    };
    let is_file = match metadata(tmp_file) {
        Ok(m) => if m.is_file { 1 } else { 0 },
        Err(_) => 0,
    };
    let dir_exists = if exists(tmp_dir) { 1 } else { 0 };
    let walked = match walk_dir(".") {
        Ok(entries) => if vec_len(entries) >= 0 { 1 } else { 0 },
        Err(_) => 0,
    };
    delete(tmp_file);
    delete(tmp_dir);
    print_int(is_file + dir_exists + walked);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "3\n");
}

#[test]
fn exec_fs_not_found_and_invalid_input_errors_are_stable() {
    let src = r#"
import std.io;
import std.fs;

fn err_code(err: FsError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        AlreadyExists => 3,
        InvalidInput => 4,
        Io => 5,
    }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let missing = match read_text("missing-aicore-file.txt") {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let invalid = match read_text("") {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    print_int(missing * 10 + invalid);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "14\n");
}

#[cfg(unix)]
#[test]
fn exec_fs_permission_error_maps_to_permission_denied() {
    use std::os::unix::fs::PermissionsExt;

    let src = r#"
import std.io;
import std.fs;

fn err_code(err: FsError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        AlreadyExists => 3,
        InvalidInput => 4,
        Io => 5,
    }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let result = read_text("secret.txt");
    let code = match result {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    print_int(code);
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run_with_setup(src, |root| {
        let path = root.join("secret.txt");
        fs::write(&path, "locked").expect("write secret file");
        let mut perms = fs::metadata(&path).expect("metadata").permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&path, perms).expect("chmod 000");
    });
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "2\n");
}

#[test]
fn exec_fs_bytes_roundtrip() {
    let src = r#"
import std.io;
import std.fs;
import std.string;
import std.bytes;

fn ok_bool(v: Result[Bool, FsError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let wrote = ok_bool(write_bytes("bytes.bin", bytes.from_string("abc")));
    let appended = ok_bool(append_bytes("bytes.bin", bytes.from_string("XYZ")));
    let payload = match read_bytes("bytes.bin") {
        Ok(value) => value,
        Err(_) => bytes.empty(),
    };
    let payload_text = bytes.to_string_lossy(payload);
    let payload_ok = if len(payload_text) == 6 && string.starts_with(payload_text, "ab") && string.ends_with(payload_text, "XYZ") {
        1
    } else {
        0
    };
    let removed = ok_bool(delete("bytes.bin"));
    if wrote + appended + payload_ok + removed == 4 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_fs_file_read_line_and_close() {
    let src = r#"
import std.io;
import std.fs;
import std.string;

fn ok_bool(v: Result[Bool, FsError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn unwrap_handle(v: Result[FileHandle, FsError]) -> FileHandle {
    match v {
        Ok(handle) => handle,
        Err(_) => FileHandle { handle: 0 },
    }
}

fn starts(v: Result[Option[String], FsError], prefix: String) -> Int {
    match v {
        Ok(value) => match value {
            Some(line) => if string.starts_with(line, prefix) { 1 } else { 0 },
            None => 0,
        },
        Err(_) => 0,
    }
}

fn eof(v: Result[Option[String], FsError]) -> Int {
    match v {
        Ok(value) => match value {
            None => 1,
            Some(_) => 0,
        },
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let writer = unwrap_handle(open_write("lines.txt"));
    let writer_ok = if writer.handle > 0 { 1 } else { 0 };
    let wrote = ok_bool(file_write_str(writer, "alpha\nbeta\ngamma"));
    let closed_writer = ok_bool(file_close(writer));

    let reader = unwrap_handle(open_read("lines.txt"));
    let reader_ok = if reader.handle > 0 { 1 } else { 0 };
    let first = starts(file_read_line(reader), "alpha");
    let second = starts(file_read_line(reader), "beta");
    let third = starts(file_read_line(reader), "gamma");
    let done = eof(file_read_line(reader));
    let closed_reader = ok_bool(file_close(reader));
    let removed = ok_bool(delete("lines.txt"));

    let score = writer_ok + reader_ok + wrote + closed_writer + first + second + third + done + closed_reader + removed;
    if score == 10 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_raii_file_handle_cleanup_on_scope_exit_and_early_return() {
    let src = r#"
import std.io;
import std.fs;

fn unwrap_handle(v: Result[FileHandle, FsError]) -> FileHandle {
    match v {
        Ok(handle) => handle,
        Err(_) => FileHandle { handle: 0 },
    }
}

fn ok_bool(v: Result[Bool, FsError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn scope_cycle(path: String) -> Int effects { fs } capabilities { fs  } {
    let file = unwrap_handle(open_append(path));
    if file.handle == 0 {
        0
    } else {
        match file_write_str(file, "x") {
            Ok(_) => 1,
            Err(_) => 0,
        }
    }
}

fn early_cycle(path: String) -> Int effects { fs } capabilities { fs  } {
    let file = unwrap_handle(open_append(path));
    if file.handle == 0 {
        0
    } else {
        1
    }
}

fn run_scope(path: String, iterations: Int) -> Int effects { fs } capabilities { fs  } {
    let mut i = 0;
    let mut ok = 0;
    while i < iterations {
        ok = ok + scope_cycle(path);
        i = i + 1;
    };
    ok
}

fn run_early(path: String, iterations: Int) -> Int effects { fs } capabilities { fs  } {
    let mut i = 0;
    let mut ok = 0;
    while i < iterations {
        ok = ok + early_cycle(path);
        i = i + 1;
    };
    ok
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let scope_path = "raii_scope_handles.txt";
    let early_path = "raii_early_handles.txt";

    let scope_ok = run_scope(scope_path, 1100);
    let early_ok = run_early(early_path, 1100);

    let removed_scope = ok_bool(delete(scope_path));
    let removed_early = ok_bool(delete(early_path));

    if scope_ok == 1100 && early_ok == 1100 && removed_scope + removed_early == 2 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_raii_file_handle_cleanup_on_question_mark_error_return() {
    let src = r#"
import std.io;
import std.fs;

fn ok_bool(v: Result[Bool, FsError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn append_then_fail(path: String) -> Result[Int, FsError] effects { fs } capabilities { fs  } {
    let file = open_append(path)?;
    file_write_str(file, "x")?;
    read_text("")?;
    Ok(1)
}

fn error_cycle(path: String) -> Int effects { fs } capabilities { fs  } {
    match append_then_fail(path) {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let path = "raii_try_handles.txt";
    let iterations = 1100;
    let mut i = 0;
    let mut err_count = 0;
    while i < iterations {
        err_count = err_count + error_cycle(path);
        i = i + 1;
    };

    let size = match metadata(path) {
        Ok(m) => m.size,
        Err(_) => 0,
    };
    let removed = ok_bool(delete(path));

    if err_count == iterations && size == iterations && removed == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_raii_file_handle_move_out_preserves_transferred_ownership() {
    let src = r#"
import std.io;
import std.fs;
import std.string;

fn unwrap_handle(v: Result[FileHandle, FsError]) -> FileHandle {
    match v {
        Ok(handle) => handle,
        Err(_) => FileHandle { handle: 0 },
    }
}

fn ok_bool(v: Result[Bool, FsError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn open_via_return(path: String) -> FileHandle effects { fs } capabilities { fs  } {
    let file = unwrap_handle(open_append(path));
    return file;
    FileHandle { handle: 0 }
}

fn open_via_tail(path: String) -> FileHandle effects { fs } capabilities { fs  } {
    let file = unwrap_handle(open_append(path));
    file
}

fn append_via_let_move(path: String, line: String) -> Int effects { fs } capabilities { fs  } {
    let file = unwrap_handle(open_append(path));
    let moved = file;
    if moved.handle == 0 {
        0
    } else {
        match file_write_str(moved, line) {
            Ok(_) => 1,
            Err(_) => 0,
        }
    }
}

fn append_via_return_move(path: String, line: String) -> Int effects { fs } capabilities { fs  } {
    let file = open_via_return(path);
    if file.handle == 0 {
        0
    } else {
        match file_write_str(file, line) {
            Ok(_) => 1,
            Err(_) => 0,
        }
    }
}

fn append_via_tail_move(path: String, line: String) -> Int effects { fs } capabilities { fs  } {
    let file = open_via_tail(path);
    if file.handle == 0 {
        0
    } else {
        match file_write_str(file, line) {
            Ok(_) => 1,
            Err(_) => 0,
        }
    }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let path = "raii_move_transfer.txt";
    let wrote_let = append_via_let_move(path, "alpha\n");
    let wrote_return = append_via_return_move(path, "beta\n");
    let wrote_tail = append_via_tail_move(path, "gamma\n");

    let content_ok = match read_text(path) {
        Ok(text) =>
            if string.starts_with(text, "alpha")
                && string.contains(text, "beta")
                && string.contains(text, "gamma") {
                1
            } else {
                0
            },
        Err(_) => 0,
    };
    let removed = ok_bool(delete(path));

    if wrote_let + wrote_return + wrote_tail + content_ok + removed == 5 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_drop_trait_dispatch_lifo_question_mark_and_move_paths() {
    let src = r#"
import std.io;
import std.fs;
import std.string;

trait Drop[T] {
    fn drop(self) -> () effects { fs };
}

struct AuditDrop {
    path: String,
    marker: String,
    id: Int,
}

impl Drop[AuditDrop] {
    fn drop(self) -> () effects { fs } capabilities { fs  } {
        let _ignored = append_text(self.path, self.marker);
        ()
    }
}

fn ok_bool(v: Result[Bool, FsError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn drop_scope(path: String) -> Int effects { fs } capabilities { fs  } {
    let first = AuditDrop { path: path, marker: "A", id: 1 };
    let second = AuditDrop { path: path, marker: "B", id: 2 };
    if first.id + second.id == 3 { 1 } else { 0 }
}

fn drop_try(path: String) -> Int effects { fs } capabilities { fs  } {
    match drop_fail(path) {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

fn drop_fail(path: String) -> Result[Int, FsError] effects { fs } capabilities { fs  } {
    let probe = AuditDrop { path: path, marker: "C", id: 3 };
    read_text("")?;
    Ok(probe.id)
}

fn make_probe(path: String) -> AuditDrop {
    let probe = AuditDrop { path: path, marker: "D", id: 4 };
    probe
}

fn drop_move(path: String) -> Int effects { fs } capabilities { fs  } {
    let moved = make_probe(path);
    if moved.id == 4 { 1 } else { 0 }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let path = "drop_trait_runtime.txt";
    let reset = ok_bool(write_text(path, ""));
    let scope_ok = drop_scope(path);
    let try_ok = drop_try(path);
    let move_ok = drop_move(path);

    let text = match read_text(path) {
        Ok(value) => value,
        Err(_) => "",
    };
    let shape_ok =
        if string.starts_with(text, "BA")
            && string.ends_with(text, "CD")
            && len(text) == 4 {
            1
        } else {
            0
        };
    let removed = ok_bool(delete(path));

    if reset + scope_ok + try_ok + move_ok + shape_ok + removed == 6 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_fs_mkdir_all_nested_directories() {
    let src = r#"
import std.io;
import std.fs;
import std.path;

fn ok_bool(v: Result[Bool, FsError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let root = "tree_root";
    let level1 = path.join(root, "a");
    let level2 = path.join(level1, "b");

    let made = ok_bool(mkdir_all(level2));
    let root_ok = match metadata(root) {
        Ok(m) => if m.is_dir { 1 } else { 0 },
        Err(_) => 0,
    };
    let leaf_ok = match metadata(level2) {
        Ok(m) => if m.is_dir { 1 } else { 0 },
        Err(_) => 0,
    };

    let rm2 = ok_bool(rmdir(level2));
    let rm1 = ok_bool(rmdir(level1));
    let rm0 = ok_bool(rmdir(root));

    if made + root_ok + leaf_ok + rm2 + rm1 + rm0 == 6 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_fs_list_dir_immediate_children_only() {
    let src = r#"
import std.io;
import std.fs;
import std.path;
import std.string;
import std.vec;

fn ok_bool(v: Result[Bool, FsError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let root = "list_root";
    let nested = path.join(root, "nested");
    let top_file = path.join(root, "top.txt");
    let inner_file = path.join(nested, "inner.txt");

    let mk_root = ok_bool(mkdir(root));
    let mk_nested = ok_bool(mkdir(nested));
    let wrote_top = ok_bool(write_text(top_file, "top"));
    let wrote_inner = ok_bool(write_text(inner_file, "inner"));

    let count_ok = match list_dir(root) {
        Ok(entries) => if vec_len(entries) == 2 { 1 } else { 0 },
        Err(_) => 0,
    };
    let names_ok = match list_dir(root) {
        Ok(entries) => if string.contains(string.join(entries, "|"), "top.txt") &&
            string.contains(string.join(entries, "|"), "nested") {
            if string.contains(string.join(entries, "|"), "inner.txt") { 0 } else { 1 }
        } else {
            0
        },
        Err(_) => 0,
    };
    let list_score = count_ok + names_ok;

    let removed_inner = ok_bool(delete(inner_file));
    let removed_top = ok_bool(delete(top_file));
    let rm_nested = ok_bool(rmdir(nested));
    let rm_root = ok_bool(rmdir(root));

    let score =
        mk_root + mk_nested + wrote_top + wrote_inner + list_score +
        removed_inner + removed_top + rm_nested + rm_root;
    if score == 10 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_env_and_path_apis_roundtrip() {
    let src = r#"
import std.io;
import std.env;
import std.path;
import std.string;

fn ok_bool(v: Result[Bool, EnvError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, env, fs } capabilities { io, env, fs  } {
    let original = match env.cwd() {
        Ok(path) => path,
        Err(_) => "",
    };
    let set_ok = ok_bool(env.set("AIC_EXEC_ENV_KEY", "value-xyz"));
    let got_len = match env.get("AIC_EXEC_ENV_KEY") {
        Ok(value) => len(value),
        Err(_) => 0,
    };
    let rm_ok = ok_bool(env.remove("AIC_EXEC_ENV_KEY"));
    let missing_ok = match env.get("AIC_EXEC_ENV_KEY") {
        Ok(_) => 0,
        Err(err) => match err {
            NotFound => 1,
            _ => 0,
        },
    };
    let cwd_set_ok = ok_bool(env.set_cwd("."));
    let now = match env.cwd() {
        Ok(path) => path,
        Err(_) => "",
    };
    let joined = path.join(now, "alpha.txt");
    let base_len = len(path.basename(joined));
    let dir_len = len(path.dirname(joined));
    let ext_len = len(path.extension(joined));
    let abs_ok = if path.is_abs(now) { 1 } else { 0 };
    let restore_ok = ok_bool(env.set_cwd(original));

    let score = set_ok + rm_ok + missing_ok + cwd_set_ok + abs_ok + restore_ok;
    if score == 6 && got_len == 9 && base_len == 9 && dir_len > 0 && ext_len == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_env_args_and_arg_at_roundtrip() {
    let src = r#"
import std.io;
import std.env;
import std.vec;
import std.string;

fn main() -> Int effects { io, env } capabilities { io, env  } {
    let values = args();
    let count = arg_count();
    let same_count = if vec_len(values) == count { 1 } else { 0 };
    let first_ok = match arg_at(0) {
        Some(v) => if len(v) > 0 { 1 } else { 0 },
        None => 0,
    };
    let second_ok = match arg_at(1) {
        Some(v) => if len(v) == 5 && string.starts_with(v, "alpha") { 1 } else { 0 },
        None => 0,
    };
    let third_ok = match arg_at(2) {
        Some(v) => if len(v) == 4 && string.starts_with(v, "beta") { 1 } else { 0 },
        None => 0,
    };
    let missing_ok = match arg_at(999) {
        Some(_) => 0,
        None => 1,
    };

    if same_count + first_ok + second_ok + third_ok + missing_ok == 5 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run_with_args(src, &["alpha", "beta"]);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_env_listing_and_platform_helpers_are_stable() {
    let src = r#"
import std.io;
import std.env;
import std.vec;
import std.string;

fn home_ok(v: Result[String, EnvError]) -> Int {
    match v {
        Ok(path) => if len(path) > 0 { 1 } else { 0 },
        Err(err) => match err {
            NotFound => 1,
            _ => 0,
        },
    }
}

fn temp_ok(v: Result[String, EnvError]) -> Int {
    match v {
        Ok(path) => if len(path) > 0 { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, env, fs } capabilities { io, env, fs  } {
    let vars_ok = if vec_len(all_vars()) > 0 { 1 } else { 0 };
    let os = os_name();
    let os_linux = if len(os) == 5 && string.starts_with(os, "linux") { 1 } else { 0 };
    let os_macos = if len(os) == 5 && string.starts_with(os, "macos") { 1 } else { 0 };
    let os_windows = if len(os) == 7 && string.starts_with(os, "windows") { 1 } else { 0 };
    let os_ok = if os_linux + os_macos + os_windows == 1 { 1 } else { 0 };
    let arch_ok = if len(arch()) > 0 { 1 } else { 0 };

    let score = vars_ok + home_ok(env.home_dir()) + temp_ok(env.temp_dir()) + os_ok + arch_ok;
    if score == 5 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_env_exit_sets_process_exit_code() {
    let src_ok = r#"
import std.io;
import std.env;

fn main() -> Int effects { io, env } capabilities { io, env  } {
    print_int(1);
    exit(0);
    print_int(2);
    0
}
"#;
    let (code_ok, stdout_ok, stderr_ok) = compile_and_run(src_ok);
    assert_eq!(code_ok, 0, "stderr={stderr_ok}");
    assert_eq!(stdout_ok, "1\n");

    let src_err = r#"
import std.io;
import std.env;

fn main() -> Int effects { io, env } capabilities { io, env  } {
    print_int(1);
    exit(1);
    print_int(2);
    0
}
"#;
    let (code_err, stdout_err, stderr_err) = compile_and_run(src_err);
    assert_eq!(code_err, 1, "stderr={stderr_err}");
    assert_eq!(stdout_err, "1\n");
}

#[test]
fn exec_map_string_string_ops_are_deterministic() {
    let src = r#"
import std.io;
import std.map;
import std.option;
import std.string;
import std.vec;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(x) => x,
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let m0: Map[String, String] = map.new_map();
    let m1 = map.insert(m0, "content-type", "application/json");
    let m2 = map.insert(m1, "accept", "*/*");
    let m3 = map.insert(m2, "x-id", "42");
    let m4 = map.insert(m3, "accept", "text/plain");
    let m5 = map.remove(m4, "content-type");

    let has_accept = if map.contains_key(m5, "accept") { 1 } else { 0 };
    let missing_ok = if map.contains_key(m5, "missing") { 0 } else { 1 };
    let accept_len = match map.get(m5, "accept") {
        Some(v) => len(v),
        None => 0,
    };
    let removed_ok = match map.get(m5, "content-type") {
        Some(_) => 0,
        None => 1,
    };
    let size_ok = if map.size(m5) == 2 { 1 } else { 0 };
    let keys_join = string.join(map.keys(m5), ",");
    let keys_order_ok = if opt_int_or(string.index_of(keys_join, "accept,x-id"), -1) == 0 {
        1
    } else {
        0
    };
    let values_join = string.join(map.values(m5), ",");
    let values_order_ok = if opt_int_or(string.index_of(values_join, "text/plain,42"), -1) == 0 {
        1
    } else {
        0
    };
    let entries_len_ok = if vec_len(map.entries(m5)) == 2 { 1 } else { 0 };

    let score =
        has_accept +
        missing_ok +
        removed_ok +
        size_ok +
        keys_order_ok +
        values_order_ok +
        entries_len_ok;
    if score == 7 && accept_len == 10 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_map_string_sso_boundary_behavior_matches_with_and_without_sso() {
    let src = r#"
import std.io;
import std.map;
import std.string;

fn main() -> Int effects { io, env } capabilities { io, env } {
    let short_key = "sssssssssssssssssssssss";
    let long_key = "llllllllllllllllllllllll";
    let short_value = "vvvvvvvvvvvvvvvvvvvvvvv";
    let long_value = "wwwwwwwwwwwwwwwwwwwwwwww";

    let mut m: Map[String, String] = map.new_map();
    m = map.insert(m, short_key, short_value);
    m = map.insert(m, long_key, long_value);

    let mut i = 0;
    while i < 200 {
        m = map.insert(m, short_key, short_value);
        m = map.insert(m, long_key, long_value);
        i = i + 1;
    };

    let short_len = match map.get(m, short_key) {
        Some(v) => len(v),
        None => 0,
    };
    let long_len = match map.get(m, long_key) {
        Some(v) => len(v),
        None => 0,
    };
    let removed = map.remove(m, short_key);
    let removed_ok = if map.contains_key(removed, short_key) { 0 } else { 1 };
    let long_ok = if map.contains_key(removed, long_key) { 1 } else { 0 };
    let size_ok = if map.size(removed) == 1 { 1 } else { 0 };

    if short_len == 23 && long_len == 24 && removed_ok + long_ok + size_ok == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    let (code_on, stdout_on, stderr_on) = compile_and_run(src);
    assert_eq!(code_on, 0, "stderr={stderr_on}");
    assert_eq!(stdout_on, "42\n");

    let (code_off, stdout_off, stderr_off) = compile_and_run_with_setup_and_args_and_input_and_env(
        src,
        &[],
        "",
        &[("AIC_RT_DISABLE_MAP_SSO", "1")],
        |_| {},
    );
    assert_eq!(code_off, 0, "stderr={stderr_off}");
    assert_eq!(stdout_off, "42\n");
}

#[test]
fn exec_map_string_int_ops_are_deterministic() {
    let src = r#"
import std.io;
import std.map;
import std.option;
import std.string;
import std.vec;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(x) => x,
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let m0: Map[String, Int] = map.new_map();
    let m1 = map.insert(m0, "b", 2);
    let m2 = map.insert(m1, "a", 1);
    let m3 = map.insert(m2, "c", 3);
    let m4 = map.insert(m3, "b", 20);
    let m5 = map.remove(m4, "a");

    let b_value = opt_int_or(map.get(m5, "b"), 0);
    let a_missing_ok = match map.get(m5, "a") {
        Some(_) => 0,
        None => 1,
    };
    let size_ok = if map.size(m5) == 2 { 1 } else { 0 };
    let has_c = if map.contains_key(m5, "c") { 1 } else { 0 };
    let keys_join = string.join(map.keys(m5), ",");
    let keys_order_ok = if opt_int_or(string.index_of(keys_join, "b,c"), -1) == 0 { 1 } else { 0 };
    let values_len_ok = if vec_len(map.values(m5)) == 2 { 1 } else { 0 };
    let entries_len_ok = if vec_len(map.entries(m5)) == 2 { 1 } else { 0 };

    let score = a_missing_ok + size_ok + has_c + keys_order_ok + values_len_ok + entries_len_ok;
    if score == 6 && b_value == 20 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_map_close_api_is_stable() {
    let src = r#"
import std.io;
import std.map;

fn main() -> Int effects { io } capabilities { io  } {
    let m0: Map[Int, Int] = map.new_map();
    let m1 = map.insert(m0, 7, 11);
    map.close_map(m1);
    print_int(42);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_map_int_string_key_ops_are_deterministic() {
    let src = r#"
import std.io;
import std.map;
import std.vec;
import std.option;
import std.string;

fn opt_text_or(v: Option[String], fallback: String) -> String {
    match v {
        Some(text) => text,
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let m0: Map[Int, String] = map.new_map();
    let m1 = map.insert(m0, 20, "beta");
    let m2 = map.insert(m1, 10, "alpha");
    let m3 = map.insert(m2, 30, "gamma");
    let m4 = map.insert(m3, 20, "beta2");
    let m5 = map.remove(m4, 10);

    let key_vec = map.keys(m5);
    let k0 = match vec.get(key_vec, 0) { Some(v) => v, None => -1 };
    let k1 = match vec.get(key_vec, 1) { Some(v) => v, None => -1 };
    let v20 = opt_text_or(map.get(m5, 20), "");
    let has_30 = if map.contains_key(m5, 30) { 1 } else { 0 };
    let missing_10 = if map.contains_key(m5, 10) { 0 } else { 1 };
    let size_ok = if map.size(m5) == 2 { 1 } else { 0 };
    let values_ok = if vec_len(map.values(m5)) == 2 { 1 } else { 0 };

    if k0 == 20 && k1 == 30 && len(v20) == 5 && has_30 + missing_10 + size_ok + values_ok == 4 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    assert_program_prints_42(src);
}

#[test]
fn exec_map_bool_int_key_ops_are_deterministic() {
    let src = r#"
import std.io;
import std.map;
import std.vec;
import std.option;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let m0: Map[Bool, Int] = map.new_map();
    let m1 = map.insert(m0, true, 7);
    let m2 = map.insert(m1, false, 9);
    let m3 = map.insert(m2, true, 70);

    let keys = map.keys(m3);
    let k0 = match vec.get(keys, 0) { Some(v) => if v { 1 } else { 2 }, None => 0 };
    let k1 = match vec.get(keys, 1) { Some(v) => if v { 1 } else { 2 }, None => 0 };

    let v_true = opt_int_or(map.get(m3, true), 0);
    let v_false = opt_int_or(map.get(m3, false), 0);
    let has_true = if map.contains_key(m3, true) { 1 } else { 0 };
    let removed = map.remove(m3, false);
    let removed_ok = if map.contains_key(removed, false) { 0 } else { 1 };
    let size_ok = if map.size(removed) == 1 { 1 } else { 0 };

    if k0 == 2 && k1 == 1 && v_true == 70 && v_false == 9 && has_true + removed_ok + size_ok == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    assert_program_prints_42(src);
}

#[test]
fn exec_set_ops_are_deterministic() {
    let src = r#"
import std.io;
import std.option;
import std.set;
import std.string;

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let s0: Set[String] = set.new_set();
    let s1 = set.add(s0, "pear");
    let s2 = set.add(s1, "apple");
    let s3 = set.add(s2, "banana");
    let s4 = set.add(s3, "banana");

    let t0: Set[String] = set.new_set();
    let t1 = set.add(t0, "banana");
    let t2 = set.add(t1, "date");

    let unioned = set.union(s4, t2);
    let crossed = set.intersection(s4, t2);
    let only_left = set.difference(s4, t2);

    let base_size_ok = if set.set_size(s4) == 3 { 1 } else { 0 };
    let base_join = string.join(set.to_vec(s4), ",");
    let base_order_ok = if opt_int_or(string.index_of(base_join, "apple,banana,pear"), -1) == 0 { 1 } else { 0 };
    let union_join = string.join(set.to_vec(unioned), ",");
    let union_ok = if opt_int_or(string.index_of(union_join, "apple,banana,date,pear"), -1) == 0 { 1 } else { 0 };
    let intersection_join = string.join(set.to_vec(crossed), ",");
    let intersection_ok = if opt_int_or(string.index_of(intersection_join, "banana"), -1) == 0 { 1 } else { 0 };
    let diff_join = string.join(set.to_vec(only_left), ",");
    let diff_ok = if opt_int_or(string.index_of(diff_join, "apple,pear"), -1) == 0 { 1 } else { 0 };
    let removed = set.discard(unioned, "date");
    let remove_ok = if set.has(removed, "date") { 0 } else { 1 };

    if base_size_ok + base_order_ok + union_ok + intersection_ok + diff_ok + remove_ok == 6 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_set_int_ops_are_deterministic() {
    let src = r#"
import std.io;
import std.set;

fn main() -> Int effects { io } capabilities { io  } {
    let s0: Set[Int] = set.new_set();
    let s1 = set.add(s0, 3);
    let s2 = set.add(s1, 1);
    let s3 = set.add(s2, 2);
    let s4 = set.add(s3, 2);

    let t0: Set[Int] = set.new_set();
    let t1 = set.add(t0, 2);
    let t2 = set.add(t1, 4);

    let unioned = set.union(s4, t2);
    let crossed = set.intersection(s4, t2);
    let only_left = set.difference(s4, t2);
    let removed = set.discard(unioned, 4);

    let score = if set.set_size(s4) == 3 { 1 } else { 0 }
        + if set.has(crossed, 2) { 1 } else { 0 }
        + if set.has(only_left, 1) { 1 } else { 0 }
        + if set.has(only_left, 3) { 1 } else { 0 }
        + if set.has(removed, 4) { 0 } else { 1 };

    if score == 5 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    assert_program_prints_42(src);
}

#[test]
fn exec_set_bool_ops_are_deterministic() {
    let src = r#"
import std.io;
import std.set;

fn main() -> Int effects { io } capabilities { io  } {
    let s0: Set[Bool] = set.new_set();
    let s1 = set.add(s0, true);
    let s2 = set.add(s1, false);
    let s3 = set.add(s2, false);

    let t0: Set[Bool] = set.new_set();
    let t1 = set.add(t0, false);

    let unioned = set.union(s3, t1);
    let crossed = set.intersection(s3, t1);
    let only_left = set.difference(s3, t1);
    let removed = set.discard(unioned, true);

    let score = if set.set_size(s3) == 2 { 1 } else { 0 }
        + if set.has(crossed, false) { 1 } else { 0 }
        + if set.has(only_left, true) { 1 } else { 0 }
        + if set.has(only_left, false) { 0 } else { 1 }
        + if set.has(removed, true) { 0 } else { 1 };

    if score == 5 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    assert_program_prints_42(src);
}

#[test]
fn exec_set_float_keys_report_explicit_unsupported_key_diagnostic() {
    let src = r#"
import std.io;
import std.set;

fn main() -> Int effects { io } capabilities { io  } {
    let s0: Set[Float] = set.new_set();
    let s1 = set.add(s0, 1.5);
    if set.has(s1, 1.5) {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    assert_unsupported_map_key_diagnostic(src);
}

#[test]
fn exec_deque_queue_workloads_are_deterministic() {
    let src = r#"
import std.deque;
import std.io;

fn int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn bfs_queue_score() -> Int {
    let mut q: Queue[Int] = new_queue();
    q = enqueue(q, 1);
    q = enqueue(q, 2);
    q = enqueue(q, 3);

    let (a_opt, q1) = dequeue(q);
    let (b_opt, q2) = dequeue(q1);
    let (c_opt, q3) = dequeue(q2);
    let (d_opt, q4) = dequeue(q3);

    let a = int_or(a_opt, -1);
    let b = int_or(b_opt, -1);
    let c = int_or(c_opt, -1);
    let d = int_or(d_opt, -1);

    if a == 1 && b == 2 && c == 3 && d == -1 && queue_len(q4) == 0 {
        1
    } else {
        0
    }
}

fn sliding_window_score() -> Int {
    let mut d: Deque[Int] = new_deque();
    d = push_back(d, 10);
    d = push_back(d, 20);
    d = push_back(d, 30);

    let (_drop_old, d1) = pop_front(d);
    let d2 = push_back(d1, 40);

    let (a_opt, d3) = pop_front(d2);
    let (b_opt, d4) = pop_front(d3);
    let (c_opt, d5) = pop_front(d4);

    let a = int_or(a_opt, -1);
    let b = int_or(b_opt, -1);
    let c = int_or(c_opt, -1);

    if a == 20 && b == 30 && c == 40 && deque_len(d5) == 0 {
        1
    } else {
        0
    }
}

fn round_robin_score() -> Int {
    let mut d: Deque[Int] = new_deque();
    d = push_back(d, 1);
    d = push_back(d, 2);
    d = push_back(d, 3);

    let (first_opt, d1) = pop_front(d);
    let d2 = push_back(d1, int_or(first_opt, 0));

    let (next_opt, d3) = pop_front(d2);
    let (tail_opt, d4) = pop_back(d3);
    let (last_opt, d5) = pop_front(d4);

    let next = int_or(next_opt, -1);
    let tail = int_or(tail_opt, -1);
    let last = int_or(last_opt, -1);

    if next == 2 && tail == 1 && last == 3 && deque_len(d5) == 0 {
        1
    } else {
        0
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let score = bfs_queue_score() + sliding_window_score() + round_robin_score();
    if score == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_structured_logging_json_mode_filters_by_level() {
    let src = r#"
import std.log;

fn main() -> Int effects { io } capabilities { io  } {
    set_json_output(true);
    set_level(Info());
    debug("hidden debug");
    info("server started");
    warn("high latency");
    error("boom");
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("\"level\":\"info\""), "stderr={stderr}");
    assert!(
        stderr.contains("\"msg\":\"server started\""),
        "stderr={stderr}"
    );
    assert!(stderr.contains("\"level\":\"warn\""), "stderr={stderr}");
    assert!(
        stderr.contains("\"msg\":\"high latency\""),
        "stderr={stderr}"
    );
    assert!(stderr.contains("\"level\":\"error\""), "stderr={stderr}");
    assert!(stderr.contains("\"msg\":\"boom\""), "stderr={stderr}");
    assert!(stderr.contains("\"ts\":\""), "stderr={stderr}");
    assert!(stderr.contains("\"trace_id\":\""), "stderr={stderr}");
    assert!(!stderr.contains("hidden debug"), "stderr={stderr}");
}

#[test]
fn exec_vec_algorithms_are_stable_and_deterministic() {
    let src = r#"
import std.io;
import std.option;
import std.vec;

struct Row {
    key: Int,
    tag: Int,
}

fn opt_int_or(v: Option[Int], fallback: Int) -> Int {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn less_int(a: Int, b: Int) -> Bool {
    a < b
}

fn less_row(a: Row, b: Row) -> Bool {
    a.key < b.key
}

fn row_tag(row: Row) -> Int {
    row.tag
}

fn is_even(x: Int) -> Bool {
    x % 2 == 0
}

fn positive(x: Int) -> Bool {
    x > 0
}

fn over_two(x: Int) -> Bool {
    x > 2
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let mut nums: Vec[Int] = vec.new_vec();
    nums = vec.push(nums, 3);
    nums = vec.push(nums, 1);
    nums = vec.push(nums, 2);
    nums = vec.push(nums, 2);

    let sorted = vec.sort(nums, less_int);
    let sorted_ok =
        (if opt_int_or(vec.get(sorted, 0), -1) == 1 { 1 } else { 0 }) +
        (if opt_int_or(vec.get(sorted, 1), -1) == 2 { 1 } else { 0 }) +
        (if opt_int_or(vec.get(sorted, 2), -1) == 2 { 1 } else { 0 }) +
        (if opt_int_or(vec.get(sorted, 3), -1) == 3 { 1 } else { 0 });

    let mut rows: Vec[Row] = vec.new_vec();
    rows = vec.push(rows, Row { key: 2, tag: 21 });
    rows = vec.push(rows, Row { key: 1, tag: 11 });
    rows = vec.push(rows, Row { key: 2, tag: 22 });
    rows = vec.push(rows, Row { key: 1, tag: 12 });
    let sorted_rows = vec.sort(rows, less_row);
    let tags = vec.map_vec(sorted_rows, row_tag);
    let stable_ok =
        (if opt_int_or(vec.get(tags, 0), -1) == 11 { 1 } else { 0 }) +
        (if opt_int_or(vec.get(tags, 1), -1) == 12 { 1 } else { 0 }) +
        (if opt_int_or(vec.get(tags, 2), -1) == 21 { 1 } else { 0 }) +
        (if opt_int_or(vec.get(tags, 3), -1) == 22 { 1 } else { 0 });

    let find_ok = if opt_int_or(vec.find(sorted, over_two), 0) == 3 { 1 } else { 0 };
    let any_ok = if vec.any(sorted, is_even) { 1 } else { 0 };
    let all_ok = if vec.all(sorted, positive) { 1 } else { 0 };
    let count_ok = if vec.count(sorted, is_even) == 2 { 1 } else { 0 };

    let zipped = vec.zip(sorted, vec.append(vec.vec_of(9), vec.vec_of(8)));
    let zip_ok = match vec.get(zipped, 1) {
        Some(pair) => if pair.left == 2 && pair.right == 8 && vec.vec_len(zipped) == 2 { 1 } else { 0 },
        None => 0,
    };

    let numbers = vec.append(vec.vec_of(40), vec.vec_of(50));
    let indexed = vec.enumerate(numbers);
    let enumerate_ok = match vec.get(indexed, 1) {
        Some(item) => if item.index == 1 && item.value == 50 { 1 } else { 0 },
        None => 0,
    };

    if sorted_ok + stable_ok + find_ok + any_ok + all_ok + count_ok + zip_ok + enumerate_ok == 14 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_time_helpers_are_predictable() {
    let src = r#"
import std.io;
import std.time;

fn main() -> Int effects { io, time } capabilities { io, time  } {
    let wall = now_ms();
    let start = monotonic_ms();
    let deadline = deadline_after_ms(25);
    sleep_ms(35);
    let after = monotonic_ms();
    let remain = remaining_ms(deadline);
    let expired = if timeout_expired(deadline) { 1 } else { 0 };
    sleep_until(deadline);

    let progressed = if after >= start { 1 } else { 0 };
    if wall > 0 && progressed == 1 && remain == 0 && expired == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_time_parse_and_format_roundtrip_are_deterministic() {
    let src = r#"
import std.io;
import std.time;
import std.string;

fn same_datetime(a: DateTime, b: DateTime) -> Int {
    if a.year == b.year &&
        a.month == b.month &&
        a.day == b.day &&
        a.hour == b.hour &&
        a.minute == b.minute &&
        a.second == b.second &&
        a.millisecond == b.millisecond &&
        a.offset_minutes == b.offset_minutes {
        1
    } else {
        0
    }
}

fn fallback() -> DateTime {
    DateTime {
        year: 0,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
        millisecond: 0,
        offset_minutes: 0,
    }
}

fn main() -> Int effects { io, time } capabilities { io, time  } {
    let parsed = match parse_rfc3339("2026-02-21T18:45:10.230+05:30") {
        Ok(value) => value,
        Err(_) => fallback(),
    };
    let normalized = match format_rfc3339(parsed) {
        Ok(value) => value,
        Err(_) => "",
    };
    let reparsed = match parse_rfc3339(normalized) {
        Ok(value) => value,
        Err(_) => fallback(),
    };
    let date_only = match parse_iso8601("2026-02-21") {
        Ok(value) => value,
        Err(_) => fallback(),
    };
    let iso_text = match format_iso8601(date_only) {
        Ok(value) => value,
        Err(_) => "",
    };
    let iso_reparsed = match parse_iso8601(iso_text) {
        Ok(value) => value,
        Err(_) => fallback(),
    };

    let roundtrip_ok = same_datetime(parsed, reparsed);
    let rfc_shape_ok = if len(normalized) == 29 { 1 } else { 0 };
    let iso_shape_ok = if len(iso_text) == 29 { 1 } else { 0 };
    let iso_roundtrip_ok = same_datetime(date_only, iso_reparsed);
    if roundtrip_ok + rfc_shape_ok + iso_shape_ok + iso_roundtrip_ok == 4 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_time_arithmetic_and_custom_format_are_deterministic() {
    let src = r#"
import std.io;
import std.time;
import std.string;

fn format_text_score(text: String) -> Int {
    let remove_date = replace(text, "2026-02-21", "");
    let remove_clock = replace(text, "22:30:15.120", "");
    let remove_offset = replace(text, "+05:30", "");
    let has_date = len(remove_date) < len(text);
    let has_clock = len(remove_clock) < len(text);
    let has_offset = len(remove_offset) < len(text);
    if len(text) == 30 && has_date && has_clock && has_offset {
        1
    } else {
        0
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let base = DateTime {
        year: 2026,
        month: 2,
        day: 21,
        hour: 22,
        minute: 30,
        second: 15,
        millisecond: 120,
        offset_minutes: 330,
    };

    let plus_days = add_days(base, 30);
    let plus_hours = add_hours(base, 5);
    let diff_s = diff_seconds(plus_hours, base);
    let diff_d = diff_days(plus_days, base);
    let dow = day_of_week(base);
    let leap_ok =
        if is_leap_year(2000) && !is_leap_year(1900) && is_leap_year(2024) {
            1
        } else {
            0
        };

    let format_ok = match format_custom(base, "%Y-%m-%d %H:%M:%S.%L %z") {
        Ok(text) => format_text_score(text),
        Err(_) => 0,
    };
    let invalid_pattern_ok = match format_custom(base, "%Q") {
        Ok(_) => 0,
        Err(err) => match err {
            InvalidFormat => 1,
            _ => 0,
        },
    };

    let rollover_days_ok =
        if plus_days.year == 2026 && plus_days.month == 3 && plus_days.day == 23 {
            1
        } else {
            0
        };
    let rollover_hours_ok =
        if plus_hours.year == 2026 && plus_hours.month == 2 && plus_hours.day == 22 && plus_hours.hour == 3 {
            1
        } else {
            0
        };

    let score =
        rollover_days_ok +
        rollover_hours_ok +
        (if diff_s == 18000 { 1 } else { 0 }) +
        (if diff_d == 30 { 1 } else { 0 }) +
        (if dow == 5 { 1 } else { 0 }) +
        leap_ok +
        format_ok +
        invalid_pattern_ok;

    if score == 8 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_time_invalid_inputs_return_stable_error_variants() {
    let src = r#"
import std.io;
import std.time;

fn code(err: TimeError) -> Int {
    match err {
        InvalidFormat => 1,
        InvalidDate => 2,
        InvalidTime => 3,
        InvalidOffset => 4,
        InvalidInput => 5,
        Internal => 6,
    }
}

fn parse_code(v: Result[DateTime, TimeError]) -> Int {
    match v {
        Ok(_) => 0,
        Err(err) => code(err),
    }
}

fn format_code(v: Result[String, TimeError]) -> Int {
    match v {
        Ok(_) => 0,
        Err(err) => code(err),
    }
}

fn main() -> Int effects { io, time } capabilities { io, time  } {
    let invalid_format = parse_code(parse_rfc3339("2026-02-21 18:45:10Z"));
    let invalid_date = parse_code(parse_iso8601("2025-02-29"));
    let invalid_time = parse_code(parse_iso8601("2026-02-21T24:00:00Z"));
    let invalid_offset = parse_code(parse_rfc3339("2026-02-21T18:45:10.000+15:00"));
    let bad_format_offset = format_code(format_iso8601(DateTime {
        year: 2026,
        month: 2,
        day: 21,
        hour: 8,
        minute: 15,
        second: 0,
        millisecond: 0,
        offset_minutes: 901,
    }));

    if invalid_format == 1 &&
        invalid_date == 2 &&
        invalid_time == 3 &&
        invalid_offset == 4 &&
        bad_format_offset == 4 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_rand_seed_reproducibility_and_range() {
    let src = r#"
import std.io;
import std.rand;

fn bool_eq(a: Bool, b: Bool) -> Int {
    if a == b { 1 } else { 0 }
}

fn main() -> Int effects { io, rand } capabilities { io, rand  } {
    seed(20260220);
    let a = random_int();
    let b = random_int();
    let c = random_range(10, 20);
    let d = random_bool();
    let fixed = random_range(5, 5);

    seed(20260220);
    let a2 = random_int();
    let b2 = random_int();
    let c2 = random_range(10, 20);
    let d2 = random_bool();

    let same = if a == a2 && b == b2 && c == c2 && bool_eq(d, d2) == 1 { 1 } else { 0 };
    let in_range = if c >= 10 && c < 20 && fixed == 5 { 1 } else { 0 };

    if same == 1 && in_range == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_rand_and_time_honor_test_mode_env_overrides() {
    let src = r#"
import std.io;
import std.rand;
import std.time;

fn main() -> Int effects { io, rand, time } capabilities { io, rand, time  } {
    let now = now_ms();
    let first = random_int();

    seed(42);
    let replay_a = random_int();
    seed(42);
    let replay_b = random_int();

    if now == 0 && first == replay_a && replay_a == replay_b {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run_with_setup_and_args_and_input_and_env(
        src,
        &[],
        "",
        &[
            ("AIC_TEST_MODE", "1"),
            ("AIC_TEST_SEED", "42"),
            ("AIC_TEST_TIME_MS", "0"),
        ],
        |_| {},
    );
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_retry_succeeds_after_backoff() {
    let src = r#"
import std.io;
import std.rand;
import std.retry;
import std.time;

fn flaky_connect() -> Result[Int, String] effects { rand } capabilities { rand  } {
    let roll = random_range(0, 4);
    if roll == 2 {
        Ok(42)
    } else {
        Err("transient")
    }
}

fn run_retry_int(config: RetryConfig, operation: Fn() -> Result[Int, String]) -> RetryResult[Int] effects { time, rand } capabilities { time, rand  } {
    retry(config, operation)
}

fn main() -> Int effects { io, time, rand } capabilities { io, time, rand  } {
    seed(424242);
    let cfg = RetryConfig {
        max_attempts: 5,
        initial_backoff_ms: 2,
        backoff_multiplier: 2,
        max_backoff_ms: 20,
        jitter_enabled: false,
        jitter_ms: 0,
    };
    let out = run_retry_int(cfg, | | -> Result[Int, String] { flaky_connect() });

    let value_ok = match out.result {
        Ok(v) => if v == 42 { 1 } else { 0 },
        Err(_) => 0,
    };
    let attempts_ok = if out.attempts == 4 { 1 } else { 0 };
    let elapsed_ok = if out.elapsed_ms >= 14 { 1 } else { 0 };

    if value_ok + attempts_ok + elapsed_ok == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_with_timeout_enforces_deadline_semantics() {
    let src = r#"
import std.io;
import std.retry;
import std.time;

fn main() -> Int effects { io, time } capabilities { io, time  } {
    let timeout_result = with_timeout(5, | | -> Int {
        sleep_ms(20);
        7
    });
    let success_result = with_timeout(50, | | -> Int {
        sleep_ms(2);
        9
    });
    let immediate_result = with_timeout(0, | | -> Int { 1 });

    let timeout_ok = match timeout_result {
        Ok(_) => 0,
        Err(_) => 1,
    };
    let success_ok = match success_result {
        Ok(v) => if v == 9 { 1 } else { 0 },
        Err(_) => 0,
    };
    let immediate_ok = match immediate_result {
        Ok(_) => 0,
        Err(_) => 1,
    };

    if timeout_ok + success_ok + immediate_ok == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_concurrency_worker_pool_is_deterministic() {
    let src = r#"
import std.io;
import std.concurrent;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn unwrap_task(v: Result[Task[Int], ConcurrencyError]) -> Task[Int] {
    match v {
        Ok(task) => task,
        Err(_) => Task { handle: 0 },
    }
}

fn unwrap_channel(v: Result[IntChannel, ConcurrencyError]) -> IntChannel {
    match v {
        Ok(ch) => ch,
        Err(_) => IntChannel { handle: 0 },
    }
}

fn unwrap_mutex(v: Result[IntMutex, ConcurrencyError]) -> IntMutex {
    match v {
        Ok(m) => m,
        Err(_) => IntMutex { handle: 0 },
    }
}

fn unwrap_int(v: Result[Int, ConcurrencyError]) -> Int {
    match v {
        Ok(value) => value,
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let t1 = unwrap_task(spawn_task(10, 20));
    let t2 = unwrap_task(spawn_task(11, 5));
    let ch = unwrap_channel(channel_int(4));
    let m = unwrap_mutex(mutex_int(0));

    let r1 = unwrap_int(join_task(t1));
    let r2 = unwrap_int(join_task(t2));

    let sent1 = match send_int(ch, r1, 1000) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let sent2 = match send_int(ch, r2, 1000) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };

    let a = unwrap_int(recv_int(ch, 1000));
    let b = unwrap_int(recv_int(ch, 1000));

    let base = unwrap_int(lock_int(m, 1000));
    let _release = unlock_int(m, base + a + b);
    let total = unwrap_int(lock_int(m, 1000));
    let _release_final = unlock_int(m, total);

    let closed_ch = match close_channel(ch) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let closed_m = match close_mutex(m) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };

    if sent1 + sent2 + closed_ch + closed_m == 4 && total == 42 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_generic_channels_support_string_struct_and_vec() {
    let src = r#"
import std.concurrent;
import std.io;
import std.string;
import std.string;
import std.vec;

struct Message {
    id: Int,
    label: String,
}

fn vec_values_ok(values: Vec[Int]) -> Int effects { env } capabilities { env  } {
    let a = match vec.get(values, 0) { Some(v) => v, None => -1 };
    let b = match vec.get(values, 1) { Some(v) => v, None => -1 };
    let c = match vec.get(values, 2) { Some(v) => v, None => -1 };
    if vec.vec_len(values) == 3 && a == 7 && b == 8 && c == 9 { 1 } else { 0 }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let pair_s: (Sender[String], Receiver[String]) = channel();
    let tx_s = pair_s.0;
    let rx_s = pair_s.1;
    let string_ok = match send(tx_s, "hello") {
        Ok(_) => match recv(rx_s) {
            Ok(value) => if len(value) == 5 { 1 } else { 0 },
            Err(_) => 0,
        },
        Err(_) => 0,
    };

    let mut payload: Vec[Int] = vec.new_vec();
    payload = vec.push(payload, 7);
    payload = vec.push(payload, 8);
    payload = vec.push(payload, 9);
    let pair_v: (Sender[Vec[Int]], Receiver[Vec[Int]]) = buffered_channel(2);
    let tx_v = pair_v.0;
    let rx_v = pair_v.1;
    let vec_send_ok = match send(tx_v, payload) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let vec_recv_ok = match recv(rx_v) {
        Ok(values) => vec_values_ok(values),
        Err(_) => 0,
    };

    let pair_m: (Sender[Message], Receiver[Message]) = buffered_channel(1);
    let tx_m = pair_m.0;
    let rx_m = pair_m.1;
    let msg_send_ok = match send(tx_m, Message { id: 21, label: "done" }) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let msg_recv_ok = match recv(rx_m) {
        Ok(msg) => if msg.id == 21 && len(msg.label) == 4 { 1 } else { 0 },
        Err(_) => 0,
    };

    let _close_tx_s = close_sender(tx_s);
    let _close_rx_s = close_receiver(rx_s);
    let _close_tx_v = close_sender(tx_v);
    let _close_rx_v = close_receiver(rx_v);
    let _close_tx_m = close_sender(tx_m);
    let _close_rx_m = close_receiver(rx_m);

    if string_ok + vec_send_ok + vec_recv_ok + msg_send_ok + msg_recv_ok == 5 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_bytes_channel_path_roundtrips_binary_payloads() {
    let src = r#"
import std.bytes;
import std.concurrent;
import std.io;
import std.vec;

fn make_payload() -> Bytes effects { env } capabilities { env } {
    let mut raw: Vec[UInt8] = vec.new_vec();
    raw = vec.push(raw, 0);
    raw = vec.push(raw, 255);
    raw = vec.push(raw, 16);
    match bytes.from_byte_values(raw) {
        Ok(data) => data,
        Err(_) => bytes.empty(),
    }
}

fn payload_ok(data: Bytes) -> Int effects { env } capabilities { env } {
    let values = bytes.to_byte_values(data);
    let a = match vec.get(values, 0) { Some(v) => v, None => 0 };
    let b = match vec.get(values, 1) { Some(v) => v, None => 0 };
    let c = match vec.get(values, 2) { Some(v) => v, None => 0 };
    if vec.vec_len(values) == 3 && a == 0 && b == 255 && c == 16 { 1 } else { 0 }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env } {
    let pair: (Sender[Bytes], Receiver[Bytes]) = buffered_bytes_channel(2);
    let tx = pair.0;
    let rx = pair.1;

    let sent = match send_bytes(tx, make_payload()) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let recv_ok = match recv_bytes(rx) {
        Ok(data) => payload_ok(data),
        Err(_) => 0,
    };
    let try_send_ok = match try_send_bytes(tx, make_payload()) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let timeout_recv_ok = match recv_bytes_timeout(rx, 1000) {
        Ok(data) => payload_ok(data),
        Err(_) => 0,
    };
    let empty_ok = match try_recv_bytes(rx) {
        Ok(_) => 0,
        Err(_) => 1,
    };

    let close_tx = match close_sender(tx) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let close_rx = match close_receiver(rx) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if sent + recv_ok + try_send_ok + timeout_recv_ok + empty_ok + close_tx + close_rx == 7 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_bytes_channel_benchmark_reduces_overhead_vs_json_path() {
    let src = r#"
import std.bytes;
import std.concurrent;
import std.io;
import std.json;
import std.time;

fn payload_template() -> Bytes {
    bytes.from_string("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
}

fn run_manual_json_codec_path(iterations: Int) -> Int effects { concurrency, time } capabilities { concurrency, time } {
    let pair: (Sender[String], Receiver[String]) = buffered_channel(64);
    let tx = pair.0;
    let rx = pair.1;
    let expected_len = bytes.byte_len(payload_template());
    let started = monotonic_ms();
    let mut i = 0;
    let mut ok = 1;
    while i < iterations {
        let sent = match json.encode(payload_template()) {
            Ok(encoded) => match json.stringify(encoded) {
                Ok(payload_text) => match send(tx, payload_text) {
                    Ok(_) => 1,
                    Err(_) => 0,
                },
                Err(_) => 0,
            },
            Err(_) => 0,
        };
        let recv_ok = match recv(rx) {
            Ok(payload_text) => match json.parse(payload_text) {
                Ok(parsed) => match json.decode_with(parsed, Some(payload_template())) {
                    Ok(value) => if bytes.byte_len(value) == expected_len { 1 } else { 0 },
                    Err(_) => 0,
                },
                Err(_) => 0,
            },
            Err(_) => 0,
        };
        ok = ok * sent * recv_ok;
        i = i + 1;
    };
    let elapsed = monotonic_ms() - started;
    let _close_tx = close_sender(tx);
    let _close_rx = close_receiver(rx);
    if ok == 1 { elapsed } else { -1 }
}

fn run_binary_path(iterations: Int) -> Int effects { concurrency, time } capabilities { concurrency, time } {
    let pair: (Sender[Bytes], Receiver[Bytes]) = buffered_bytes_channel(64);
    let tx = pair.0;
    let rx = pair.1;
    let expected_len = bytes.byte_len(payload_template());
    let started = monotonic_ms();
    let mut i = 0;
    let mut ok = 1;
    while i < iterations {
        let sent = match send_bytes(tx, payload_template()) {
            Ok(_) => 1,
            Err(_) => 0,
        };
        let recv_ok = match recv_bytes(rx) {
            Ok(value) => if bytes.byte_len(value) == expected_len { 1 } else { 0 },
            Err(_) => 0,
        };
        ok = ok * sent * recv_ok;
        i = i + 1;
    };
    let elapsed = monotonic_ms() - started;
    let _close_tx = close_sender(tx);
    let _close_rx = close_receiver(rx);
    if ok == 1 { elapsed } else { -1 }
}

fn main() -> Int effects { io, concurrency, time } capabilities { io, concurrency, time } {
    let iterations = 2500;
    let json_elapsed = run_manual_json_codec_path(iterations);
    let binary_elapsed = run_binary_path(iterations);
    let measured = if json_elapsed >= 0 && binary_elapsed >= 0 { 1 } else { 0 };
    let improved = if binary_elapsed <= json_elapsed { 1 } else { 0 };
    if measured == 1 && improved == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_generic_mutex_supports_map_and_vec_payloads() {
    let src = r#"
import std.concurrent;
import std.io;
import std.map;
import std.vec;

fn map_write_guard(g: MutexGuard[Map[String, Int]]) -> Int effects { concurrency, env } capabilities { concurrency, env  } {
    let next_map = map.insert(g.value, "count", 42);
    let updated = guard_set(g, next_map);
    unlock_guard(updated);
    1
}

fn map_write(m: Mutex[Map[String, Int]]) -> Int effects { concurrency, env } capabilities { concurrency, env  } {
    match lock(m) {
        Ok(g) => map_write_guard(g),
        Err(_) => 0,
    }
}

fn map_read_guard(g: MutexGuard[Map[String, Int]]) -> Int effects { concurrency, env } capabilities { concurrency, env  } {
    let out = match map.get(g.value, "count") {
        Some(v) => v,
        None => 0,
    };
    unlock_guard(g);
    out
}

fn map_read(m: Mutex[Map[String, Int]]) -> Int effects { concurrency, env } capabilities { concurrency, env  } {
    match lock(m) {
        Ok(g) => map_read_guard(g),
        Err(_) => 0,
    }
}

fn vec_write_guard(g: MutexGuard[Vec[Int]]) -> Int effects { concurrency, env } capabilities { concurrency, env  } {
    let next_vec = vec.push(g.value, 9);
    let updated = guard_set(g, next_vec);
    unlock_guard(updated);
    1
}

fn vec_write(m: Mutex[Vec[Int]]) -> Int effects { concurrency, env } capabilities { concurrency, env  } {
    match lock(m) {
        Ok(g) => vec_write_guard(g),
        Err(_) => 0,
    }
}

fn vec_read_guard(g: MutexGuard[Vec[Int]]) -> Int effects { concurrency, env } capabilities { concurrency, env  } {
    let out = match vec.get(g.value, 1) {
        Some(v) => v,
        None => 0,
    };
    unlock_guard(g);
    out
}

fn vec_read(m: Mutex[Vec[Int]]) -> Int effects { concurrency, env } capabilities { concurrency, env  } {
    match lock(m) {
        Ok(g) => vec_read_guard(g),
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let base_map: Map[String, Int] = map.new_map();
    let seeded_map = map.insert(base_map, "count", 1);
    let map_mutex: Mutex[Map[String, Int]] = new_mutex(seeded_map);
    let map_write_ok = map_write(map_mutex);
    let map_value = map_read(map_mutex);

    let base_vec: Vec[Int] = vec.vec_of(7);
    let vec_mutex: Mutex[Vec[Int]] = new_mutex(base_vec);
    let vec_write_ok = vec_write(vec_mutex);
    let vec_second = vec_read(vec_mutex);

    if map_write_ok + vec_write_ok == 2 && map_value == 42 && vec_second == 9 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_rwlock_reader_writer_contention_is_enforced() {
    let src = r#"
import std.concurrent;
import std.io;

fn read_ok(rw: RwLock[Int]) -> Int effects { concurrency } capabilities { concurrency  } {
    match read_lock(rw) {
        Ok(value) => if value == 7 { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn writer_release(g: MutexGuard[Int], release_rx: Receiver[Int]) -> Int effects { concurrency } capabilities { concurrency  } {
    let released = match recv(release_rx) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    unlock_guard(g);
    released
}

fn writer_with_guard(
    g: MutexGuard[Int],
    entered_tx: Sender[Int],
    release_rx: Receiver[Int],
) -> Int effects { concurrency } capabilities { concurrency  } {
    let entered = match send(entered_tx, 1) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let released = writer_release(g, release_rx);
    if entered == 1 && released == 1 { 1 } else { 0 }
}

fn writer_hold_and_release(
    rw: RwLock[Int],
    entered_tx: Sender[Int],
    release_rx: Receiver[Int],
) -> Int effects { concurrency } capabilities { concurrency  } {
    match write_lock(rw) {
        Ok(g) => writer_with_guard(g, entered_tx, release_rx),
        Err(_) => 0,
    }
}

fn publish_reader_value(value: Int, out_tx: Sender[Int]) -> Int effects { concurrency } capabilities { concurrency  } {
    let sent = match send(out_tx, value) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    if sent == 1 && value == 7 { 1 } else { 0 }
}

fn read_and_publish(rw: RwLock[Int], out_tx: Sender[Int]) -> Int effects { concurrency } capabilities { concurrency  } {
    match read_lock(rw) {
        Ok(value) => publish_reader_value(value, out_tx),
        Err(_) => 0,
    }
}

fn recv_one_or_zero(rx: Receiver[Int]) -> Int effects { concurrency } capabilities { concurrency  } {
    match recv(rx) {
        Ok(value) => value,
        Err(_) => 0,
    }
}

fn try_recv_empty(rx: Receiver[Int]) -> Int effects { concurrency } capabilities { concurrency  } {
    match try_recv(rx) {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let rw: RwLock[Int] = new_rwlock(7);

    let t1: Task[Int] = spawn_named("read-1", || -> Int { read_ok(rw) });
    let t2: Task[Int] = spawn_named("read-2", || -> Int { read_ok(rw) });
    let readers_ok = match join_value(t1) {
        Ok(a) => match join_value(t2) {
            Ok(b) => if a + b == 2 { 1 } else { 0 },
            Err(_) => 0,
        },
        Err(_) => 0,
    };

    let entered_pair: (Sender[Int], Receiver[Int]) = buffered_channel(1);
    let release_pair: (Sender[Int], Receiver[Int]) = buffered_channel(1);
    let blocked_pair: (Sender[Int], Receiver[Int]) = buffered_channel(1);

    let entered_tx = entered_pair.0;
    let entered_rx = entered_pair.1;
    let release_tx = release_pair.0;
    let release_rx = release_pair.1;
    let blocked_tx = blocked_pair.0;
    let blocked_rx = blocked_pair.1;

    let writer: Task[Int] = spawn_named("writer", || -> Int {
        writer_hold_and_release(rw, entered_tx, release_rx)
    });
    let writer_entered = match recv(entered_rx) {
        Ok(v) => if v == 1 { 1 } else { 0 },
        Err(_) => 0,
    };

    let blocked_reader: Task[Int] = spawn_named("blocked-reader", || -> Int {
        read_and_publish(rw, blocked_tx)
    });

    let blocked_before_release = try_recv_empty(blocked_rx);
    let released = match send(release_tx, 1) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let blocked_value = recv_one_or_zero(blocked_rx);
    let writer_ok = match join_value(writer) {
        Ok(v) => v,
        Err(_) => 0,
    };
    let blocked_reader_ok = match join_value(blocked_reader) {
        Ok(v) => v,
        Err(_) => 0,
    };
    let final_read = match read_lock(rw) {
        Ok(value) => value,
        Err(_) => 0,
    };
    let closed_ok = match close_rwlock(rw) {
        Ok(ok) => if ok { 1 } else { 0 },
        Err(_) => 0,
    };

    if readers_ok == 1
        && writer_entered == 1
        && blocked_before_release == 1
        && released == 1
        && blocked_value == 7
        && writer_ok == 1
        && blocked_reader_ok == 1
        && final_read == 7
        && closed_ok == 1
    {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_arc_shares_config_across_threads() {
    let src = r#"
import std.concurrent;
import std.io;

struct Config {
    port: Int,
    host: String,
}

fn read_port(shared: Arc[Config]) -> Int effects { concurrency } capabilities { concurrency } {
    match arc_get(shared) {
        Ok(cfg) => cfg.port,
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env } {
    let shared: Arc[Config] = arc_new(Config {
        port: 8080,
        host: "localhost",
    });

    let c1: Arc[Config] = arc_clone(shared);
    let c2: Arc[Config] = arc_clone(shared);
    let t1: Task[Int] = spawn_named("config-read-1", || -> Int { read_port(c1) });
    let t2: Task[Int] = spawn_named("config-read-2", || -> Int { read_port(c2) });

    let joined_ok = match join_value(t1) {
        Ok(v1) => match join_value(t2) {
            Ok(v2) => if v1 == 8080 && v2 == 8080 { 1 } else { 0 },
            Err(_) => 0,
        },
        Err(_) => 0,
    };

    let main_ok = match arc_get(shared) {
        Ok(cfg) => if cfg.port == 8080 { 1 } else { 0 },
        Err(_) => 0,
    };

    if joined_ok == 1 && main_ok == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_arc_mutex_map_and_refcount_lifecycle() {
    let src = r#"
import std.concurrent;
import std.io;
import std.map;

fn increment_guard(g: MutexGuard[Map[String, Int]]) -> Int effects { concurrency, env } capabilities { concurrency, env } {
    let current = match map.get(g.value, "count") {
        Some(v) => v,
        None => 0,
    };
    let next = map.insert(g.value, "count", current + 1);
    let updated = guard_set(g, next);
    unlock_guard(updated);
    1
}

fn increment_once(shared: Arc[Mutex[Map[String, Int]]]) -> Int effects { concurrency, env } capabilities { concurrency, env } {
    match arc_get(shared) {
        Ok(mutex) => match lock(mutex) {
            Ok(g) => increment_guard(g),
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

fn read_guard(g: MutexGuard[Map[String, Int]]) -> Int effects { concurrency, env } capabilities { concurrency, env } {
    let out = match map.get(g.value, "count") {
        Some(v) => v,
        None => 0,
    };
    unlock_guard(g);
    out
}

fn read_count(shared: Arc[Mutex[Map[String, Int]]]) -> Int effects { concurrency, env } capabilities { concurrency, env } {
    match arc_get(shared) {
        Ok(mutex) => match lock(mutex) {
            Ok(g) => read_guard(g),
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

fn make_and_drop_clone(shared: Arc[Int]) -> Int effects { concurrency } capabilities { concurrency } {
    let cloned: Arc[Int] = arc_clone(shared);
    let count_int = arc_strong_count(shared);
    let count_u32 = match arc_strong_count_u32(shared) {
        Ok(value) => if value == 2u32 { 1 } else { 0 },
        Err(_) => 0,
    };
    if cloned.handle > 0 && count_int == 2 && count_u32 == 1 { 1 } else { 0 }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env } {
    let base_map: Map[String, Int] = map.new_map();
    let seeded = map.insert(base_map, "count", 0);
    let shared: Arc[Mutex[Map[String, Int]]] = arc_new(new_mutex(seeded));

    let c1: Arc[Mutex[Map[String, Int]]] = arc_clone(shared);
    let c2: Arc[Mutex[Map[String, Int]]] = arc_clone(shared);
    let t1: Task[Int] = spawn_named("inc-1", || -> Int { increment_once(c1) });
    let t2: Task[Int] = spawn_named("inc-2", || -> Int { increment_once(c2) });

    let joined = match join_value(t1) {
        Ok(v1) => match join_value(t2) {
            Ok(v2) => v1 + v2,
            Err(_) => 0,
        },
        Err(_) => 0,
    };
    let final_count = read_count(shared);

    let ref_shared: Arc[Int] = arc_new(9);
    let before = arc_strong_count(ref_shared);
    let before_u32 = match arc_strong_count_u32(ref_shared) {
        Ok(value) => if value == 1u32 { 1 } else { 0 },
        Err(_) => 0,
    };
    let inside = make_and_drop_clone(ref_shared);
    let after = arc_strong_count(ref_shared);
    let after_u32 = match arc_strong_count_u32(ref_shared) {
        Ok(value) => if value == 1u32 { 1 } else { 0 },
        Err(_) => 0,
    };

    if joined == 2
        && final_count == 2
        && before == 1
        && before_u32 == 1
        && inside == 1
        && after == 1
        && after_u32 == 1
    {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_atomic_int_ten_threads_thousand_increments_each() {
    let src = r#"
import std.concurrent;
import std.io;
import std.vec;

fn bump_many(counter: AtomicInt, iterations: Int) -> Int effects { concurrency } capabilities { concurrency } {
    let mut i = 0;
    while i < iterations {
        let _old = atomic_add(counter, 1);
        i = i + 1;
    };
    1
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env } {
    let counter = atomic_int(0);
    let mut tasks: Vec[Task[Int]] = vec.new_vec();
    let mut i = 0;
    while i < 10 {
        let task: Task[Int] = spawn_named("atomic-inc", || -> Int { bump_many(counter, 1000) });
        tasks = vec.push(tasks, task);
        i = i + 1;
    };

    let mut joined = 0;
    let mut j = 0;
    while j < tasks.len {
        joined = joined + match vec.get(tasks, j) {
            Some(task) => match join_value(task) {
                Ok(v) => v,
                Err(_) => 0,
            },
            None => 0,
        };
        j = j + 1;
    };

    let total = atomic_load(counter);
    if joined == 10 && total == 10000 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_atomic_ops_cas_and_bool_swap_are_consistent() {
    let src = r#"
import std.concurrent;
import std.io;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env } {
    let num = atomic_int(10);
    let old_add = atomic_add(num, 5);
    let old_sub = atomic_sub(num, 3);
    let cas_ok = atomic_cas(num, 12, 99);
    let cas_fail = atomic_cas(num, 12, 77);
    let final_num = atomic_load(num);

    let flag = atomic_bool(false);
    let old_first = atomic_swap_bool(flag, true);
    atomic_store_bool(flag, false);
    let old_second = atomic_swap_bool(flag, true);
    let final_flag = atomic_load_bool(flag);

    let pass = if old_add == 10
        && old_sub == 15
        && cas_ok
        && !cas_fail
        && final_num == 99
        && !old_first
        && !old_second
        && final_flag {
        1
    } else {
        0
    };

    if pass == 1 {
        print_int(42);
    } else {
        print_int(bool_to_int(cas_ok) + bool_to_int(final_flag));
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_thread_local_isolates_values_between_threads() {
    let src = r#"
import std.concurrent;
import std.io;

fn worker_set_get(tl: ThreadLocal[Int], value: Int) -> Int effects { concurrency } capabilities { concurrency } {
    let before = tl_get(tl);
    tl_set(tl, value);
    let after = tl_get(tl);
    before * 100 + after
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env } {
    let tl = thread_local(|| -> Int { 7 });

    let t1: Task[Int] = spawn_named("tl-1", || -> Int { worker_set_get(tl, 11) });
    let t2: Task[Int] = spawn_named("tl-2", || -> Int { worker_set_get(tl, 22) });

    let main_before = tl_get(tl);
    let r1 = match join_value(t1) {
        Ok(v) => v,
        Err(_) => 0,
    };
    let r2 = match join_value(t2) {
        Ok(v) => v,
        Err(_) => 0,
    };
    let main_after = tl_get(tl);

    if r1 == 711 && r2 == 722 && main_before == 7 && main_after == 7 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_thread_local_init_is_lazy_per_thread() {
    let src = r#"
import std.concurrent;
import std.io;

fn read_twice(tl: ThreadLocal[Int]) -> Int effects { concurrency } capabilities { concurrency } {
    let first = tl_get(tl);
    let second = tl_get(tl);
    if first == 100 && second == 100 { 1 } else { 0 }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env } {
    let init_runs = atomic_int(0);
    let tl = thread_local(|| -> Int {
        let _old = atomic_add(init_runs, 1);
        100
    });

    let before = atomic_load(init_runs);
    let t1: Task[Int] = spawn_named("tl-lazy-1", || -> Int { read_twice(tl) });
    let t2: Task[Int] = spawn_named("tl-lazy-2", || -> Int { read_twice(tl) });

    let main_first = tl_get(tl);
    let main_second = tl_get(tl);
    let j1 = match join_value(t1) {
        Ok(v) => v,
        Err(_) => 0,
    };
    let j2 = match join_value(t2) {
        Ok(v) => v,
        Err(_) => 0,
    };
    let after = atomic_load(init_runs);

    if before == 0 && main_first == 100 && main_second == 100 && j1 == 1 && j2 == 1 && after == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_spawn_join_generic_closure_capture_is_stable() {
    let src = r#"
import std.concurrent;
import std.io;
import std.string;

struct Job {
    id: Int,
    label: String,
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let base = 40;
    let int_task: Task[Int] = spawn_named("int", || -> Int { base + 2 });
    let int_ok = match join_value(int_task) {
        Ok(value) => if value == 42 { 1 } else { 0 },
        Err(_) => 0,
    };

    let job_task: Task[Job] = spawn_named("job", || -> Job { Job { id: 7, label: "ok" } });
    let job_ok = match join_value(job_task) {
        Ok(job) => if job.id == 7 && len(job.label) == 2 { 1 } else { 0 },
        Err(_) => 0,
    };

    if int_ok + job_ok == 2 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_scoped_spawn_joins_before_scope_exit() {
    let src = r#"
import std.concurrent;
import std.io;

fn recv_or_zero(rx: Receiver[Int]) -> Int effects { concurrency } capabilities { concurrency  } {
    match try_recv(rx) {
        Ok(v) => v,
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let pair: (Sender[Int], Receiver[Int]) = buffered_channel(4);
    let tx = pair.0;
    let rx = pair.1;
    let tx1: Sender[Int] = Sender { handle: tx.handle };
    let tx2: Sender[Int] = Sender { handle: tx.handle };

    let scoped_ok = scoped(|scope: Scope| -> Int {
        let _first: Task[Int] = scope_spawn(scope, || -> Int {
            let _sent = send(tx1, 11);
            11
        });
        let _second: Task[Int] = scope_spawn(scope, || -> Int {
            let _sent = send(tx2, 31);
            31
        });
        1
    });

    let rx1: Receiver[Int] = Receiver { handle: rx.handle };
    let rx2: Receiver[Int] = Receiver { handle: rx.handle };
    let first = recv_or_zero(rx1);
    let second = recv_or_zero(rx2);
    let _close_tx = close_sender(tx);
    let _close_rx = close_receiver(rx);

    if scoped_ok == 1 && first + second == 42 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_cancellation_timeout_and_panic_are_stable() {
    let src = r#"
import std.io;
import std.concurrent;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn err_code(err: ConcurrencyError) -> Int {
    match err {
        NotFound => 1,
        Timeout => 2,
        Cancelled => 3,
        InvalidInput => 4,
        Panic => 5,
        Closed => 6,
        Io => 7,
    }
}

fn unwrap_task(v: Result[Task[Int], ConcurrencyError]) -> Task[Int] {
    match v {
        Ok(task) => task,
        Err(_) => Task { handle: 0 },
    }
}

fn unwrap_channel(v: Result[IntChannel, ConcurrencyError]) -> IntChannel {
    match v {
        Ok(ch) => ch,
        Err(_) => IntChannel { handle: 0 },
    }
}

fn unwrap_mutex(v: Result[IntMutex, ConcurrencyError]) -> IntMutex {
    match v {
        Ok(m) => m,
        Err(_) => IntMutex { handle: 0 },
    }
}

fn unwrap_int(v: Result[Int, ConcurrencyError]) -> Int {
    match v {
        Ok(value) => value,
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let to_cancel = unwrap_task(spawn_task(9, 80));
    let cancelled = match cancel_task(to_cancel) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let join_cancel = match join_task(to_cancel) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let panic_task = unwrap_task(spawn_task(-1, 1));
    let panic_code = match join_task(panic_task) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let ch = unwrap_channel(channel_int(1));
    let recv_timeout = match recv_int(ch, 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let close_ch = match close_channel(ch) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let recv_closed = match recv_int(ch, 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let m = unwrap_mutex(mutex_int(7));
    let first = unwrap_int(lock_int(m, 20));
    let lock_timeout = match lock_int(m, 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let _unlock = unlock_int(m, first);
    let close_m = match close_mutex(m) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };

    if cancelled == 1 && join_cancel == 3 && panic_code == 5 &&
        recv_timeout == 2 && recv_closed == 6 &&
        first == 7 && lock_timeout == 2 &&
        close_ch == 1 && close_m == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_buffered_try_and_select_channel_paths_are_stable() {
    let src = r#"
import std.io;
import std.concurrent;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn channel_err_code(err: ChannelError) -> Int {
    match err {
        Closed => 1,
        Full => 2,
        Empty => 3,
        Timeout => 4,
    }
}

fn unwrap_channel(v: Result[IntChannel, ConcurrencyError]) -> IntChannel {
    match v {
        Ok(ch) => ch,
        Err(_) => IntChannel { handle: 0 },
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let ch1 = unwrap_channel(buffered_channel_int(1));
    let ch2 = unwrap_channel(channel_int_buffered(1));

    let sent = match send_int(ch1, 10, 1000) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let backpressure = match send_int(ch1, 11, 20) {
        Ok(_) => 0,
        Err(err) => match err {
            Timeout => 1,
            _ => 0,
        },
    };
    let try_full = match try_send_int(ch1, 12) {
        Ok(_) => 0,
        Err(err) => if channel_err_code(err) == 2 { 1 } else { 0 },
    };

    let first_recv = match try_recv_int(ch1) {
        Ok(value) => if value == 10 { 1 } else { 0 },
        Err(_) => 0,
    };
    let empty_recv = match try_recv_int(ch1) {
        Ok(_) => 0,
        Err(err) => if channel_err_code(err) == 3 { 1 } else { 0 },
    };

    let sent_second = match try_send_int(ch2, 32) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let selected = match select_recv_int(ch1, ch2, 100) {
        Ok(selection) => if selection.channel_index == 1 && selection.value == 32 { 1 } else { 0 },
        Err(_) => 0,
    };

    let close1 = match close_channel(ch1) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let close2 = match close_channel(ch2) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let closed_recv = match try_recv_int(ch1) {
        Ok(_) => 0,
        Err(err) => if channel_err_code(err) == 1 { 1 } else { 0 },
    };

    if sent + backpressure + try_full + first_recv + empty_recv + sent_second +
        selected + close1 + close2 + closed_recv == 10 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_select2_and_select_any_cover_fan_in_and_timeout_paths() {
    let src = r#"
import std.concurrent;
import std.io;
import std.vec;

fn channel_err_code(err: ChannelError) -> Int {
    match err {
        Closed => 1,
        Full => 2,
        Empty => 3,
        Timeout => 4,
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let pair_data: (Sender[Int], Receiver[Int]) = buffered_channel(1);
    let tx_data = pair_data.0;
    let rx_data = pair_data.1;
    let pair_quit: (Sender[Bool], Receiver[Bool]) = buffered_channel(1);
    let tx_quit = pair_quit.0;
    let rx_quit = pair_quit.1;

    let first_ok = match send(tx_data, 7) {
        Ok(_) => match select2(rx_data, rx_quit, 100) {
            First(value) => if value == 7 { 1 } else { 0 },
            Second(_) => 0,
            Timeout => 0,
            Closed => 0,
        },
        Err(_) => 0,
    };

    let second_ok = match send(tx_quit, true) {
        Ok(_) => match select2(rx_data, rx_quit, 100) {
            First(_) => 0,
            Second(value) => if value { 1 } else { 0 },
            Timeout => 0,
            Closed => 0,
        },
        Err(_) => 0,
    };

    let timeout_ok = match select2(rx_data, rx_quit, 0) {
        Timeout => 1,
        _ => 0,
    };

    let pair0: (Sender[Int], Receiver[Int]) = buffered_channel(1);
    let pair1: (Sender[Int], Receiver[Int]) = buffered_channel(1);
    let pair2: (Sender[Int], Receiver[Int]) = buffered_channel(1);
    let tx0 = pair0.0;
    let rx0 = pair0.1;
    let tx1 = pair1.0;
    let rx1 = pair1.1;
    let tx2 = pair2.0;
    let rx2 = pair2.1;

    let _send0 = send(tx0, 11);
    let _send1 = send(tx1, 22);
    let _send2 = send(tx2, 33);

    let mut receivers: Vec[Receiver[Int]] = vec.new_vec();
    receivers = vec.push(receivers, rx0);
    receivers = vec.push(receivers, rx1);
    receivers = vec.push(receivers, rx2);

    let pick1 = match select_any(receivers, 20) {
        Ok(found) => if found.0 == 0 && found.1 == 11 { 1 } else { 0 },
        Err(_) => 0,
    };
    let pick2 = match select_any(receivers, 20) {
        Ok(found) => if found.0 == 1 && found.1 == 22 { 1 } else { 0 },
        Err(_) => 0,
    };
    let pick3 = match select_any(receivers, 20) {
        Ok(found) => if found.0 == 2 && found.1 == 33 { 1 } else { 0 },
        Err(_) => 0,
    };

    let timeout_pair: (Sender[Int], Receiver[Int]) = buffered_channel(1);
    let timeout_rx = timeout_pair.1;
    let timeout_any = match select_any(vec.vec_of(timeout_rx), 0) {
        Ok(_) => 0,
        Err(err) => if channel_err_code(err) == 4 { 1 } else { 0 },
    };

    let closed_pair: (Sender[Int], Receiver[Int]) = buffered_channel(1);
    let closed_tx = closed_pair.0;
    let closed_rx = closed_pair.1;
    let _close_closed_tx = close_sender(closed_tx);
    let closed_any = match select_any(vec.vec_of(closed_rx), 0) {
        Ok(_) => 0,
        Err(err) => if channel_err_code(err) == 1 { 1 } else { 0 },
    };

    let _close_data_tx = close_sender(tx_data);
    let _close_data_rx = close_receiver(rx_data);
    let _close_quit_tx = close_sender(tx_quit);
    let _close_quit_rx = close_receiver(rx_quit);

    if first_ok + second_ok + timeout_ok + pick1 + pick2 + pick3 + timeout_any + closed_any == 8 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_pool_ten_workers_share_five_connections_without_leaks() {
    let src = r#"
import std.concurrent;
import std.io;
import std.pool;
import std.vec;

struct FakeConn {
    id: Int,
    healthy: Bool,
}

fn release_and_one(conn: PooledConn[FakeConn]) -> Int effects { concurrency } capabilities { concurrency } {
    release(conn);
    1
}

fn worker_once(pool_ref: Pool[FakeConn]) -> Int effects { concurrency } capabilities { concurrency } {
    let mut attempts = 0;
    let mut out = 0;
    let mut done = false;
    while !done && attempts < 200 {
        let acquired: Result[PooledConn[FakeConn], PoolError] = acquire(pool_ref);
        match acquired {
            Ok(conn) => if true {
                out = release_and_one(conn);
                done = true;
                ()
            } else {
                ()
            },
            Err(_) => if true {
                wait_ms(10);
                attempts = attempts + 1;
                ()
            } else {
                ()
            },
        };
    };
    out
}

fn spawn_worker(pool_ref: Pool[FakeConn]) -> Task[Int] effects { concurrency } capabilities { concurrency } {
    spawn_named("pool-worker", || -> Int { worker_once(pool_ref) })
}

fn wait_ms(ms: Int) -> () effects { concurrency } capabilities { concurrency } {
    match spawn_task(1, ms) {
        Ok(task) => if true {
            let _joined = join_task(task);
            ()
        } else {
            ()
        },
        Err(_) => (),
    }
}

fn wait_for_zero_in_use(pool_ref: Pool[FakeConn], retries: Int) -> PoolStats effects { concurrency } capabilities { concurrency } {
    let mut stats = pool_stats(pool_ref);
    let mut remaining = retries;
    while remaining > 0 && stats.in_use > 0 {
        wait_ms(10);
        stats = pool_stats(pool_ref);
        remaining = remaining - 1;
    };
    stats
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env } {
    let created = atomic_int(0);
    let create_cb: Fn() -> Result[FakeConn, PoolError] = || -> Result[FakeConn, PoolError] {
        let prior = atomic_add(created, 1);
        Ok(FakeConn {
            id: prior + 1,
            healthy: true,
        })
    };
    let check_cb: Fn(FakeConn) -> Bool = |conn: FakeConn| -> Bool { conn.healthy };
    let destroy_cb: Fn(FakeConn) -> () = |conn: FakeConn| -> () { () };

    let pool_result: Result[Pool[FakeConn], PoolError] = new_pool(
        PoolConfig {
            min_size: 5,
            max_size: 5,
            acquire_timeout_ms: 15000,
            idle_timeout_ms: 40,
            max_lifetime_ms: 0,
            health_check_ms: 0,
        },
        create_cb,
        check_cb,
        destroy_cb,
    );
    let pool: Pool[FakeConn] = match pool_result {
        Ok(p) => p,
        Err(_) => Pool { handle: 0 },
    };

    let mut tasks: Vec[Task[Int]] = vec.new_vec();
    let mut spawned = 0;
    let mut attempts = 0;
    while spawned < 10 && attempts < 200 {
        let task = spawn_worker(pool);
        if task.handle > 0 {
            tasks = vec.push(tasks, task);
            spawned = spawned + 1;
        } else {
            wait_ms(10);
        };
        attempts = attempts + 1;
    };

    let mut joined = 0;
    let mut j = 0;
    while j < tasks.len {
        joined = joined + match vec.get(tasks, j) {
            Some(task) => match join_value(task) {
                Ok(v) => v,
                Err(_) => 0,
            },
            None => 0,
        };
        j = j + 1;
    };

    let stats = wait_for_zero_in_use(pool, 200);
    close_pool(pool);

    if spawned == 10 && joined == 10 && stats.total <= 5 && stats.in_use == 0 {
        print_int(42);
    } else {
        print_int(spawned * 1000 + joined * 100 + stats.total * 10 + stats.in_use);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_pool_idle_connections_are_recycled_after_timeout() {
    let src = r#"
import std.concurrent;
import std.io;
import std.pool;

struct FakeConn {
    id: Int,
    healthy: Bool,
}

fn wait_ms(ms: Int) -> () effects { concurrency } capabilities { concurrency } {
    match spawn_task(1, ms) {
        Ok(task) => if true {
            let _joined = join_task(task);
            ()
        } else {
            ()
        },
        Err(_) => (),
    }
}

fn main() -> Int effects { io, concurrency } capabilities { io, concurrency } {
    let created = atomic_int(0);
    let create_cb: Fn() -> Result[FakeConn, PoolError] = || -> Result[FakeConn, PoolError] {
        let prior = atomic_add(created, 1);
        Ok(FakeConn {
            id: prior + 1,
            healthy: true,
        })
    };
    let check_cb: Fn(FakeConn) -> Bool = |conn: FakeConn| -> Bool { conn.healthy };
    let destroy_cb: Fn(FakeConn) -> () = |conn: FakeConn| -> () { () };

    let pool_result: Result[Pool[FakeConn], PoolError] = new_pool(
        PoolConfig {
            min_size: 1,
            max_size: 2,
            acquire_timeout_ms: 30,
            idle_timeout_ms: 4,
            max_lifetime_ms: 0,
            health_check_ms: 0,
        },
        create_cb,
        check_cb,
        destroy_cb,
    );
    let pool: Pool[FakeConn] = match pool_result {
        Ok(p) => p,
        Err(_) => Pool { handle: 0 },
    };

    let first_id = match acquire(pool) {
        Ok(conn) => if true {
            let id = conn.value.id;
            release(conn);
            id
        } else {
            0
        },
        Err(_) => 0,
    };

    wait_ms(20);

    let second_id = match acquire(pool) {
        Ok(conn) => if true {
            let id = conn.value.id;
            release(conn);
            id
        } else {
            0
        },
        Err(_) => 0,
    };

    close_pool(pool);

    if first_id > 0 && second_id > first_id {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_pool_discarded_connection_is_replaced() {
    let src = r#"
import std.concurrent;
import std.io;
import std.pool;

struct FakeConn {
    id: Int,
    healthy: Bool,
}

fn main() -> Int effects { io, concurrency } capabilities { io, concurrency } {
    let created = atomic_int(0);
    let create_cb: Fn() -> Result[FakeConn, PoolError] = || -> Result[FakeConn, PoolError] {
        let prior = atomic_add(created, 1);
        Ok(FakeConn {
            id: prior + 1,
            healthy: true,
        })
    };
    let check_cb: Fn(FakeConn) -> Bool = |conn: FakeConn| -> Bool { conn.healthy };
    let destroy_cb: Fn(FakeConn) -> () = |conn: FakeConn| -> () { () };

    let pool_result: Result[Pool[FakeConn], PoolError] = new_pool(
        PoolConfig {
            min_size: 1,
            max_size: 1,
            acquire_timeout_ms: 30,
            idle_timeout_ms: 0,
            max_lifetime_ms: 0,
            health_check_ms: 0,
        },
        create_cb,
        check_cb,
        destroy_cb,
    );
    let pool: Pool[FakeConn] = match pool_result {
        Ok(p) => p,
        Err(_) => Pool { handle: 0 },
    };

    let first_id = match acquire(pool) {
        Ok(conn) => if true {
            let id = conn.value.id;
            discard(conn);
            id
        } else {
            0
        },
        Err(_) => 0,
    };

    let second_id = match acquire(pool) {
        Ok(conn) => if true {
            let id = conn.value.id;
            release(conn);
            id
        } else {
            0
        },
        Err(_) => 0,
    };

    close_pool(pool);

    if first_id > 0 && second_id > first_id {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_structured_group_timeout_and_select_first_are_stable() {
    let src = r#"
import std.io;
import std.concurrent;
import std.vec;

fn err_code(err: ConcurrencyError) -> Int {
    match err {
        NotFound => 1,
        Timeout => 2,
        Cancelled => 3,
        InvalidInput => 4,
        Panic => 5,
        Closed => 6,
        Io => 7,
    }
}

fn unwrap_task(v: Result[Task[Int], ConcurrencyError]) -> Task[Int] {
    match v {
        Ok(task) => task,
        Err(_) => Task { handle: 0 },
    }
}

fn group_values_ok(values: Vec[Int]) -> Int effects { env } capabilities { env  } {
    let a = match vec.get(values, 0) {
        Some(v) => if v == 2 { 1 } else { 0 },
        None => 0,
    };
    let b = match vec.get(values, 1) {
        Some(v) => if v == 4 { 1 } else { 0 },
        None => 0,
    };
    let c = match vec.get(values, 2) {
        Some(v) => if v == 6 { 1 } else { 0 },
        None => 0,
    };
    if a + b + c == 3 { 1 } else { 0 }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let grouped = spawn_group(vec.range(1, 4), 10);
    let group_ok = match grouped {
        Ok(values) => group_values_ok(values),
        Err(_) => 0,
    };

    let slow = unwrap_task(spawn_task(21, 120));
    let timeout_code = match timeout_task(slow, 5) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let t0 = unwrap_task(spawn_task(1, 80));
    let t1 = unwrap_task(spawn_task(9, 5));
    let t2 = unwrap_task(spawn_task(3, 90));
    let mut race: Vec[Task[Int]] = vec.new_vec();
    race = vec.push(race, t0);
    race = vec.push(race, t1);
    race = vec.push(race, t2);

    let first_ok = match select_first(race, 200) {
        Ok(selection) => if selection.task_index == 1 && selection.value == 18 { 1 } else { 0 },
        Err(_) => 0,
    };
    let drained_after_select =
        (match join_task(t0) { Ok(_) => 0, Err(err) => if err_code(err) == 1 { 1 } else { 0 } }) +
        (match join_task(t1) { Ok(_) => 0, Err(err) => if err_code(err) == 1 { 1 } else { 0 } }) +
        (match join_task(t2) { Ok(_) => 0, Err(err) => if err_code(err) == 1 { 1 } else { 0 } });

    let empty_tasks: Vec[Task[Int]] = vec.new_vec();
    let invalid_empty = match select_first(empty_tasks, 20) {
        Ok(_) => 0,
        Err(err) => if err_code(err) == 4 { 1 } else { 0 },
    };

    if group_ok == 1 && timeout_code == 2 && first_ok == 1 &&
        drained_after_select == 3 && invalid_empty == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_spawn_ten_cancel_three_collect_seven() {
    let src = r#"
import std.io;
import std.concurrent;
import std.vec;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn err_code(err: ConcurrencyError) -> Int {
    match err {
        NotFound => 1,
        Timeout => 2,
        Cancelled => 3,
        InvalidInput => 4,
        Panic => 5,
        Closed => 6,
        Io => 7,
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env  } {
    let mut tasks: Vec[Task[Int]] = vec.new_vec();
    let mut i = 0;
    while i < 10 {
        tasks = match spawn_task(i + 1, 40 + i) {
            Ok(task) => vec.push(tasks, task),
            Err(_) => tasks,
        };
        i = i + 1;
    };

    let mut cancel_ok = 0;
    let mut c = 0;
    while c < 3 {
        cancel_ok = cancel_ok + match vec.get(tasks, c) {
            Some(task) => match cancel_task(task) {
                Ok(done) => bool_to_int(done),
                Err(_) => 0,
            },
            None => 0,
        };
        c = c + 1;
    };

    let mut cancelled = 0;
    let mut completed = 0;
    let mut sum = 0;
    let mut j = 0;
    while j < 10 {
        let join_value_or_err = match vec.get(tasks, j) {
            Some(task) => match join_task(task) {
                Ok(v) => v,
                Err(err) => 0 - err_code(err),
            },
            None => 0 - 7,
        };
        if join_value_or_err >= 0 {
            completed = completed + 1;
            sum = sum + join_value_or_err;
        } else {
            let join_code = 0 - join_value_or_err;
            cancelled = if join_code == 3 {
                cancelled + 1
            } else {
                cancelled
            };
        };
        j = j + 1;
    };

    if cancel_ok == 3 && cancelled == 3 && completed == 7 && sum > 0 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_proc_run_pipe_spawn_wait_and_kill() {
    let src = r#"
import std.proc;
import std.string;

fn main() -> Int effects { proc, env } capabilities { proc, env  } {
    let run_out = match run("echo out; echo err 1>&2; exit 7") {
        Ok(out) => out,
        Err(_) => ProcResult { status: proc_nonnegative_int_to_i32(99), stdout: "", stderr: "" },
    };
    let pipe_out = match pipe("echo 42", "cat") {
        Ok(out) => out,
        Err(_) => ProcResult { status: proc_nonnegative_int_to_i32(99), stdout: "", stderr: "" },
    };
    let spawned = match spawn("exit 5") {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let waited = match wait(spawned) {
        Ok(code) => code,
        Err(_) => -1,
    };
    let kill_missing = match kill(999999) {
        Ok(_) => 0,
        Err(err) => match err {
            UnknownProcess => 1,
            _ => 0,
        },
    };

    let run_status_ok = if run_out.status == proc_nonnegative_int_to_i32(7) { 1 } else { 0 };
    let run_stdout_ok = if len(run_out.stdout) > 0 { 1 } else { 0 };
    let run_stderr_ok = if len(run_out.stderr) > 0 { 1 } else { 0 };
    let pipe_ok = if pipe_out.status == proc_nonnegative_int_to_i32(0) && len(pipe_out.stdout) > 0 { 1 } else { 0 };
    let wait_ok = if waited == 5 { 1 } else { 0 };

    if run_status_ok + run_stdout_ok + run_stderr_ok + pipe_ok + wait_ok + kill_missing == 6 {
        42
    } else {
        0
    }
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 42, "stderr={stderr}");
    assert_eq!(stdout, "");
}

#[cfg(target_os = "windows")]
#[test]
fn exec_proc_run_pipe_spawn_wait_and_kill() {
    let src = r#"
import std.proc;
import std.string;

fn main() -> Int effects { proc, env } capabilities { proc, env  } {
    let run_out = match run("echo out & echo err 1>&2 & exit /B 7") {
        Ok(out) => out,
        Err(_) => ProcResult { status: proc_nonnegative_int_to_i32(99), stdout: "", stderr: "" },
    };
    let pipe_out = match pipe("echo 42", "findstr 42") {
        Ok(out) => out,
        Err(_) => ProcResult { status: proc_nonnegative_int_to_i32(99), stdout: "", stderr: "" },
    };
    let spawned = match spawn("exit /B 5") {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let waited = match wait(spawned) {
        Ok(code) => code,
        Err(_) => -1,
    };
    let kill_missing = match kill(999999) {
        Ok(_) => 0,
        Err(err) => match err {
            UnknownProcess => 1,
            _ => 0,
        },
    };

    let run_status_ok = if run_out.status == proc_nonnegative_int_to_i32(7) { 1 } else { 0 };
    let run_stdout_ok = if string.contains(run_out.stdout, "out") { 1 } else { 0 };
    let run_stderr_ok = if string.contains(run_out.stderr, "err") { 1 } else { 0 };
    let pipe_ok = if pipe_out.status == proc_nonnegative_int_to_i32(0) && string.contains(pipe_out.stdout, "42") { 1 } else { 0 };
    let wait_ok = if waited == 5 { 1 } else { 0 };

    if run_status_ok + run_stdout_ok + run_stderr_ok + pipe_ok + wait_ok + kill_missing == 6 {
        42
    } else {
        0
    }
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 42, "stderr={stderr}");
    assert_eq!(stdout, "");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_proc_run_with_stdin_env_cwd_and_timeout() {
    let src = r#"
import std.proc;
import std.string;
import std.vec;

fn invalid_input_code(err: ProcError) -> Int {
    match err {
        InvalidInput => 1,
        _ => 0,
    }
}

fn main() -> Int effects { proc, env } capabilities { proc, env  } {
    let opts = RunOptions {
        stdin: "from-stdin",
        cwd: "workdir",
        env: vec.vec_of("AIC_PROC_ENV=from-env"),
        timeout_ms: 0,
    };
    let out = match run_with("cat input.txt; printf '|'; printf \"$AIC_PROC_ENV\"; printf '|'; cat", opts) {
        Ok(value) => value,
        Err(_) => ProcResult { status: proc_nonnegative_int_to_i32(99), stdout: "", stderr: "" },
    };

    let empty_env: Vec[String] = vec.new_vec();
    let timeout_opts = RunOptions {
        stdin: "",
        cwd: "",
        env: empty_env,
        timeout_ms: 50,
    };
    let run_with_timeout = match run_with("sleep 2", timeout_opts) {
        Ok(value) => if value.status == 124 { 1 } else { 0 },
        Err(_) => 0,
    };
    let run_timeout_ok = match run_timeout("sleep 2", 50) {
        Ok(value) => if value.status == 124 { 1 } else { 0 },
        Err(_) => 0,
    };
    let invalid_timeout = match run_timeout("echo ok", -1) {
        Ok(_) => 0,
        Err(err) => invalid_input_code(err),
    };

    let run_with_ok = if out.status == 0 &&
        string.contains(out.stdout, "cwd-data|from-env|from-stdin") &&
        len(out.stderr) == 0 {
        1
    } else {
        0
    };

    if run_with_ok + run_with_timeout + run_timeout_ok + invalid_timeout == 4 {
        42
    } else {
        0
    }
}
"#;
    let (code, stdout, stderr) = compile_and_run_with_setup(src, |root| {
        let workdir = root.join("workdir");
        fs::create_dir_all(&workdir).expect("mkdir workdir");
        fs::write(workdir.join("input.txt"), "cwd-data").expect("write workdir input");
    });
    assert_eq!(code, 42, "stderr={stderr}");
    assert_eq!(stdout, "");
}

#[cfg(target_os = "windows")]
#[test]
fn exec_proc_run_with_stdin_env_cwd_and_timeout() {
    let src = r#"
import std.proc;
import std.string;
import std.vec;

fn invalid_input_code(err: ProcError) -> Int {
    match err {
        InvalidInput => 1,
        _ => 0,
    }
}

fn main() -> Int effects { proc, env } capabilities { proc, env  } {
    let opts = RunOptions {
        stdin: "from-stdin",
        cwd: "workdir",
        env: vec.vec_of("AIC_PROC_ENV=from-env"),
        timeout_ms: 0,
    };
    let out = match run_with("type input.txt & set /p =\"|\" <NUL & set /p =\"%AIC_PROC_ENV%|\" <NUL & set /p STDINVAL= & echo %STDINVAL%", opts) {
        Ok(value) => value,
        Err(_) => ProcResult { status: proc_nonnegative_int_to_i32(99), stdout: "", stderr: "" },
    };

    let empty_env: Vec[String] = vec.new_vec();
    let timeout_opts = RunOptions {
        stdin: "",
        cwd: "",
        env: empty_env,
        timeout_ms: 50,
    };
    let run_with_timeout = match run_with("ping -n 6 127.0.0.1 >NUL", timeout_opts) {
        Ok(value) => if value.status == 124 { 1 } else { 0 },
        Err(_) => 0,
    };
    let run_timeout_ok = match run_timeout("ping -n 6 127.0.0.1 >NUL", 50) {
        Ok(value) => if value.status == 124 { 1 } else { 0 },
        Err(_) => 0,
    };
    let invalid_timeout = match run_timeout("echo ok", -1) {
        Ok(_) => 0,
        Err(err) => invalid_input_code(err),
    };

    let run_with_ok = if out.status == 0 &&
        string.contains(out.stdout, "cwd-data|from-env|from-stdin") &&
        len(out.stderr) == 0 {
        1
    } else {
        0
    };

    if run_with_ok + run_with_timeout + run_timeout_ok + invalid_timeout == 4 {
        42
    } else {
        0
    }
}
"#;
    let (code, stdout, stderr) = compile_and_run_with_setup(src, |root| {
        let workdir = root.join("workdir");
        fs::create_dir_all(&workdir).expect("mkdir workdir");
        fs::write(workdir.join("input.txt"), "cwd-data").expect("write workdir input");
    });
    assert_eq!(code, 42, "stderr={stderr}");
    assert_eq!(stdout, "");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_proc_pipe_chain_is_running_and_current_pid() {
    let src = r#"
import std.proc;
import std.string;
import std.vec;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn invalid_input_code(err: ProcError) -> Int {
    match err {
        InvalidInput => 1,
        _ => 0,
    }
}

fn main() -> Int effects { proc, env } capabilities { proc, env  } {
    let mut stages: Vec[String] = vec.vec_of("echo beta");
    stages = vec.push(stages, "tr a-z A-Z");
    let chained = match pipe_chain(stages) {
        Ok(out) => out,
        Err(_) => ProcResult { status: proc_nonnegative_int_to_i32(99), stdout: "", stderr: "" },
    };
    let chain_ok = if chained.status == proc_nonnegative_int_to_i32(0) && string.contains(chained.stdout, "BETA") { 1 } else { 0 };
    let empty_stages: Vec[String] = vec.new_vec();
    let empty_chain = match pipe_chain(empty_stages) {
        Ok(_) => 0,
        Err(err) => invalid_input_code(err),
    };

    let handle = match spawn("sleep 1") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let running_now = match is_running(handle) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let waited = match wait(handle) {
        Ok(code) => if code == 0 { 1 } else { 0 },
        Err(_) => 0,
    };
    let running_after_wait = match is_running(handle) {
        Ok(_) => 0,
        Err(err) => match err {
            UnknownProcess => 1,
            _ => 0,
        },
    };
    let pid_ok = match current_pid() {
        Ok(pid) => if pid > 1 { 1 } else { 0 },
        Err(_) => 0,
    };

    if chain_ok + empty_chain + running_now + waited + running_after_wait + pid_ok == 6 {
        42
    } else {
        0
    }
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 42, "stderr={stderr}");
    assert_eq!(stdout, "");
}

#[cfg(target_os = "windows")]
#[test]
fn exec_proc_pipe_chain_is_running_and_current_pid() {
    let src = r#"
import std.proc;
import std.string;
import std.vec;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn invalid_input_code(err: ProcError) -> Int {
    match err {
        InvalidInput => 1,
        _ => 0,
    }
}

fn main() -> Int effects { proc, env } capabilities { proc, env  } {
    let mut stages: Vec[String] = vec.vec_of("echo beta");
    stages = vec.push(stages, "findstr beta");
    let chained = match pipe_chain(stages) {
        Ok(out) => out,
        Err(_) => ProcResult { status: proc_nonnegative_int_to_i32(99), stdout: "", stderr: "" },
    };
    let chain_ok = if chained.status == proc_nonnegative_int_to_i32(0) && string.contains(chained.stdout, "beta") { 1 } else { 0 };
    let empty_stages: Vec[String] = vec.new_vec();
    let empty_chain = match pipe_chain(empty_stages) {
        Ok(_) => 0,
        Err(err) => invalid_input_code(err),
    };

    let handle = match spawn("ping -n 6 127.0.0.1 >NUL") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let running_now = match is_running(handle) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let waited = match wait(handle) {
        Ok(code) => if code == 0 { 1 } else { 0 },
        Err(_) => 0,
    };
    let running_after_wait = match is_running(handle) {
        Ok(_) => 0,
        Err(err) => match err {
            UnknownProcess => 1,
            _ => 0,
        },
    };
    let pid_ok = match current_pid() {
        Ok(pid) => if pid > 1 { 1 } else { 0 },
        Err(_) => 0,
    };

    if chain_ok + empty_chain + running_now + waited + running_after_wait + pid_ok == 6 {
        42
    } else {
        0
    }
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 42, "stderr={stderr}");
    assert_eq!(stdout, "");
}

#[test]
fn exec_net_tcp_loopback_echo() {
    let src = r#"
import std.io;
import std.net;
import std.string;
import std.bytes;

fn main() -> Int effects { io, net } capabilities { io, net  } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let listen_addr = match tcp_local_addr(listener) {
        Ok(addr) => addr,
        Err(_) => "",
    };
    let client = match tcp_connect(listen_addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match tcp_accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let sent = match tcp_send(client, bytes.from_string("ping")) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let received = match tcp_recv(server, 16, 1000) {
        Ok(text) => text,
        Err(_) => bytes.empty(),
    };
    let closed_client = match tcp_close(client) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_server = match tcp_close(server) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_listener = match tcp_close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if sent == 4 && bytes.byte_len(received) == 4 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_typed_endpoint_wrappers_and_compat_boundaries_are_deterministic() {
    let src = r#"
import std.io;
import std.net;
import std.bytes;
import std.string;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn is_invalid_input(err: NetError) -> Bool {
    match err {
        InvalidInput => true,
        _ => false,
    }
}

fn main() -> Int effects { io, net } capabilities { io, net  } {
    let listener = match tcp_listen_host_port("127.0.0.1", 0u16) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let typed_local: (String, NetPortU16) = match tcp_local_endpoint(listener) {
        Ok(endpoint) => endpoint,
        Err(_) => ("", 0u16),
    };
    let (typed_host, typed_port) = typed_local;
    let typed_host_ok = bool_to_int(string.len(typed_host) > 0);
    let typed_port_ok = bool_to_int(typed_port <= 65535u16);

    let typed_client = match tcp_connect_host_port(typed_host, typed_port, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let typed_server = match tcp_accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let typed_send_ok = match tcp_send(typed_client, bytes.from_string("hp")) {
        Ok(sent) => bool_to_int(sent == 2),
        Err(_) => 0,
    };
    let typed_recv_ok = match tcp_recv(typed_server, 16, 1000) {
        Ok(payload) => bool_to_int(bytes.compare_bytes(payload, bytes.from_string("hp")) == 0),
        Err(_) => 0,
    };
    let typed_close =
        (match tcp_close(typed_client) {
            Ok(_) => 1,
            Err(_) => 0,
        }) +
        (match tcp_close(typed_server) {
            Ok(_) => 1,
            Err(_) => 0,
        });

    let legacy_addr = net_format_host_port("127.0.0.1", typed_port);
    let compat_client = match tcp_connect(legacy_addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let compat_server = match tcp_accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let compat_send_ok = match tcp_send(compat_client, bytes.from_string("lg")) {
        Ok(sent) => bool_to_int(sent == 2),
        Err(_) => 0,
    };
    let compat_recv_ok = match tcp_recv(compat_server, 16, 1000) {
        Ok(payload) => bool_to_int(bytes.compare_bytes(payload, bytes.from_string("lg")) == 0),
        Err(_) => 0,
    };
    let compat_close =
        (match tcp_close(compat_client) {
            Ok(_) => 1,
            Err(_) => 0,
        }) +
        (match tcp_close(compat_server) {
            Ok(_) => 1,
            Err(_) => 0,
        });

    let parse_malformed = match net_parse_host_port("127.0.0.1") {
        Err(err) => bool_to_int(is_invalid_input(err)),
        _ => 0,
    };
    let parse_out_of_range = match net_parse_host_port("127.0.0.1:70000") {
        Err(err) => bool_to_int(is_invalid_input(err)),
        _ => 0,
    };
    let parse_negative = match net_parse_host_port("127.0.0.1:-1") {
        Err(err) => bool_to_int(is_invalid_input(err)),
        _ => 0,
    };
    let parse_roundtrip = match net_parse_host_port(legacy_addr) {
        Ok(endpoint) => bool_to_int(endpoint.1 == typed_port),
        Err(_) => 0,
    };

    let udp = match udp_bind_host_port("127.0.0.1", 0u16) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let udp_close_ok = match udp_close(udp) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    let listener_close = match tcp_close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    let score =
        typed_host_ok +
        typed_port_ok +
        typed_send_ok +
        typed_recv_ok +
        typed_close +
        compat_send_ok +
        compat_recv_ok +
        compat_close +
        parse_malformed +
        parse_out_of_range +
        parse_negative +
        parse_roundtrip +
        udp_close_ok +
        listener_close;

    if score == 16 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_tcp_socket_tuning_roundtrip_and_negative_paths() {
    let src = r#"
import std.io;
import std.net;
import std.string;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn is_invalid_input(err: NetError) -> Bool {
    match err {
        InvalidInput => true,
        _ => false,
    }
}

fn is_io(err: NetError) -> Bool {
    match err {
        Io => true,
        _ => false,
    }
}

fn is_io_or_invalid(err: NetError) -> Bool {
    match err {
        Io => true,
        InvalidInput => true,
        ConnectionClosed => true,
        _ => false,
    }
}

fn option_bool_score(set_result: Result[Bool, NetError], get_result: Result[Bool, NetError], expected: Bool) -> Int {
    match set_result {
        Ok(done) => if done {
            match get_result {
                Ok(value) => bool_to_int(value == expected),
                Err(err) => bool_to_int(is_io_or_invalid(err)),
            }
        } else {
            0
        },
        Err(err) => bool_to_int(is_io(err)),
    }
}

fn option_int_score(set_result: Result[Bool, NetError], get_result: Result[Int, NetError]) -> Int {
    match set_result {
        Ok(done) => if done {
            match get_result {
                Ok(size_bytes) => bool_to_int(size_bytes > 0),
                Err(err) => bool_to_int(is_io_or_invalid(err)),
            }
        } else {
            0
        },
        Err(err) => bool_to_int(is_io(err)),
    }
}

fn option_u32_score(set_result: Result[Bool, NetError], get_result: Result[UInt32, NetError]) -> Int {
    match set_result {
        Ok(done) => if done {
            match get_result {
                Ok(size_bytes) => bool_to_int(size_bytes > 0u32),
                Err(err) => bool_to_int(is_io_or_invalid(err)),
            }
        } else {
            0
        },
        Err(err) => bool_to_int(is_io(err)),
    }
}

fn option_addr_score(result: Result[String, NetError]) -> Int {
    match result {
        Ok(addr) => bool_to_int(len(addr) > 0),
        Err(err) => bool_to_int(is_io(err)),
    }
}

fn main() -> Int effects { io, net } capabilities { io, net } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let listen_addr = match tcp_local_addr(listener) {
        Ok(addr) => addr,
        Err(_) => "",
    };
    let client = match tcp_connect(listen_addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match tcp_accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let client_stream = tcp_stream(client);
    let server_stream = tcp_stream(server);

    let nodelay_ok = option_bool_score(
        tcp_stream_set_nodelay(client_stream, true),
        tcp_stream_get_nodelay(client_stream),
        true,
    );
    let keepalive_ok = option_bool_score(
        tcp_set_keepalive(server, true),
        tcp_get_keepalive(server),
        true,
    );
    let keepalive_idle_ok = option_int_score(
        tcp_set_keepalive_idle_secs(server, 30),
        tcp_get_keepalive_idle_secs(server),
    );
    let keepalive_idle_u32_ok = option_u32_score(
        tcp_set_keepalive_idle_secs_u32(server, 30u32),
        tcp_get_keepalive_idle_secs_u32(server),
    );
    let keepalive_interval_ok = option_int_score(
        tcp_stream_set_keepalive_interval_secs(client_stream, 10),
        tcp_stream_get_keepalive_interval_secs(client_stream),
    );
    let keepalive_interval_u32_ok = option_u32_score(
        tcp_stream_set_keepalive_interval_secs_u32(client_stream, 10u32),
        tcp_stream_get_keepalive_interval_secs_u32(client_stream),
    );
    let keepalive_count_ok = option_int_score(
        tcp_set_keepalive_count(client, 5),
        tcp_get_keepalive_count(client),
    );
    let keepalive_count_u32_ok = option_u32_score(
        tcp_set_keepalive_count_u32(client, 5u32),
        tcp_get_keepalive_count_u32(client),
    );
    let send_buffer_ok = option_int_score(
        tcp_set_send_buffer_size(client, 8192),
        tcp_get_send_buffer_size(client),
    );
    let send_buffer_u32_ok = option_u32_score(
        tcp_set_send_buffer_size_u32(client, 8192u32),
        tcp_get_send_buffer_size_u32(client),
    );
    let recv_buffer_ok = option_int_score(
        tcp_stream_set_recv_buffer_size(server_stream, 8192),
        tcp_stream_get_recv_buffer_size(server_stream),
    );
    let recv_buffer_u32_ok = option_u32_score(
        tcp_stream_set_recv_buffer_size_u32(server_stream, 8192u32),
        tcp_stream_get_recv_buffer_size_u32(server_stream),
    );
    let peer_addr_ok = option_addr_score(tcp_stream_peer_addr(client_stream));

    let shutdown_write_ok = match tcp_stream_shutdown_write(client_stream) {
        Ok(done) => bool_to_int(done),
        Err(err) => bool_to_int(is_io_or_invalid(err)),
    };
    let shutdown_read_ok = match tcp_stream_shutdown_read(server_stream) {
        Ok(done) => bool_to_int(done),
        Err(err) => bool_to_int(is_io_or_invalid(err)),
    };
    let shutdown_both_ok = match tcp_shutdown(server) {
        Ok(done) => bool_to_int(done),
        Err(err) => bool_to_int(is_io_or_invalid(err)),
    };

    let invalid_size_ok = match tcp_set_send_buffer_size(client, 0) {
        Err(err) => bool_to_int(is_invalid_input(err)),
        _ => 0,
    };
    let invalid_keepalive_ok = match tcp_set_keepalive_idle_secs(client, 0) {
        Err(err) => bool_to_int(is_invalid_input(err)),
        _ => 0,
    };
    let invalid_keepalive_u32_range_ok = match tcp_set_keepalive_idle_secs_u32(client, 2147483648u32) {
        Err(err) => bool_to_int(is_invalid_input(err)),
        _ => 0,
    };
    let invalid_send_buffer_u32_range_ok = match tcp_set_send_buffer_size_u32(client, 2147483648u32) {
        Err(err) => bool_to_int(is_invalid_input(err)),
        _ => 0,
    };
    let wrong_handle_ok = match tcp_set_nodelay(listener, true) {
        Err(err) => bool_to_int(is_invalid_input(err)),
        _ => 0,
    };
    let wrong_peer_ok = match tcp_peer_addr(listener) {
        Err(err) => bool_to_int(is_invalid_input(err)),
        _ => 0,
    };
    let wrong_shutdown_ok = match tcp_shutdown(listener) {
        Err(err) => bool_to_int(is_invalid_input(err)),
        _ => 0,
    };

    let close_ok = match tcp_close(client) {
        Ok(_) => 1,
        Err(_) => 0,
    } + match tcp_close(server) {
        Ok(_) => 1,
        Err(_) => 0,
    } + match tcp_close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if nodelay_ok
        + keepalive_ok
        + keepalive_idle_ok
        + keepalive_idle_u32_ok
        + keepalive_interval_ok
        + keepalive_interval_u32_ok
        + keepalive_count_ok
        + keepalive_count_u32_ok
        + send_buffer_ok
        + send_buffer_u32_ok
        + recv_buffer_ok
        + recv_buffer_u32_ok
        + peer_addr_ok
        + shutdown_write_ok
        + shutdown_read_ok
        + shutdown_both_ok
        + invalid_size_ok
        + invalid_keepalive_ok
        + invalid_keepalive_u32_range_ok
        + invalid_send_buffer_u32_range_ok
        + wrong_handle_ok
        + wrong_peer_ok
        + wrong_shutdown_ok
        + close_ok
        == 26
    {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_tcp_stream_exact_and_framed_reads_with_deadlines() {
    let src = r#"
import std.io;
import std.net;
import std.time;
import std.bytes;
import std.vec;
import std.buffer;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn is_timeout(err: NetError) -> Bool {
    match err {
        Timeout => true,
        _ => false,
    }
}

fn is_invalid_input(err: NetError) -> Bool {
    match err {
        InvalidInput => true,
        _ => false,
    }
}

fn frame_header_u32(len: UInt32) -> Bytes {
    let header = new_buffer(4);
    let wrote = buf_write_u32_be(header, len);
    let _status = wrote;
    buffer_to_bytes(header)
}

struct TestTcpStream {
    handle: Int,
}

fn test_tcp_stream(handle: Int) -> TestTcpStream {
    TestTcpStream { handle: handle }
}

fn test_tcp_stream_send(stream: TestTcpStream, payload: Bytes) -> Result[Int, NetError] effects { net } capabilities { net } {
    tcp_send(stream.handle, payload)
}

fn test_tcp_stream_close(stream: TestTcpStream) -> Result[Bool, NetError] effects { net } capabilities { net } {
    tcp_close(stream.handle)
}

fn test_tcp_stream_recv(stream: TestTcpStream, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net } capabilities { net } {
    tcp_recv(stream.handle, max_bytes, timeout_ms)
}

fn test_net_io_error() -> NetError {
    Io()
}

struct TestTcpRecvStep {
    failed: Bool,
    failure: NetError,
    chunk: Bytes,
}

fn test_tcp_stream_recv_step(next: Result[Bytes, NetError]) -> TestTcpRecvStep {
    match next {
        Err(err) => TestTcpRecvStep {
            failed: true,
            failure: err,
            chunk: bytes.empty(),
        },
        Ok(chunk) => TestTcpRecvStep {
            failed: false,
            failure: test_net_io_error(),
            chunk: chunk,
        },
    }
}

fn test_tcp_stream_frame_len_be(header: Bytes) -> Result[Int, NetError] {
    tcp_stream_frame_len_be(header)
}

fn test_tcp_stream_recv_framed_payload(stream: TestTcpStream, frame_header: Bytes, max_frame_bytes: Int, deadline_ms: Int) -> Result[Bytes, NetError] effects { net, time } capabilities { net, time } {
    let frame_len = test_tcp_stream_frame_len_be(frame_header);
    match frame_len {
        Err(err) => Err(err),
        Ok(payload_len) => if payload_len < 0 || payload_len > max_frame_bytes {
            Err(InvalidInput())
        } else {
            test_tcp_stream_recv_exact_deadline(stream, payload_len, deadline_ms)
        },
    }
}

fn test_tcp_stream_recv_exact_deadline(stream: TestTcpStream, expected_bytes: Int, deadline_ms: Int) -> Result[Bytes, NetError] effects { net, time } capabilities { net, time } {
    if expected_bytes < 0 {
        Err(InvalidInput())
    } else {
        let mut remaining = expected_bytes;
        let mut out = bytes.empty();
        let mut failed = false;
        let mut failure: NetError = test_net_io_error();
        while remaining > 0 && !failed {
            let timeout_ms = remaining_ms(deadline_ms);
            if timeout_ms <= 0 {
                failed = true;
                failure = Timeout();
            } else {
                let next = test_tcp_stream_recv(stream, remaining, timeout_ms);
                let step = test_tcp_stream_recv_step(next);
                if step.failed {
                    failed = true;
                    failure = step.failure;
                } else {
                    let read_count = bytes.byte_len(step.chunk);
                    if read_count <= 0 {
                        failed = true;
                        failure = test_net_io_error();
                    } else {
                        out = bytes.concat(out, step.chunk);
                        remaining = remaining - read_count;
                    }
                }
            }
        };
        if failed {
            Err(failure)
        } else {
            Ok(out)
        }
    }
}

fn test_tcp_stream_recv_exact(stream: TestTcpStream, expected_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net, time } capabilities { net, time } {
    let deadline_ms = deadline_after_ms(timeout_ms);
    test_tcp_stream_recv_exact_deadline(stream, expected_bytes, deadline_ms)
}

fn test_tcp_stream_recv_framed_deadline(stream: TestTcpStream, max_frame_bytes: Int, deadline_ms: Int) -> Result[Bytes, NetError] effects { net, time } capabilities { net, time } {
    if max_frame_bytes < 0 {
        Err(InvalidInput())
    } else {
        let header = test_tcp_stream_recv_exact_deadline(stream, 4, deadline_ms);
        match header {
            Err(err) => Err(err),
            Ok(frame_header) => test_tcp_stream_recv_framed_payload(stream, frame_header, max_frame_bytes, deadline_ms),
        }
    }
}

fn test_tcp_stream_recv_framed(stream: TestTcpStream, max_frame_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net, time } capabilities { net, time } {
    let deadline_ms = deadline_after_ms(timeout_ms);
    test_tcp_stream_recv_framed_deadline(stream, max_frame_bytes, deadline_ms)
}
fn main() -> Int effects { io, net, time } capabilities { io, net, time } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let listen_addr = match tcp_local_addr(listener) {
        Ok(addr) => addr,
        Err(_) => "",
    };
    let client = match tcp_connect(listen_addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match tcp_accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let client_stream = test_tcp_stream(client);
    let server_stream = test_tcp_stream(server);

    let timeout_ok = match test_tcp_stream_recv_exact_deadline(server_stream, 1, deadline_after_ms(0)) {
        Err(err) => is_timeout(err),
        _ => false,
    };

    let sent_exact_a = match test_tcp_stream_send(client_stream, bytes.from_string("abc")) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let sent_exact_b = match test_tcp_stream_send(client_stream, bytes.from_string("defg")) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let exact_payload = match test_tcp_stream_recv_exact(server_stream, 7, 2000) {
        Ok(v) => v,
        Err(_) => bytes.empty(),
    };
    let exact_ok = bytes.compare_bytes(exact_payload, bytes.from_string("abcdefg")) == 0;

    let frame_payload = bytes.from_string("frame");
    let frame_header_bytes = frame_header_u32(5u32);
    let sent_header = match test_tcp_stream_send(client_stream, frame_header_bytes) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let sent_frame_a = match test_tcp_stream_send(client_stream, bytes.from_string("fr")) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let sent_frame_b = match test_tcp_stream_send(client_stream, bytes.from_string("ame")) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let framed_payload = match test_tcp_stream_recv_framed_deadline(server_stream, 64, deadline_after_ms(2000)) {
        Ok(v) => v,
        Err(_) => bytes.empty(),
    };
    let framed_ok = bytes.compare_bytes(framed_payload, frame_payload) == 0;

    let oversized_header = frame_header_u32(9u32);
    let sent_oversized_header = match test_tcp_stream_send(client_stream, oversized_header) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let oversized_ok = match test_tcp_stream_recv_framed(server_stream, 8, 2000) {
        Err(err) => is_invalid_input(err),
        _ => false,
    };

    let closed_client = match test_tcp_stream_close(client_stream) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_server = match test_tcp_stream_close(server_stream) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_listener = match tcp_close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if timeout_ok &&
        sent_exact_a + sent_exact_b == 7 &&
        exact_ok &&
        sent_header == 4 &&
        sent_frame_a + sent_frame_b == 5 &&
        framed_ok &&
        sent_oversized_header == 4 &&
        oversized_ok &&
        closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_tls_fixed_width_frame_len_wrappers_are_deterministic() {
    let src = r#"
import std.io;
import std.net;
import std.tls;
import std.buffer;
import std.bytes;

fn net_err_code(err: NetError) -> Int {
    match err {
        InvalidInput => 1,
        _ => 0,
    }
}

fn tls_err_code(err: TlsError) -> Int {
    match err {
        ProtocolError => 1,
        _ => 0,
    }
}

fn header_for_len(len: UInt32) -> Bytes {
    let buf = new_buffer(4);
    let _w = buf_write_u32_be(buf, len);
    buffer_to_bytes(buf)
}

fn main() -> Int effects { io } capabilities { io } {
    let header_zero = header_for_len(0u32);
    let header_255 = header_for_len(255u32);

    let net_zero = match tcp_stream_frame_len_be_u32(header_zero) {
        Ok(value) => if value == 0u32 { 1 } else { 0 },
        Err(_) => 0,
    };
    let net_255 = match tcp_stream_frame_len_be_u32(header_255) {
        Ok(value) => if value == 255u32 { 1 } else { 0 },
        Err(_) => 0,
    };
    let tls_zero = match tls_frame_len_be_u32(header_zero) {
        Ok(value) => if value == 0u32 { 1 } else { 0 },
        Err(_) => 0,
    };
    let tls_255 = match tls_frame_len_be_u32(header_255) {
        Ok(value) => if value == 255u32 { 1 } else { 0 },
        Err(_) => 0,
    };

    let invalid = bytes.from_string("abc");
    let net_invalid = match tcp_stream_frame_len_be_u32(invalid) {
        Err(err) => net_err_code(err),
        Ok(_) => 0,
    };
    let tls_invalid = match tls_frame_len_be_u32(invalid) {
        Err(err) => tls_err_code(err),
        Ok(_) => 0,
    };

    if net_zero + net_255 + tls_zero + tls_255 + net_invalid + tls_invalid == 6 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_fixed_width_capacity_and_index_wrappers_work() {
    let src = r#"
import std.io;
import std.concurrent;
import std.vec;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn unwrap_channel(v: Result[IntChannel, ConcurrencyError]) -> IntChannel {
    match v {
        Ok(ch) => ch,
        Err(_) => IntChannel { handle: 0 },
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env } {
    let ch0 = unwrap_channel(channel_int_u32(1u32));
    let sent = match send_int(ch0, 255, 100) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let received = match recv_int(ch0, 100) {
        Ok(v) => if v == 255 { 1 } else { 0 },
        Err(_) => 0,
    };

    let ch1 = unwrap_channel(buffered_channel_int_u32(1u32));
    let ch2 = unwrap_channel(channel_int_buffered_u32(1u32));
    let _seed = send_int(ch2, 32, 100);
    let select_ok = match select_recv_int_u32(ch1, ch2, 100) {
        Ok(selection) => if selection.channel_index == 1u32 && selection.value == 32 { 1 } else { 0 },
        Err(_) => 0,
    };

    let pair0: (Sender[Int], Receiver[Int]) = channel();
    let pair1: (Sender[Int], Receiver[Int]) = channel();
    let tx0: Sender[Int] = pair0.0;
    let rx0: Receiver[Int] = pair0.1;
    let tx1: Sender[Int] = pair1.0;
    let rx1: Receiver[Int] = pair1.1;
    let _s0 = send(tx0, 11);
    let _s1 = send(tx1, 22);
    let mut receivers: Vec[Receiver[Int]] = vec.new_vec();
    receivers = vec.push(receivers, rx0);
    receivers = vec.push(receivers, rx1);
    let any0 = match select_any_u32(receivers, 20) {
        Ok(found) => if found.0 == 0u32 && found.1 == 11 { 1 } else { 0 },
        Err(_) => 0,
    };
    let any1 = match select_any_u32(receivers, 20) {
        Ok(found) => if found.0 == 1u32 && found.1 == 22 { 1 } else { 0 },
        Err(_) => 0,
    };

    let close_1 = match close_channel(ch1) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let close_2 = match close_channel(ch2) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let close_0 = match close_channel(ch0) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let close_tx = match close_sender(tx0) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let close_rx = match close_receiver(rx0) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };

    if sent + received + select_ok + any0 + any1 + close_0 + close_1 + close_2 + close_tx + close_rx == 10 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_u32_helper_surfaces_cover_conversion_and_readback_paths() {
    let src = r#"
import std.concurrent;
import std.io;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn is_invalid(err: ConcurrencyError) -> Int {
    match err {
        InvalidInput => 1,
        _ => 0,
    }
}

fn is_closed(err: ChannelError) -> Int {
    match err {
        Closed => 1,
        _ => 0,
    }
}

fn main() -> Int effects { io, concurrency, env } capabilities { io, concurrency, env } {
    let pair: (Sender[Int], Receiver[Int]) = channel();
    let tx: Sender[Int] = pair.0;
    let rx: Receiver[Int] = pair.1;

    let tx_handle_ok = match sender_handle_u32(tx) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let rx_handle_ok = match receiver_handle_u32(rx) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    let task_value: Task[Int] = Task { handle: 1 };
    let task_handle_ok = match task_handle_u32(task_value) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    let scope_value: Scope = Scope { handle: 1 };
    let scope_handle_ok = match scope_handle_u32(scope_value) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    let shared_arc: Arc[Int] = arc_new(11);
    let arc_handle_ok = match arc_handle_u32(shared_arc) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let arc_count_ok = match arc_strong_count_u32(shared_arc) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    let payload_id = match store_payload_for_channel_u32("hello") {
        Ok(id) => id,
        Err(_) => 0u32,
    };
    let payload_text_ok = match take_payload_string_u32(payload_id) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    let payload_pair: (Sender[Int], Receiver[Int]) = channel();
    let payload_rx: Receiver[Int] = payload_pair.1;
    let payload_id_int = match store_payload_for_channel_u32(77) {
        Ok(id) => id,
        Err(_) => 0u32,
    };
    let payload_int_ok = match take_payload_for_channel_u32(payload_id_int, payload_rx) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    let invalid_task_value: Task[Int] = Task { handle: -1 };
    let invalid_task = match task_handle_u32(invalid_task_value) {
        Err(err) => is_invalid(err),
        Ok(_) => 0,
    };
    let invalid_scope_value: Scope = Scope { handle: -1 };
    let invalid_scope = match scope_handle_u32(invalid_scope_value) {
        Err(err) => is_invalid(err),
        Ok(_) => 0,
    };
    let invalid_sender_value: Sender[Int] = Sender { handle: -1 };
    let invalid_sender = match sender_handle_u32(invalid_sender_value) {
        Err(err) => is_invalid(err),
        Ok(_) => 0,
    };
    let invalid_receiver_value: Receiver[Int] = Receiver { handle: -1 };
    let invalid_receiver = match receiver_handle_u32(invalid_receiver_value) {
        Err(err) => is_invalid(err),
        Ok(_) => 0,
    };
    let invalid_arc_value: Arc[Int] = Arc { handle: -1 };
    let invalid_arc = match arc_handle_u32(invalid_arc_value) {
        Err(err) => is_invalid(err),
        Ok(_) => 0,
    };

    let too_large: UInt32 = 2147483648u32;
    let invalid_payload_string = match take_payload_string_u32(too_large) {
        Err(err) => is_closed(err),
        Ok(_) => 0,
    };
    let invalid_pair: (Sender[Int], Receiver[Int]) = channel();
    let invalid_payload_rx: Receiver[Int] = invalid_pair.1;
    let invalid_payload_take = match take_payload_for_channel_u32(too_large, invalid_payload_rx) {
        Err(err) => is_closed(err),
        Ok(_) => 0,
    };

    let close_tx = match close_sender(tx) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let close_rx = match close_receiver(rx) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };

    if tx_handle_ok
        + rx_handle_ok
        + task_handle_ok
        + scope_handle_ok
        + arc_handle_ok
        + arc_count_ok
        + payload_text_ok
        + payload_int_ok
        + invalid_task
        + invalid_scope
        + invalid_sender
        + invalid_receiver
        + invalid_arc
        + invalid_payload_string
        + invalid_payload_take
        + close_tx
        + close_rx
        == 17
    {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_prod_t1_intrinsics_runtime_smoke() {
    let src = r#"
import std.bytes;
import std.concurrent;
import std.crypto;
import std.io;
import std.net;

fn bool_to_int(value: Bool) -> Int {
    if value { 1 } else { 0 }
}

fn digest_matches_hex(digest_hex: String, expected_hex: String) -> Bool {
    match hex_decode(digest_hex) {
        Ok(actual) => match hex_decode(expected_hex) {
            Ok(expected) => secure_eq(actual, expected),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

fn main() -> Int effects { io, net, concurrency, env, proc } capabilities { io, net, concurrency, env, proc  } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let listen_addr = match tcp_local_addr(listener) {
        Ok(addr) => addr,
        Err(_) => "",
    };
    let client = match tcp_connect(listen_addr, 1000) {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let server = match tcp_accept(listener, 1000) {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let send_ok = match tcp_send(client, bytes.from_string("ok")) {
        Ok(n) => bool_to_int(n == 2),
        Err(_) => 0,
    };
    let recv_ok = match tcp_recv(server, 8, 1000) {
        Ok(payload) => bool_to_int(bytes.byte_len(payload) == 2),
        Err(_) => 0,
    };
    let close_ok =
        match tcp_close(client) {
            Ok(done) => bool_to_int(done),
            Err(_) => 0,
        } +
        match tcp_close(server) {
            Ok(done) => bool_to_int(done),
            Err(_) => 0,
        } +
        match tcp_close(listener) {
            Ok(done) => bool_to_int(done),
            Err(_) => 0,
        };

    let sha_ok = bool_to_int(
        digest_matches_hex(
            sha256("hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        )
    );
    let task: Task[Int] = spawn_named("prod-t1-smoke", || -> Int { 42 });
    let join_ok = match join_value(task) {
        Ok(value) => bool_to_int(value == 42),
        Err(_) => 0,
    };

    if send_ok + recv_ok + close_ok + sha_ok + join_ok == 7 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_udp_and_dns_helpers() {
    let src = r#"
import std.io;
import std.net;
import std.string;
import std.bytes;
import std.vec;

fn main() -> Int effects { io, net } capabilities { io, net  } {
    let receiver = match udp_bind("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let receiver_addr = match udp_local_addr(receiver) {
        Ok(addr) => addr,
        Err(_) => "",
    };
    let sender = match udp_bind("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let sent = match udp_send_to(sender, receiver_addr, bytes.from_string("pong")) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let packet = match udp_recv_from(receiver, 64, 1000) {
        Ok(value) => value,
        Err(_) => UdpPacket { from: "", payload: bytes.empty() },
    };
    let closed_sender = match udp_close(sender) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_receiver = match udp_close(receiver) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    let lookup = match dns_lookup("localhost") {
        Ok(ip) => ip,
        Err(_) => "",
    };
    let lookup_all_count = match dns_lookup_all("localhost") {
        Ok(addrs) => vec.vec_len(addrs),
        Err(_) => 0,
    };
    let single_lookup_checked = match dns_lookup_all("127.0.0.1") {
        Ok(addrs) => if vec.vec_len(addrs) == 1 { 1 } else { 0 },
        Err(_) => 0,
    };
    let not_found_checked = match dns_lookup_all("aicore.invalid") {
        Ok(addrs) => if vec.vec_len(addrs) == 0 { 1 } else { 0 },
        Err(err) => match err {
            NotFound => 1,
            _ => 0,
        },
    };
    let invalid_input_checked = match dns_lookup_all("") {
        Err(err) => match err {
            InvalidInput => 1,
            _ => 0,
        },
        Ok(_) => 0,
    };
    let reverse_checked = match dns_reverse("127.0.0.1") {
        Ok(name) => if len(name) > 0 { 1 } else { 0 },
        Err(err) => match err {
            NotFound => 1,
            _ => 0,
        },
    };

    if sent == 4 && bytes.byte_len(packet.payload) == 4 && len(packet.from) > 0 &&
        len(lookup) > 0 && lookup_all_count > 0 &&
        single_lookup_checked == 1 && not_found_checked == 1 && invalid_input_checked == 1 &&
        reverse_checked == 1 && closed_sender + closed_receiver == 2 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_async_event_loop_multi_connection() {
    let src = r#"
import std.io;
import std.net;
import std.string;
import std.bytes;

fn main() -> Int effects { io, net, concurrency } capabilities { io, net, concurrency  } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(v) => v,
        Err(_) => "",
    };

    let c1 = match tcp_connect(addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let c2 = match tcp_connect(addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let accept1 = match async_accept_submit(listener, 1000) {
        Ok(op) => op,
        Err(_) => AsyncIntOp { handle: 0 },
    };
    let accept2 = match async_accept_submit(listener, 1000) {
        Ok(op) => op,
        Err(_) => AsyncIntOp { handle: 0 },
    };
    let s1 = match async_wait_int(accept1, 2000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let s2 = match async_wait_int(accept2, 2000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let recv1 = match async_tcp_recv_submit(s1, 32, 2000) {
        Ok(op) => op,
        Err(_) => AsyncStringOp { handle: 0 },
    };
    let recv2 = match async_tcp_recv_submit(s2, 32, 2000) {
        Ok(op) => op,
        Err(_) => AsyncStringOp { handle: 0 },
    };

    let sent1 = match tcp_send(c1, bytes.from_string("one")) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let sent2 = match tcp_send(c2, bytes.from_string("two")) {
        Ok(n) => n,
        Err(_) => 0,
    };

    let msg1 = match async_wait_string(recv1, 2000) {
        Ok(v) => v,
        Err(_) => bytes.empty(),
    };
    let msg2 = match async_wait_string(recv2, 2000) {
        Ok(v) => v,
        Err(_) => bytes.empty(),
    };

    let ack_submit1 = match async_tcp_send_submit(s1, bytes.from_string("ack")) {
        Ok(op) => op,
        Err(_) => AsyncIntOp { handle: 0 },
    };
    let ack_submit2 = match async_tcp_send_submit(s2, bytes.from_string("ack")) {
        Ok(op) => op,
        Err(_) => AsyncIntOp { handle: 0 },
    };
    let ack1 = match async_wait_int(ack_submit1, 2000) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let ack2 = match async_wait_int(ack_submit2, 2000) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let client_read1 = match tcp_recv(c1, 16, 2000) {
        Ok(v) => v,
        Err(_) => bytes.empty(),
    };
    let client_read2 = match tcp_recv(c2, 16, 2000) {
        Ok(v) => v,
        Err(_) => bytes.empty(),
    };

    let shutdown_ok = match async_shutdown() {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let close_count =
        (match tcp_close(c1) { Ok(_) => 1, Err(_) => 0 }) +
        (match tcp_close(c2) { Ok(_) => 1, Err(_) => 0 }) +
        (match tcp_close(s1) { Ok(_) => 1, Err(_) => 0 }) +
        (match tcp_close(s2) { Ok(_) => 1, Err(_) => 0 }) +
        (match tcp_close(listener) { Ok(_) => 1, Err(_) => 0 });

    let payload_ok = if bytes.byte_len(msg1) + bytes.byte_len(msg2) == 6 { 1 } else { 0 };
    let ack_ok = if ack1 + ack2 == 6 && bytes.byte_len(client_read1) == 3 && bytes.byte_len(client_read2) == 3 {
        1
    } else {
        0
    };

    if sent1 + sent2 == 6 && payload_ok == 1 && ack_ok == 1 && shutdown_ok == 1 && close_count == 5 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_async_queue_backpressure_and_shutdown() {
    let src = r#"
import std.io;
import std.net;

fn main() -> Int effects { io, net, concurrency } capabilities { io, net, concurrency  } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };

    let mut i = 0;
    let mut timeout_errs = 0;
    while i < 320 {
        let submitted = async_accept_submit(listener, 1);
        let err_inc = match submitted {
            Ok(_) => 0,
            Err(err) => match err {
                Timeout => 1,
                _ => 1,
            },
        };
        timeout_errs = timeout_errs + err_inc;
        i = i + 1;
    };

    let shutdown_ok = match async_shutdown() {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed = match tcp_close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if timeout_errs > 0 && shutdown_ok == 1 && closed == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_async_runtime_pressure_snapshots_are_typed_and_configurable() {
    let src = r#"
import std.io;
import std.net;
import std.tls;

fn bool_to_int(value: Bool) -> Int {
    if value { 1 } else { 0 }
}

fn net_code(err: NetError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        Refused => 3,
        Timeout => 4,
        AddressInUse => 5,
        InvalidInput => 6,
        Io => 7,
        ConnectionClosed => 8,
        Cancelled => 9,
    }
}

fn tls_code(err: TlsError) -> Int {
    match err {
        HandshakeFailed => 1,
        CertificateInvalid => 2,
        CertificateExpired => 3,
        HostnameMismatch => 4,
        ProtocolError => 5,
        ConnectionClosed => 6,
        Io => 7,
        Timeout => 8,
        Cancelled => 9,
    }
}

fn zero_pressure() -> AsyncRuntimePressure {
    AsyncRuntimePressure {
        active_ops: 0,
        queue_depth: 0,
        op_limit: 0,
        queue_limit: 0,
    }
}

fn main() -> Int effects { io, net, concurrency } capabilities { io, net, concurrency } {
    let baseline = match async_runtime_pressure() {
        Ok(value) => value,
        Err(_) => zero_pressure(),
    };
    let baseline_ok = if baseline.active_ops == 0 &&
        baseline.queue_depth == 0 &&
        baseline.op_limit == 3 &&
        baseline.queue_limit == 2 {
        1
    } else {
        0
    };

    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let op = match async_accept_submit(listener, 2000) {
        Ok(value) => value,
        Err(_) => AsyncIntOp { handle: 0 },
    };

    let inflight = match async_runtime_pressure() {
        Ok(value) => value,
        Err(_) => zero_pressure(),
    };
    let inflight_ok = if inflight.op_limit == 3 &&
        inflight.queue_limit == 2 &&
        inflight.active_ops + inflight.queue_depth >= 1 {
        1
    } else {
        0
    };

    let cancel_ok = match async_cancel_int(op) {
        Ok(value) => bool_to_int(value),
        Err(_) => 0,
    };
    let cancel_wait_ok = match async_wait_int(op, 2000) {
        Ok(_) => 0,
        Err(err) => if net_code(err) == 9 { 1 } else { 0 },
    };

    let after = match async_runtime_pressure() {
        Ok(value) => value,
        Err(_) => zero_pressure(),
    };
    let after_ok = if after.active_ops == 0 && after.queue_depth == 0 { 1 } else { 0 };

    let tls_ok = match tls_async_runtime_pressure() {
        Ok(value) => if value.queue_depth == 0 && value.queue_limit == 0 && value.op_limit == 4 {
            1
        } else {
            0
        },
        Err(err) => if tls_code(err) == 5 { 1 } else { 0 },
    };

    let shutdown_ok = match async_shutdown() {
        Ok(value) => bool_to_int(value),
        Err(_) => 0,
    };
    let close_ok = match tcp_close(listener) {
        Ok(value) => bool_to_int(value),
        Err(_) => 0,
    };

    let score = baseline_ok + inflight_ok + cancel_ok + cancel_wait_ok +
        after_ok + tls_ok + shutdown_ok + close_ok;
    if score == 8 {
        print_int(42);
    } else {
        print_int(score);
    };
    0
}
"#;

    let envs = [
        ("AIC_RT_LIMIT_NET_ASYNC_OPS", "3"),
        ("AIC_RT_LIMIT_NET_ASYNC_QUEUE", "2"),
        ("AIC_RT_LIMIT_TLS_ASYNC_OPS", "4"),
    ];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[test]
fn exec_net_timeout_and_invalid_input_errors_are_stable() {
    let src = r#"
import std.io;
import std.net;

fn err_code(err: NetError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        Refused => 3,
        Timeout => 4,
        AddressInUse => 5,
        InvalidInput => 6,
        ConnectionClosed => 8,
        Cancelled => 9,
        _ => 7,
    }
}

fn main() -> Int effects { io, net } capabilities { io, net  } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let timeout = match tcp_accept(listener, 0) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let invalid = match tcp_connect("", 10) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    tcp_close(listener);
    print_int(timeout * 10 + invalid);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "46\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_tcp_recv_timeout_then_peer_close_is_deterministic() {
    let src = r#"
import std.io;
import std.net;

fn err_code(err: NetError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        Refused => 3,
        Timeout => 4,
        AddressInUse => 5,
        InvalidInput => 6,
        ConnectionClosed => 8,
        Cancelled => 9,
        _ => 7,
    }
}

fn main() -> Int effects { io, net } capabilities { io, net  } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(value) => value,
        Err(_) => "",
    };

    let client = match tcp_connect(addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match tcp_accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let timeout_code = match tcp_recv(server, 16, 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    tcp_close(client);

    let close_code = match tcp_recv(server, 16, 1000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let close_count =
        (match tcp_close(server) {
            Ok(_) => 1,
            Err(_) => 0,
        }) +
        (match tcp_close(listener) {
            Ok(_) => 1,
            Err(_) => 0,
        });

    if timeout_code == 4 && close_code == 8 && close_count == 2 {
        print_int(42);
    } else {
        print_int(timeout_code * 10 + close_code);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_refused_and_address_in_use_errors_are_stable() {
    let src = r#"
import std.io;
import std.net;

fn err_code(err: NetError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        Refused => 3,
        Timeout => 4,
        AddressInUse => 5,
        InvalidInput => 6,
        _ => 7,
    }
}

fn main() -> Int effects { io, net } capabilities { io, net  } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(value) => value,
        Err(_) => "",
    };

    let bind_conflict = match tcp_listen(addr) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    tcp_close(listener);

    let refused = match tcp_connect(addr, 200) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    print_int(bind_conflict * 10 + refused);
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "53\n");
}

#[test]
fn exec_net_async_wait_negative_paths_are_stable() {
    let src = r#"
import std.io;
import std.net;
import std.string;
import std.bytes;

fn err_code(err: NetError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        Refused => 3,
        Timeout => 4,
        AddressInUse => 5,
        InvalidInput => 6,
        Io => 7,
        ConnectionClosed => 8,
        Cancelled => 9,
    }
}

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn main() -> Int effects { io, net, concurrency } capabilities { io, net, concurrency  } {
    let invalid_int_wait = match async_wait_int(AsyncIntOp { handle: 0 }, 10) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let invalid_string_wait = match async_wait_string(AsyncStringOp { handle: 0 }, 10) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(value) => value,
        Err(_) => "",
    };

    let cancel_op = match async_accept_submit(listener, 2000) {
        Ok(op) => op,
        Err(_) => AsyncIntOp { handle: 0 },
    };
    let cancel_handle = cancel_op.handle;
    let cancel_applied = match async_cancel_int(AsyncIntOp { handle: cancel_handle }) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let cancel_wait = match async_wait_int(AsyncIntOp { handle: cancel_handle }, 2000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let accept_op = match async_accept_submit(listener, 2000) {
        Ok(op) => op,
        Err(_) => AsyncIntOp { handle: 0 },
    };
    let accept_timeout = match async_wait_int(accept_op, 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let client = match tcp_connect(addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match async_wait_int(accept_op, 2000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let accept_rewait = match async_wait_int(accept_op, 2000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let recv_op = match async_tcp_recv_submit(server, 8, 2000) {
        Ok(op) => op,
        Err(_) => AsyncStringOp { handle: 0 },
    };
    let recv_timeout = match async_wait_string(recv_op, 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let sent_ok = match tcp_send(client, bytes.from_string("ok")) {
        Ok(written) => if written == 2 { 1 } else { 0 },
        Err(_) => 0,
    };
    let recv_ok = match async_wait_string(recv_op, 2000) {
        Ok(value) => if bytes.byte_len(value) == 2 { 1 } else { 0 },
        Err(_) => 0,
    };
    let recv_rewait = match async_wait_string(recv_op, 2000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let shutdown_ok = match async_shutdown() {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let close_count =
        (match tcp_close(client) {
            Ok(v) => bool_to_int(v),
            Err(_) => 0,
        }) +
        (match tcp_close(server) {
            Ok(v) => bool_to_int(v),
            Err(_) => 0,
        }) +
        (match tcp_close(listener) {
            Ok(v) => bool_to_int(v),
            Err(_) => 0,
        });

    let invalid_int_ok = if invalid_int_wait == 6 { 1 } else { 0 };
    let invalid_string_ok = if invalid_string_wait == 6 { 1 } else { 0 };
    let accept_timeout_ok = if accept_timeout == 4 { 1 } else { 0 };
    let accept_rewait_ok = if accept_rewait == 1 { 1 } else { 0 };
    let recv_timeout_ok = if recv_timeout == 4 { 1 } else { 0 };
    let recv_rewait_ok = if recv_rewait == 1 { 1 } else { 0 };
    let cancel_applied_ok = if cancel_applied == 1 { 1 } else { 0 };
    let cancel_wait_ok = if cancel_wait == 9 { 1 } else { 0 };
    let close_ok = if close_count == 3 { 1 } else { 0 };
    let score = invalid_int_ok + invalid_string_ok + cancel_applied_ok +
        cancel_wait_ok + accept_timeout_ok + accept_rewait_ok + recv_timeout_ok +
        sent_ok + recv_ok + recv_rewait_ok + shutdown_ok + close_ok;

    if score == 12 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_async_wait_many_paths_are_stable() {
    let src = r#"
import std.io;
import std.net;
import std.bytes;
import std.vec;

fn err_code(err: NetError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        Refused => 3,
        Timeout => 4,
        AddressInUse => 5,
        InvalidInput => 6,
        Io => 7,
        ConnectionClosed => 8,
        Cancelled => 9,
    }
}

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn main() -> Int effects { io, net, concurrency, time } capabilities { io, net, concurrency, time } {
    let listener1 = match tcp_listen("127.0.0.1:0") { Ok(h) => h, Err(_) => 0 };
    let listener2 = match tcp_listen("127.0.0.1:0") { Ok(h) => h, Err(_) => 0 };
    let listener3 = match tcp_listen("127.0.0.1:0") { Ok(h) => h, Err(_) => 0 };
    let addr3 = match tcp_local_addr(listener3) { Ok(v) => v, Err(_) => "" };

    let op_int1 = match async_accept_submit(listener1, 2000) { Ok(op) => op, Err(_) => AsyncIntOp { handle: 0 } };
    let op_int2 = match async_accept_submit(listener2, 2000) { Ok(op) => op, Err(_) => AsyncIntOp { handle: 0 } };
    let op_int3 = match async_accept_submit(listener3, 2000) { Ok(op) => op, Err(_) => AsyncIntOp { handle: 0 } };
    let client_int = match tcp_connect(addr3, 1000) { Ok(h) => h, Err(_) => 0 };

    let mut int_ops: Vec[AsyncIntOp] = vec.new_vec();
    int_ops = vec.push(int_ops, op_int1);
    int_ops = vec.push(int_ops, op_int2);
    int_ops = vec.push(int_ops, op_int3);

    let int_select_ok = match async_wait_many_int(int_ops, 1000) {
        Ok(sel) => if sel.index == 2 && sel.value > 0 {
            match tcp_close(sel.value) {
                Ok(closed) => bool_to_int(closed),
                Err(_) => 0,
            }
        } else {
            0
        },
        Err(_) => 0,
    };

    let cancel_int_1 = match async_cancel_int(op_int1) { Ok(v) => bool_to_int(v), Err(_) => 0 };
    let cancel_int_2 = match async_cancel_int(op_int2) { Ok(v) => bool_to_int(v), Err(_) => 0 };
    let ignored_wait_int_1 = async_wait_int(op_int1, 50);
    let ignored_wait_int_2 = async_wait_int(op_int2, 50);

    let listener_timeout = match tcp_listen("127.0.0.1:0") { Ok(h) => h, Err(_) => 0 };
    let op_timeout_int = match async_accept_submit(listener_timeout, 2000) {
        Ok(op) => op,
        Err(_) => AsyncIntOp { handle: 0 },
    };
    let timeout_int = match async_wait_many_int(vec.vec_of(op_timeout_int), 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let ignored_timeout_cancel = async_cancel_int(op_timeout_int);
    let ignored_timeout_wait = async_wait_int(op_timeout_int, 50);

    let listener_cancel = match tcp_listen("127.0.0.1:0") { Ok(h) => h, Err(_) => 0 };
    let op_cancel_int = match async_accept_submit(listener_cancel, 2000) {
        Ok(op) => op,
        Err(_) => AsyncIntOp { handle: 0 },
    };
    let cancel_int_applied = match async_cancel_int(op_cancel_int) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let cancel_int_code = match async_wait_many_int(vec.vec_of(op_cancel_int), 2000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let invalid_int_code = match async_wait_many_int(vec.vec_of(AsyncIntOp { handle: 0 }), 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let recv_listener = match tcp_listen("127.0.0.1:0") { Ok(h) => h, Err(_) => 0 };
    let recv_addr = match tcp_local_addr(recv_listener) { Ok(v) => v, Err(_) => "" };
    let recv_client1 = match tcp_connect(recv_addr, 1000) { Ok(h) => h, Err(_) => 0 };
    let recv_server1 = match tcp_accept(recv_listener, 1000) { Ok(h) => h, Err(_) => 0 };
    let recv_client2 = match tcp_connect(recv_addr, 1000) { Ok(h) => h, Err(_) => 0 };
    let recv_server2 = match tcp_accept(recv_listener, 1000) { Ok(h) => h, Err(_) => 0 };
    let recv_client3 = match tcp_connect(recv_addr, 1000) { Ok(h) => h, Err(_) => 0 };
    let recv_server3 = match tcp_accept(recv_listener, 1000) { Ok(h) => h, Err(_) => 0 };

    let recv_op1 = match async_tcp_recv_submit(recv_server1, 64, 2000) {
        Ok(op) => op,
        Err(_) => AsyncStringOp { handle: 0 },
    };
    let recv_op2 = match async_tcp_recv_submit(recv_server2, 64, 2000) {
        Ok(op) => op,
        Err(_) => AsyncStringOp { handle: 0 },
    };
    let recv_op3 = match async_tcp_recv_submit(recv_server3, 64, 2000) {
        Ok(op) => op,
        Err(_) => AsyncStringOp { handle: 0 },
    };

    let sent_wait_many = match tcp_send(recv_client3, bytes.from_string("xyz")) {
        Ok(sent) => if sent == 3 { 1 } else { 0 },
        Err(_) => 0,
    };

    let mut string_ops: Vec[AsyncStringOp] = vec.new_vec();
    string_ops = vec.push(string_ops, recv_op1);
    string_ops = vec.push(string_ops, recv_op2);
    string_ops = vec.push(string_ops, recv_op3);

    let string_select_ok = match async_wait_many_string(string_ops, 1000) {
        Ok(sel) => if sel.index == 2 && bytes.byte_len(sel.payload) == 3 { 1 } else { 0 },
        Err(_) => 0,
    };

    let cancel_string_1 = match async_cancel_string(recv_op1) { Ok(v) => bool_to_int(v), Err(_) => 0 };
    let cancel_string_2 = match async_cancel_string(recv_op2) { Ok(v) => bool_to_int(v), Err(_) => 0 };
    let ignored_wait_string_1 = async_wait_string(recv_op1, 50);
    let ignored_wait_string_2 = async_wait_string(recv_op2, 50);

    let timeout_string_op = match async_tcp_recv_submit(recv_server1, 64, 2000) {
        Ok(op) => op,
        Err(_) => AsyncStringOp { handle: 0 },
    };
    let timeout_string_code = match async_wait_many_string(vec.vec_of(timeout_string_op), 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let ignored_timeout_string_cancel = async_cancel_string(timeout_string_op);
    let ignored_timeout_string_wait = async_wait_string(timeout_string_op, 50);

    let cancel_string_op = match async_tcp_recv_submit(recv_server2, 64, 2000) {
        Ok(op) => op,
        Err(_) => AsyncStringOp { handle: 0 },
    };
    let cancel_string_applied = match async_cancel_string(cancel_string_op) {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };
    let cancel_string_code = match async_wait_many_string(vec.vec_of(cancel_string_op), 2000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let invalid_string_code = match async_wait_many_string(vec.vec_of(AsyncStringOp { handle: 0 }), 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let close_count =
        (match tcp_close(client_int) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(listener1) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(listener2) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(listener3) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(listener_timeout) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(listener_cancel) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(recv_client1) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(recv_client2) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(recv_client3) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(recv_server1) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(recv_server2) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(recv_server3) { Ok(v) => bool_to_int(v), Err(_) => 0 }) +
        (match tcp_close(recv_listener) { Ok(v) => bool_to_int(v), Err(_) => 0 });

    let shutdown_ok = match async_shutdown() {
        Ok(v) => bool_to_int(v),
        Err(_) => 0,
    };

    let score = int_select_ok +
        (if timeout_int == 4 { 1 } else { 0 }) +
        cancel_int_applied +
        (if cancel_int_code == 9 { 1 } else { 0 }) +
        (if invalid_int_code == 6 { 1 } else { 0 }) +
        sent_wait_many +
        string_select_ok +
        (if timeout_string_code == 4 { 1 } else { 0 }) +
        cancel_string_applied +
        (if cancel_string_code == 9 { 1 } else { 0 }) +
        (if invalid_string_code == 6 { 1 } else { 0 }) +
        (if close_count == 13 { 1 } else { 0 }) +
        shutdown_ok;

    if score == 13 && cancel_int_1 + cancel_int_2 == 2 && cancel_string_1 + cancel_string_2 == 2 {
        print_int(42);
    } else {
        print_int(score);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_async_lifecycle_controls_example_smoke() {
    let src = fs::read_to_string("examples/io/async_lifecycle_controls.aic")
        .expect("read examples/io/async_lifecycle_controls.aic");
    let (code, stdout, stderr) = compile_and_run(&src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_parses_request_and_emits_http11_response() {
    let src = r#"
import std.io;
import std.net;
import std.http_server;
import std.map;
import std.string;
import std.bytes;

fn request_matches(req: Request) -> Int effects { env } capabilities { env  } {
    let query_name = match map.get(req.query, "name") {
        Some(v) => v,
        None => "",
    };
    let host = match map.get(req.headers, "host") {
        Some(v) => v,
        None => "",
    };
    let trace = match map.get(req.headers, "x-trace") {
        Some(v) => v,
        None => "",
    };

    let method_ok = if string.contains(req.method, "GET") && len(req.method) == 3 { 1 } else { 0 };
    let path_ok = if string.contains(req.path, "/hello") && len(req.path) == 6 { 1 } else { 0 };
    let query_ok = if string.contains(query_name, "Kasun") && len(query_name) == 5 { 1 } else { 0 };
    let host_ok = if string.contains(host, "localhost") && len(host) == 9 { 1 } else { 0 };
    let trace_ok = if string.contains(trace, "abc") && len(trace) == 3 { 1 } else { 0 };
    let body_ok = if string.contains(req.body, "hello") && len(req.body) == 5 { 1 } else { 0 };

    if method_ok == 1 && path_ok == 1 && query_ok == 1 &&
       host_ok == 1 && trace_ok == 1 && body_ok == 1 {
        1
    } else {
        0
    }
}

fn main() -> Int effects { io, net, env } capabilities { io, net, env  } {
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

    let raw_req = "GET /hello?name=Kasun&lang=ai HTTP/1.1\nHost: localhost\nX-Trace: abc\nContent-Length: 5\n\nhello";
    let sent = match tcp_send(client, bytes.from_string(raw_req)) {
        Ok(n) => n,
        Err(_) => 0,
    };

    let parsed_ok = match read_request(server, 4096, 1000) {
        Ok(req) => request_matches(req),
        Err(_) => 0,
    };

    let wrote = match write_response(server, text_response(200, "ok")) {
        Ok(n) => n,
        Err(_) => 0,
    };
    let wire = match tcp_recv(client, 1024, 1000) {
        Ok(text) => text,
        Err(_) => bytes.empty(),
    };
    let wire_text = bytes.to_string_lossy(wire);
    let wire_ok = if string.contains(wire_text, "HTTP/1.1 200 OK") &&
        string.contains(wire_text, "content-length: 2") &&
        string.ends_with(wire_text, "ok") {
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

    if sent > 0 && wrote > 0 && parsed_ok == 1 && wire_ok == 1 &&
        closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_invalid_method_returns_typed_error() {
    let src = r#"
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

fn main() -> Int effects { io, net } capabilities { io, net  } {
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
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_fragmented_body_is_parsed_across_recv_boundaries() {
    let src = r#"
import std.io;
import std.net;
import std.http_server;
import std.string;
import std.env;
import std.map;
import std.fs;

fn read_env_or(key: String, fallback: String) -> String effects { env } capabilities { env } {
    match env.get(key) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn main() -> Int effects { io, net, env, fs } capabilities { io, net, env, fs } {
    let addr_file = read_env_or("AIC_HTTP_ADDR_FILE", "http_frag_addr.txt");
    let listener = match listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(v) => v,
        Err(_) => "",
    };
    let published = if len(addr) > 0 {
        match write_text(addr_file, addr) {
            Ok(_) => 1,
            Err(_) => 0,
        }
    } else {
        0
    };
    let server = match accept(listener, 15000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let parsed_ok = match read_request(server, 4096, 15000) {
        Ok(req) => if string.contains(req.method, "POST") && len(req.method) == 4 &&
            string.contains(req.path, "/frag") && len(req.path) == 5 &&
            string.contains(req.body, "hello") && len(req.body) == 5 &&
            (match map.get(req.headers, "content-length") {
                Some(v) => if string.contains(v, "5") && len(v) == 1 { 1 } else { 0 },
                None => 0,
            }) == 1 {
            1
        } else {
            0
        },
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

    if published == 1 && parsed_ok == 1 && closed_server + closed_listener == 2 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let addr_file_rel = "http_frag_addr.txt";
    let envs = [("AIC_HTTP_ADDR_FILE", addr_file_rel)];
    let client_thread: std::sync::Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let client_thread_setup = std::sync::Arc::clone(&client_thread);
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |root| {
            let addr_file = root.join(addr_file_rel);
            let handle = std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(15);
                let mut connect_addr: Option<String> = None;
                while Instant::now() < deadline {
                    match fs::read_to_string(&addr_file) {
                        Ok(value) => {
                            let trimmed = value.trim();
                            if !trimmed.is_empty() {
                                connect_addr = Some(trimmed.to_string());
                                break;
                            }
                        }
                        Err(_) => {}
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }

                let connect_addr = connect_addr.expect("server did not publish listen address");
                let mut connected: Option<std::net::TcpStream> = None;
                while Instant::now() < deadline {
                    match std::net::TcpStream::connect(&connect_addr) {
                        Ok(stream) => {
                            connected = Some(stream);
                            break;
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(20)),
                    }
                }

                let mut stream = connected.expect("client failed to connect to server");
                stream.set_nodelay(true).expect("set_nodelay");
                stream
                    .write_all(
                        b"POST /frag HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\nhe",
                    )
                    .expect("write first request chunk");
                std::thread::sleep(Duration::from_millis(60));
                stream
                    .write_all(b"llo")
                    .expect("write second request chunk");
                stream.flush().expect("flush request chunks");
                std::thread::sleep(Duration::from_millis(120));
            });
            *client_thread_setup.lock().expect("store client thread") = Some(handle);
        });
    if let Some(handle) = client_thread.lock().expect("take client thread").take() {
        handle.join().expect("client thread join");
    } else {
        panic!("client thread was not started");
    }
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_malformed_content_length_returns_invalid_header() {
    let src = r#"
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

fn main() -> Int effects { io, net } capabilities { io, net  } {
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

    tcp_send(client, bytes.from_string("POST /bad HTTP/1.1\nHost: localhost\nContent-Length: nope\n\n"));
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

    if code == 3 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_truncated_body_returns_connection_closed() {
    let src = r#"
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

fn main() -> Int effects { io, net } capabilities { io, net  } {
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

    tcp_send(client, bytes.from_string("POST /short HTTP/1.1\nHost: localhost\nContent-Length: 5\n\nhe"));
    let closed_client = match tcp_close(client) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let code = match read_request(server, 4096, 1000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let closed_server = match close(server) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let closed_listener = match close(listener) {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if code == 6 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_decodes_chunked_request_body() {
    let src = r#"
import std.io;
import std.net;
import std.http_server;
import std.string;
import std.map;
import std.bytes;

fn main() -> Int effects { io, net } capabilities { io, net  } {
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

    let raw_req = "POST /chunk HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
    let sent = match tcp_send(client, bytes.from_string(raw_req)) {
        Ok(v) => v,
        Err(_) => 0,
    };

    let parsed_ok = match read_request(server, 4096, 1000) {
        Ok(req) => if string.contains(req.method, "POST") &&
            string.contains(req.path, "/chunk") &&
            string.contains(req.body, "hello world") && len(req.body) == 11 &&
            (match map.get(req.headers, "transfer-encoding") {
                Some(v) => if string.contains(v, "chunked") { 1 } else { 0 },
                None => 0,
            }) == 1 {
            1
        } else {
            0
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

    if sent > 0 && parsed_ok == 1 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_malformed_chunk_framing_returns_invalid_request() {
    let src = r#"
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

fn main() -> Int effects { io, net } capabilities { io, net  } {
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

    tcp_send(client, bytes.from_string("POST /bad-chunk HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\n0\r\n\r\n"));
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

    if code == 1 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_transfer_encoding_and_content_length_conflict_is_invalid_header() {
    let src = r#"
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

fn main() -> Int effects { io, net } capabilities { io, net  } {
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

    tcp_send(client, bytes.from_string("POST /conflict HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\n5\r\nhello\r\n0\r\n\r\n"));
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

    if code == 3 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_guardrail_request_line_limit_returns_invalid_request() {
    let src = r#"
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

    tcp_send(client, bytes.from_string("GET /aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa HTTP/1.1\r\nHost: localhost\r\n\r\n"));
    let code = match read_request(server, 4096, 1000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let closed_client = match tcp_close(client) { Ok(_) => 1, Err(_) => 0 };
    let closed_server = match close(server) { Ok(_) => 1, Err(_) => 0 };
    let closed_listener = match close(listener) { Ok(_) => 1, Err(_) => 0 };

    if code == 1 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(code);
    };
    0
}
"#;

    let envs = [("AIC_RT_LIMIT_HTTP_REQUEST_LINE_BYTES", "24")];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_guardrail_header_bytes_limit_returns_invalid_header() {
    let src = r#"
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
    let listener = match listen("127.0.0.1:0") { Ok(h) => h, Err(_) => 0 };
    let addr = match tcp_local_addr(listener) { Ok(v) => v, Err(_) => "" };
    let client = match tcp_connect(addr, 1000) { Ok(h) => h, Err(_) => 0 };
    let server = match accept(listener, 1000) { Ok(h) => h, Err(_) => 0 };

    tcp_send(client, bytes.from_string("GET /h HTTP/1.1\r\nHost: localhost\r\nX-Long: 12345678901234567890\r\n\r\n"));
    let code = match read_request(server, 4096, 1000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let closed_client = match tcp_close(client) { Ok(_) => 1, Err(_) => 0 };
    let closed_server = match close(server) { Ok(_) => 1, Err(_) => 0 };
    let closed_listener = match close(listener) { Ok(_) => 1, Err(_) => 0 };

    if code == 3 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(code);
    };
    0
}
"#;

    let envs = [("AIC_RT_LIMIT_HTTP_HEADER_BYTES", "24")];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_guardrail_header_count_limit_returns_invalid_header() {
    let src = r#"
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
    let listener = match listen("127.0.0.1:0") { Ok(h) => h, Err(_) => 0 };
    let addr = match tcp_local_addr(listener) { Ok(v) => v, Err(_) => "" };
    let client = match tcp_connect(addr, 1000) { Ok(h) => h, Err(_) => 0 };
    let server = match accept(listener, 1000) { Ok(h) => h, Err(_) => 0 };

    tcp_send(client, bytes.from_string("GET /h HTTP/1.1\r\nHost: localhost\r\nX-A: 1\r\n\r\n"));
    let code = match read_request(server, 4096, 1000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let closed_client = match tcp_close(client) { Ok(_) => 1, Err(_) => 0 };
    let closed_server = match close(server) { Ok(_) => 1, Err(_) => 0 };
    let closed_listener = match close(listener) { Ok(_) => 1, Err(_) => 0 };

    if code == 3 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(code);
    };
    0
}
"#;

    let envs = [("AIC_RT_LIMIT_HTTP_HEADER_COUNT", "1")];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_guardrail_body_limit_returns_body_too_large() {
    let src = r#"
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
    let listener = match listen("127.0.0.1:0") { Ok(h) => h, Err(_) => 0 };
    let addr = match tcp_local_addr(listener) { Ok(v) => v, Err(_) => "" };
    let client = match tcp_connect(addr, 1000) { Ok(h) => h, Err(_) => 0 };
    let server = match accept(listener, 1000) { Ok(h) => h, Err(_) => 0 };

    tcp_send(client, bytes.from_string("POST /b HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\nhello"));
    let code = match read_request(server, 4096, 1000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let closed_client = match tcp_close(client) { Ok(_) => 1, Err(_) => 0 };
    let closed_server = match close(server) { Ok(_) => 1, Err(_) => 0 };
    let closed_listener = match close(listener) { Ok(_) => 1, Err(_) => 0 };

    if code == 7 && closed_client + closed_server + closed_listener == 3 {
        print_int(42);
    } else {
        print_int(code);
    };
    0
}
"#;

    let envs = [("AIC_RT_LIMIT_HTTP_BODY_BYTES", "4")];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_http_server_guardrail_idle_timeout_returns_timeout() {
    let src = r#"
import std.io;
import std.net;
import std.http_server;
import std.env;
import std.fs;

fn read_env_or(key: String, fallback: String) -> String effects { env } capabilities { env } {
    match env.get(key) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

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

fn main() -> Int effects { io, net, env, fs } capabilities { io, net, env, fs } {
    let addr_file = read_env_or("AIC_HTTP_ADDR_FILE", "http_guard_addr.txt");
    let listener = match listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let addr = match tcp_local_addr(listener) {
        Ok(v) => v,
        Err(_) => "",
    };
    let published = match write_text(addr_file, addr) {
        Ok(_) => 1,
        Err(_) => 0,
    };
    let server = match accept(listener, 15000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let code = match read_request(server, 4096, 15000) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let closed_server = match close(server) { Ok(_) => 1, Err(_) => 0 };
    let closed_listener = match close(listener) { Ok(_) => 1, Err(_) => 0 };

    if published == 1 && code == 5 && closed_server + closed_listener == 2 {
        print_int(42);
    } else {
        print_int(code);
    };
    0
}
"#;

    let addr_file_rel = "http_guard_addr.txt";
    let envs = [
        ("AIC_HTTP_ADDR_FILE", addr_file_rel),
        ("AIC_RT_LIMIT_HTTP_READ_IDLE_MS", "40"),
        ("AIC_RT_LIMIT_HTTP_READ_TOTAL_MS", "5000"),
    ];

    let client_thread: std::sync::Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let client_thread_setup = std::sync::Arc::clone(&client_thread);
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |root| {
            let addr_file = root.join(addr_file_rel);
            let handle = std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(15);
                let mut connect_addr: Option<String> = None;
                while Instant::now() < deadline {
                    if let Ok(value) = fs::read_to_string(&addr_file) {
                        let trimmed = value.trim();
                        if !trimmed.is_empty() {
                            connect_addr = Some(trimmed.to_string());
                            break;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }

                let connect_addr = connect_addr.expect("server did not publish listen address");
                let mut stream = loop {
                    if Instant::now() >= deadline {
                        panic!("client failed to connect to server");
                    }
                    match std::net::TcpStream::connect(&connect_addr) {
                        Ok(stream) => break stream,
                        Err(_) => std::thread::sleep(Duration::from_millis(20)),
                    }
                };

                stream.set_nodelay(true).expect("set_nodelay");
                stream
                    .write_all(
                        b"POST /idle HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\nhe",
                    )
                    .expect("write partial request");
                stream.flush().expect("flush partial request");
                std::thread::sleep(Duration::from_millis(200));
            });
            *client_thread_setup.lock().expect("store client thread") = Some(handle);
        });

    if let Some(handle) = client_thread.lock().expect("take client thread").take() {
        handle.join().expect("client thread join");
    } else {
        panic!("client thread was not started");
    }

    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[test]
fn exec_router_matches_paths_params_and_order() {
    let src = fs::read_to_string("examples/io/http_router.aic").expect("read router example");
    let (code, stdout, stderr) = compile_and_run(&src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_router_invalid_method_returns_typed_error() {
    let src = r#"
import std.io;
import std.router;

fn err_code(err: RouterError) -> Int {
    match err {
        InvalidPattern => 1,
        InvalidMethod => 2,
        Capacity => 3,
        Internal => 4,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let router = match new_router() {
        Ok(value) => value,
        Err(_) => Router { handle: 0 },
    };
    let code = match add(router, "G ET", "/health", 1) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    print_int(code);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "2\n");
}

#[test]
fn exec_router_precedence_static_param_wildcard_and_method_filtering() {
    let src = r#"
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

fn main() -> Int effects { io, env } capabilities { io, env } {
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
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_router_ambiguity_rejects_equal_precedence_overlap() {
    let src = r#"
import std.io;
import std.router;

fn err_code(err: RouterError) -> Int {
    match err {
        InvalidPattern => 1,
        InvalidMethod => 2,
        Capacity => 3,
        Internal => 4,
    }
}

fn main() -> Int effects { io } capabilities { io } {
    let router0 = match new_router() {
        Ok(value) => value,
        Err(_) => Router { handle: 0 },
    };
    let router1 = match add(router0, "GET", "/users/:id", 1) {
        Ok(value) => value,
        Err(_) => router0,
    };
    let first_conflict = match add(router1, "GET", "/users/:name", 2) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let second_conflict = match add(router1, "GET", "/users/:id", 3) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    if first_conflict == 1 && second_conflict == 1 {
        print_int(42);
    } else {
        print_int(first_conflict * 10 + second_conflict);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_router_ambiguity_rejects_precedence_violating_registration_order() {
    let src = r#"
import std.io;
import std.map;
import std.router;
import std.string;

fn err_code(err: RouterError) -> Int {
    match err {
        InvalidPattern => 1,
        InvalidMethod => 2,
        Capacity => 3,
        Internal => 4,
    }
}

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

fn route_param(router: Router, method: String, path: String, key: String) -> String {
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

fn main() -> Int effects { io, env } capabilities { io, env } {
    let router0 = match new_router() {
        Ok(value) => value,
        Err(_) => Router { handle: 0 },
    };
    let router1 = match add(router0, "GET", "/users/*", 10) {
        Ok(value) => value,
        Err(_) => router0,
    };
    let path_conflict = match add(router1, "GET", "/users/:id", 20) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let router2 = match new_router() {
        Ok(value) => value,
        Err(_) => Router { handle: 0 },
    };
    let router3 = match add(router2, "*", "/teams/:id", 30) {
        Ok(value) => value,
        Err(_) => router2,
    };
    let method_conflict = match add(router3, "GET", "/teams/:id", 40) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let fallback_ok = if route_id(router3, "GET", "/teams/7") == 30 &&
        str_eq(route_param(router3, "GET", "/teams/7", "id"), "7") == 1 {
        1
    } else {
        0
    };

    if path_conflict == 1 && method_conflict == 1 && fallback_ok == 1 {
        print_int(42);
    } else {
        print_int(path_conflict * 100 + method_conflict * 10 + fallback_ok);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_router_route_capacity_limit_override_is_enforced() {
    let src = r#"
import std.io;
import std.router;
import std.string;

fn err_code(err: RouterError) -> Int {
    match err {
        InvalidPattern => 1,
        InvalidMethod => 2,
        Capacity => 3,
        Internal => 4,
    }
}

fn add_many(router: Router, remaining: Int) -> Result[Router, RouterError] {
    if remaining == 0 {
        Ok(router)
    } else {
        let path = f"/cap/{int_to_string(remaining)}";
        match add(router, "GET", path, remaining) {
            Ok(next) => add_many(next, remaining - 1),
            Err(err) => Err(err),
        }
    }
}

fn main() -> Int effects { io } capabilities { io } {
    let router = match new_router() {
        Ok(value) => value,
        Err(_) => Router { handle: 0 },
    };
    let after_two = add_many(router, 2);
    let overflow_code = match after_two {
        Ok(next) => match add(next, "GET", "/overflow", 3) {
            Ok(_) => 0,
            Err(err) => err_code(err),
        },
        Err(err) => err_code(err) * 10,
    };
    if overflow_code == 3 {
        print_int(42);
    } else {
        print_int(overflow_code);
    };
    0
}
"#;

    let envs = [("AIC_RT_LIMIT_ROUTER_ROUTES", "2")];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[test]
fn exec_router_route_capacity_override_allows_above_legacy_cap() {
    let src = r#"
import std.io;
import std.router;
import std.string;

fn err_code(err: RouterError) -> Int {
    match err {
        InvalidPattern => 1,
        InvalidMethod => 2,
        Capacity => 3,
        Internal => 4,
    }
}

fn add_many(router: Router, remaining: Int) -> Result[Router, RouterError] {
    if remaining == 0 {
        Ok(router)
    } else {
        let path = f"/grow/{int_to_string(remaining)}";
        match add(router, "GET", path, remaining) {
            Ok(next) => add_many(next, remaining - 1),
            Err(err) => Err(err),
        }
    }
}

fn main() -> Int effects { io } capabilities { io } {
    let router = match new_router() {
        Ok(value) => value,
        Err(_) => Router { handle: 0 },
    };
    let code = match add_many(router, 130) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    if code == 0 {
        print_int(42);
    } else {
        print_int(code);
    };
    0
}
"#;

    let envs = [("AIC_RT_LIMIT_ROUTER_ROUTES", "192")];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[test]
fn exec_json_roundtrip_and_object_operations() {
    let src = r#"
import std.io;
import std.json;
import std.string;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn main() -> Int effects { io } capabilities { io  } {
    let parsed = match parse("{\"name\":\"svc\",\"enabled\":true,\"retries\":3,\"nested\":{\"mode\":\"safe\"},\"items\":[1,2]}") {
        Ok(value) => value,
        Err(_) => object_empty(),
    };

    let name_len = match object_get(parsed, "name") {
        Ok(maybe) => match maybe {
            Some(value) => match decode_string(value) {
                Ok(text) => len(text),
                Err(_) => 0,
            },
            None => 0,
        },
        Err(_) => 0,
    };
    let retries = match object_get(parsed, "retries") {
        Ok(maybe) => match maybe {
            Some(value) => match decode_int(value) {
                Ok(v) => v,
                Err(_) => 0,
            },
            None => 0,
        },
        Err(_) => 0,
    };
    let enabled = match object_get(parsed, "enabled") {
        Ok(maybe) => match maybe {
            Some(value) => match decode_bool(value) {
                Ok(v) => bool_to_int(v),
                Err(_) => 0,
            },
            None => 0,
        },
        Err(_) => 0,
    };

    let updated = match object_set(parsed, "timeout_ms", encode_int(250)) {
        Ok(value) => value,
        Err(_) => object_empty(),
    };
    let timeout = match object_get(updated, "timeout_ms") {
        Ok(maybe) => match maybe {
            Some(value) => match decode_int(value) {
                Ok(v) => v,
                Err(_) => 0,
            },
            None => 0,
        },
        Err(_) => 0,
    };

    let text = match stringify(updated) {
        Ok(s) => s,
        Err(_) => "",
    };
    let reparsed_kind = match parse(text) {
        Ok(value) => match kind(value) {
            ObjectValue => 1,
            _ => 0,
        },
        Err(_) => 0,
    };

    if name_len == 3 && retries == 3 && enabled == 1 && timeout == 250 && len(text) > 0 && reparsed_kind == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_json_malformed_and_type_errors_are_stable() {
    let src = r#"
import std.io;
import std.json;

fn err_code(err: JsonError) -> Int {
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

fn main() -> Int effects { io } capabilities { io  } {
    let malformed = match parse("{\"name\":") {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let wrong_type = match decode_int(encode_string("abc")) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let invalid_number = match parse("1.25") {
        Ok(value) => match decode_int(value) {
            Ok(_) => 0,
            Err(err) => err_code(err),
        },
        Err(_) => 0,
    };
    let invalid_string = match parse("\"\\uD800\"") {
        Ok(value) => match decode_string(value) {
            Ok(_) => 0,
            Err(err) => err_code(err),
        },
        Err(err) => err_code(err),
    };

    print_int(malformed * 1000 + wrong_type * 100 + invalid_number * 10 + invalid_string);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "1245\n");
}

#[test]
fn exec_json_runtime_hardening_limits_and_format_failures_are_deterministic() {
    let src = r#"
import std.io;
import std.json;

fn err_code(err: JsonError) -> Int {
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

fn main() -> Int effects { io } capabilities { io  } {
    let over_depth = match parse("[[[[[0]]]]]") {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let over_size = match parse("{\"payload\":\"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789\"}") {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let invalid_escape = match parse("\"\\q\"") {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let invalid_utf = match parse("\"\\uD800\"") {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let malformed_number = match parse("01") {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };
    let overflow_int = match parse("9223372036854775808") {
        Ok(v) => match decode_int(v) {
            Ok(_) => 0,
            Err(err) => err_code(err),
        },
        Err(err) => err_code(err),
    };
    let overflow_float = match parse("1e309") {
        Ok(v) => match decode_float(v) {
            Ok(_) => 0,
            Err(err) => err_code(err),
        },
        Err(err) => err_code(err),
    };

    let score = over_depth * 1000000 +
        over_size * 100000 +
        invalid_escape * 10000 +
        invalid_utf * 1000 +
        malformed_number * 100 +
        overflow_int * 10 +
        overflow_float;
    print_int(score);
    0
}
"#;

    let envs = [
        ("AIC_RT_LIMIT_JSON_DEPTH", "4"),
        ("AIC_RT_LIMIT_JSON_BYTES", "48"),
    ];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "6655444\n");
}

#[test]
fn exec_json_option_null_semantics() {
    let src = r#"
import std.io;
import std.json;

fn main() -> Int effects { io } capabilities { io  } {
    let doc = match parse("{\"a\":null,\"b\":7,\"c\":null}") {
        Ok(value) => value,
        Err(_) => object_empty(),
    };

    let a_score = match object_get(doc, "a") {
        Ok(maybe) => match maybe {
            Some(raw) => if is_null(raw) { 10 } else {
                match decode_int(raw) {
                    Ok(v) => v,
                    Err(_) => 0,
                }
            },
            None => 10,
        },
        Err(_) => 0,
    };
    let b_score = match object_get(doc, "b") {
        Ok(maybe) => match maybe {
            Some(raw) => if is_null(raw) { 10 } else {
                match decode_int(raw) {
                    Ok(v) => v,
                    Err(_) => 0,
                }
            },
            None => 10,
        },
        Err(_) => 0,
    };
    let c_score = match object_get(doc, "c") {
        Ok(maybe) => match maybe {
            Some(raw) => if is_null(raw) { 10 } else {
                match decode_int(raw) {
                    Ok(v) => v,
                    Err(_) => 0,
                }
            },
            None => 10,
        },
        Err(_) => 0,
    };
    let missing_score = match object_get(doc, "missing") {
        Ok(maybe) => match maybe {
            Some(raw) => if is_null(raw) { 10 } else {
                match decode_int(raw) {
                    Ok(v) => v,
                    Err(_) => 0,
                }
            },
            None => 10,
        },
        Err(_) => 0,
    };

    if a_score + b_score + c_score + missing_score == 37 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_float_arithmetic_and_comparisons() {
    let src = r#"
import std.io;

fn main() -> Int effects { io } capabilities { io  } {
    let total32 = (3.5f32 + 2.0f32) * 2.0f32;
    let gt32 = if total32 > 10.0f32 { 1 } else { 0 };
    let eq32 = if total32 == 11.0f32 { 1 } else { 0 };

    let total64 = (3.5f64 + 2.0f64) * 2.0f64;
    let gt64 = if total64 > 10.0f64 { 1 } else { 0 };
    let eq64 = if total64 == 11.0f64 { 1 } else { 0 };

    let alias_total: Float = (3.5 + 2.0) * 2.0;
    let alias_ok = if alias_total == 11.0 { 1 } else { 0 };

    if gt32 + eq32 + gt64 + eq64 + alias_ok == 5 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_float_string_and_json_roundtrip() {
    let src = r#"
import std.io;
import std.json;

fn near(a: Float64, b: Float64, eps: Float64) -> Bool {
    let diff = if a > b { a - b } else { b - a };
    diff <= eps
}

fn main() -> Int effects { io } capabilities { io  } {
    let alias_value: Float = 3.125;
    let alias_roundtrip = match decode_float(encode_float(alias_value)) {
        Ok(v) => v,
        Err(_) => 0.0,
    };
    let f64_roundtrip = match decode_float(encode_float(6.5f64)) {
        Ok(v) => v,
        Err(_) => 0.0,
    };
    let f32_roundtrip = match decode_float(encode_float(1.25f32)) {
        Ok(v) => v,
        Err(_) => 0.0,
    };
    let decode_bad = match decode_float(encode_string("abc")) {
        Ok(_) => 0,
        Err(_) => 1,
    };

    if near(alias_roundtrip, 3.125, 0.0000001)
        && near(f64_roundtrip, 6.5, 0.0000001)
        && near(f32_roundtrip, 1.25, 0.0000001)
        && decode_bad == 1
    {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_float_const_defaults_respect_typed_surfaces() {
    let src = r#"
import std.io;

const C32: Float32 = 1.5f32 + 2.5f32;
const C64: Float64 = 1.5f64 + 2.5f64;
const CALIAS: Float = 1.5 + 2.5;

fn main() -> Int effects { io } capabilities { io  } {
    let c32_ok = if C32 == 4.0f32 { 1 } else { 0 };
    let c64_ok = if C64 == 4.0f64 { 1 } else { 0 };
    let alias_ok = if CALIAS == 4.0 { 1 } else { 0 };
    if c32_ok + c64_ok + alias_ok == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_float_mixed_width_arithmetic_reports_type_error() {
    let src = r#"
fn main() -> Int {
    let left: Float32 = 1.0f32;
    let right: Float64 = 2.0f64;
    let _bad = left + right;
    0
}
"#;
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("main.aic");
    fs::write(&path, src).expect("write source");
    let front = run_frontend(&path).expect("frontend");
    assert!(
        has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    assert!(
        front
            .diagnostics
            .iter()
            .any(|d| d.code == "E1230" && d.message.contains("matching float widths")),
        "diagnostics={:#?}",
        front.diagnostics
    );
}

#[test]
fn exec_numeric_bigint_biguint_decimal_runtime_paths() {
    let src = r#"
import std.io;
import std.numeric;
import std.string;

fn string_eq(left: String, right: String) -> Bool {
    string.len(left) == string.len(right) && string.contains(left, right)
}

fn bigint_ok(v: Result[BigInt, String], expected: String) -> Int {
    match v {
        Ok(value) => if string_eq(numeric.big_int_to_string(value), expected) { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn biguint_ok(v: Result[BigUInt, String], expected: String) -> Int {
    match v {
        Ok(value) => if string_eq(numeric.big_uint_to_string(value), expected) { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn decimal_ok(v: Result[Decimal, String], expected: String) -> Int {
    match v {
        Ok(value) => if string_eq(numeric.decimal_to_string(value), expected) { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn bigint_err(v: Result[BigInt, String]) -> Int {
    match v {
        Ok(_) => 0,
        Err(err) => if string.len(err) > 0 { 1 } else { 0 },
    }
}

fn biguint_err(v: Result[BigUInt, String]) -> Int {
    match v {
        Ok(_) => 0,
        Err(err) => if string.len(err) > 0 { 1 } else { 0 },
    }
}

fn decimal_err(v: Result[Decimal, String]) -> Int {
    match v {
        Ok(_) => 0,
        Err(err) => if string.len(err) > 0 { 1 } else { 0 },
    }
}

fn main() -> Int effects { io } capabilities { io } {
    let big_int_add_ok =
        match numeric.parse_big_int("123456789012345678901234567890") {
            Ok(a) => match numeric.parse_big_int("987654321098765432109876543210") {
                Ok(b) => bigint_ok(
                    numeric.big_int_add(a, b),
                    "1111111110111111111011111111100"
                ),
                Err(_) => 0,
            },
            Err(_) => 0,
        };
    let big_int_sub_ok = bigint_ok(
        numeric.big_int_sub(numeric.big_int_from_int(7), numeric.big_int_from_int(42)),
        "-35"
    );
    let big_int_mul_ok =
        match numeric.parse_big_int("12345678901234567890") {
            Ok(a) => match numeric.parse_big_int("9") {
                Ok(b) => bigint_ok(numeric.big_int_mul(a, b), "111111110111111111010"),
                Err(_) => 0,
            },
            Err(_) => 0,
        };
    let big_int_div_ok =
        match numeric.parse_big_int("-100") {
            Ok(a) => match numeric.parse_big_int("3") {
                Ok(b) => bigint_ok(numeric.big_int_div(a, b), "-33"),
                Err(_) => 0,
            },
            Err(_) => 0,
        };
    let big_int_parse_err = bigint_err(numeric.parse_big_int("12x"));
    let big_int_div_zero_err = bigint_err(
        numeric.big_int_div(numeric.big_int_from_int(1), numeric.big_int_from_int(0))
    );

    let big_uint_add_ok =
        match numeric.parse_big_uint("18446744073709551616") {
            Ok(a) => match numeric.parse_big_uint("10") {
                Ok(b) => biguint_ok(numeric.big_uint_add(a, b), "18446744073709551626"),
                Err(_) => 0,
            },
            Err(_) => 0,
        };
    let big_uint_underflow_err =
        match numeric.parse_big_uint("1") {
            Ok(a) => match numeric.parse_big_uint("2") {
                Ok(b) => biguint_err(numeric.big_uint_sub(a, b)),
                Err(_) => 0,
            },
            Err(_) => 0,
        };
    let big_uint_div_zero_err =
        match numeric.parse_big_uint("7") {
            Ok(a) => match numeric.parse_big_uint("0") {
                Ok(b) => biguint_err(numeric.big_uint_div(a, b)),
                Err(_) => 0,
            },
            Err(_) => 0,
        };

    let decimal_parse_ok = decimal_ok(numeric.parse_decimal("0012.3400"), "12.34");
    let decimal_add_ok =
        match numeric.parse_decimal("1.20") {
            Ok(a) => match numeric.parse_decimal("2.03") {
                Ok(b) => decimal_ok(numeric.decimal_add(a, b), "3.23"),
                Err(_) => 0,
            },
            Err(_) => 0,
        };
    let decimal_sub_ok =
        match numeric.parse_decimal("5.0") {
            Ok(a) => match numeric.parse_decimal("7.25") {
                Ok(b) => decimal_ok(numeric.decimal_sub(a, b), "-2.25"),
                Err(_) => 0,
            },
            Err(_) => 0,
        };
    let decimal_mul_ok =
        match numeric.parse_decimal("2.5") {
            Ok(a) => match numeric.parse_decimal("4") {
                Ok(b) => decimal_ok(numeric.decimal_mul(a, b), "10"),
                Err(_) => 0,
            },
            Err(_) => 0,
        };
    let decimal_div_ok =
        match numeric.parse_decimal("1") {
            Ok(a) => match numeric.parse_decimal("8") {
                Ok(b) => decimal_ok(numeric.decimal_div(a, b), "0.125"),
                Err(_) => 0,
            },
            Err(_) => 0,
        };
    let decimal_parse_err = decimal_err(numeric.parse_decimal("1.2.3"));
    let decimal_div_zero_err =
        match numeric.parse_decimal("7.5") {
            Ok(a) => match numeric.parse_decimal("0") {
                Ok(b) => decimal_err(numeric.decimal_div(a, b)),
                Err(_) => 0,
            },
            Err(_) => 0,
        };

    let score =
        big_int_add_ok +
        big_int_sub_ok +
        big_int_mul_ok +
        big_int_div_ok +
        big_int_parse_err +
        big_int_div_zero_err +
        big_uint_add_ok +
        big_uint_underflow_err +
        big_uint_div_zero_err +
        decimal_parse_ok +
        decimal_add_ok +
        decimal_sub_ok +
        decimal_mul_ok +
        decimal_div_ok +
        decimal_parse_err +
        decimal_div_zero_err;
    if score == 16 {
        print_int(42);
    } else {
        print_int(score);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_numeric_examples_smoke() {
    for path in [
        "examples/types/bigint_factorial.aic",
        "examples/types/decimal_invoice_total.aic",
    ] {
        let src = fs::read_to_string(path).expect("read numeric example");
        let (code, stdout, stderr) = compile_and_run(&src);
        assert_eq!(code, 0, "path={path} stderr={stderr}");
        assert_eq!(stdout, "42\n", "path={path}");
    }
}

#[test]
fn exec_json_seeded_roundtrip_property_check() {
    let src = r#"
import std.io;
import std.json;
import std.rand;
import std.string;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn int_roundtrip(v: Int) -> Int {
    match decode_int(encode_int(v)) {
        Ok(got) => if got == v { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn bool_roundtrip(v: Bool) -> Int {
    match decode_bool(encode_bool(v)) {
        Ok(got) => if got == v { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn string_roundtrip(v: String) -> Int {
    match decode_string(encode_string(v)) {
        Ok(got) => if len(got) == len(v) { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, rand } capabilities { io, rand  } {
    seed(20260220);
    let i1 = random_range(0, 500);
    let i2 = random_range(0, 500);
    let i3 = random_range(0, 500);
    let b1 = random_bool();
    let b2 = random_bool();
    let s1 = if b1 { "alpha" } else { "beta" };
    let s2 = if b2 { "gamma" } else { "pi" };

    let score =
        int_roundtrip(i1) +
        int_roundtrip(i2) +
        int_roundtrip(i3) +
        bool_roundtrip(b1) +
        bool_roundtrip(b2) +
        string_roundtrip(s1) +
        string_roundtrip(s2);

    if score == 7 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_json_serde_nested_adts_roundtrip() {
    let src = r#"
import std.io;
import std.json;
import std.string;

struct Credentials {
    user: String,
    pass: String,
}

enum Event {
    Ping,
    Data(Credentials),
    Fault(String),
}

struct Envelope[T] {
    version: Int,
    data: T,
}

fn main() -> Int effects { io } capabilities { io  } {
    let creds = Credentials {
        user: "alice",
        pass: "p@ss",
    };
    let event: Event = Data(creds);
    let message = Envelope {
        version: 2,
        data: event,
    };

    let encoded = match encode(message) {
        Ok(value) => value,
        Err(_) => encode_null(),
    };
    let wire = match stringify(encoded) {
        Ok(text) => text,
        Err(_) => "",
    };
    let parsed = match parse(wire) {
        Ok(value) => value,
        Err(_) => encode_null(),
    };
    let marker: Option[Envelope[Event]] = None();
    let fallback_event: Event = Ping();
    let decoded: Envelope[Event] = match decode_with(parsed, marker) {
        Ok(value) => value,
        Err(_) => Envelope {
            version: 0,
            data: fallback_event,
        },
    };

    let model_marker: Option[Envelope[Event]] = None();
    let schema_text = match schema(model_marker) {
        Ok(text) => text,
        Err(_) => "",
    };

    let variant_score = match decoded.data {
        Data(payload) => if len(payload.user) == 5 && len(payload.pass) == 4 { 1 } else { 0 },
        _ => 0,
    };
    let version_score = if decoded.version == 2 { 1 } else { 0 };
    let schema_score = if len(schema_text) > 80 { 1 } else { 0 };

    if variant_score + version_score + schema_score == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_json_serde_backward_compat_fixtures() {
    let src = r#"
import std.io;
import std.json;

struct ConfigV2 {
    host: String,
    port: Int,
    tls: Bool,
}

enum WireEvent {
    Ping,
    Data(Int),
}

fn err_code(err: JsonError) -> Int {
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

fn main() -> Int effects { io } capabilities { io  } {
    let old_doc = match parse("{\"host\":\"svc\",\"port\":8080}") {
        Ok(value) => value,
        Err(_) => encode_null(),
    };
    let cfg_marker: Option[ConfigV2] = None();
    let missing_field = match decode_with(old_doc, cfg_marker) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    let future_enum = match parse("{\"tag\":9,\"value\":null}") {
        Ok(value) => value,
        Err(_) => encode_null(),
    };
    let ev_marker: Option[WireEvent] = None();
    let unknown_tag = match decode_with(future_enum, ev_marker) {
        Ok(_) => 0,
        Err(err) => err_code(err),
    };

    print_int(missing_field * 10 + unknown_tag);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "32\n");
}

#[test]
fn exec_json_serde_schema_golden_is_deterministic() {
    let src = r#"
import std.io;
import std.json;

struct User {
    name: String,
    id: Int,
}

enum Status {
    Active,
    Suspended,
}

struct Model {
    user: User,
    status: Status,
}

fn main() -> Int effects { io } capabilities { io  } {
    let marker1: Option[Model] = None();
    let marker2: Option[Model] = None();
    let s1 = match schema(marker1) {
        Ok(text) => text,
        Err(_) => "",
    };
    let s2 = match schema(marker2) {
        Ok(text) => text,
        Err(_) => "",
    };
    print_str(s1);
    print_str(s2);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    let expected = "{\"kind\":\"struct\",\"name\":\"Model\",\"fields\":[{\"name\":\"status\",\"type\":{\"kind\":\"enum\",\"name\":\"Status\",\"tag_encoding\":\"indexed\",\"variants\":[{\"name\":\"Active\",\"tag\":0,\"payload\":null},{\"name\":\"Suspended\",\"tag\":1,\"payload\":null}]}},{\"name\":\"user\",\"type\":{\"kind\":\"struct\",\"name\":\"User\",\"fields\":[{\"name\":\"id\",\"type\":{\"kind\":\"int\"}},{\"name\":\"name\",\"type\":{\"kind\":\"string\"}}]}}]}";
    let expected_stdout = format!("{expected}{expected}");
    assert_eq!(stdout, expected_stdout);
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_url_http_types_normalize_and_integrate_with_net() {
    let src = r#"
import std.http;
import std.io;
import std.net;
import std.string;
import std.url;
import std.vec;

fn main() -> Int effects { io, net } capabilities { io, net  } {
    let fallback = UrlView {
        scheme: "",
        host: "",
        port: None(),
        path: "/",
        query: "",
        fragment: "",
    };
    let parsed = match parse("https://Example.COM:443/api/v1?x=1#frag") {
        Ok(value) => value,
        Err(_) => fallback,
    };
    let normalized = match normalize(parsed) {
        Ok(value) => value,
        Err(_) => "",
    };
    let method = match parse_method("POST") {
        Ok(value) => value,
        Err(_) => Get(),
    };
    let method_text = match method_name(method) {
        Ok(value) => value,
        Err(_) => "",
    };

    let h = match header("Content-Type", "application/json") {
        Ok(value) => value,
        Err(_) => HttpHeader { name: "", value: "" },
    };
    let h_ok = if len(h.name) == 12 && len(h.value) == 16 { 1 } else { 0 };

    let empty_headers: Vec[HttpHeader] = Vec { ptr: 0, len: 0, cap: 0 };
    let req_ok = match request(method, "/v1/data", empty_headers, "{}") {
        Ok(req) => if len(req.target) == 8 { 1 } else { 0 },
        Err(_) => 0,
    };
    let empty_headers2: Vec[HttpHeader] = Vec { ptr: 0, len: 0, cap: 0 };
    let resp_ok = match response(201, empty_headers2, "ok") {
        Ok(resp) => if resp.status == 201 && len(resp.reason) > 0 && len(resp.body) == 2 { 1 } else { 0 },
        Err(_) => 0,
    };

    let listen_url = match parse("tcp://127.0.0.1:0") {
        Ok(value) => value,
        Err(_) => fallback,
    };
    let bind_addr = match net_addr(listen_url) {
        Ok(value) => value,
        Err(_) => "",
    };
    let listener = match tcp_listen(bind_addr) {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let local_addr = match tcp_local_addr(listener) {
        Ok(value) => value,
        Err(_) => "",
    };
    let closed = match tcp_close(listener) {
        Ok(value) => if value { 1 } else { 0 },
        Err(_) => 0,
    };

    let normalized_url = match parse(normalized) {
        Ok(value) => value,
        Err(_) => fallback,
    };
    let normalized_ok =
        if len(normalized_url.scheme) == 5 &&
            len(normalized_url.host) == 11 &&
            has_explicit_port(normalized_url) == false &&
            len(normalized_url.path) == 7 &&
            len(normalized_url.query) == 3 &&
            len(normalized_url.fragment) == 4 {
            1
        } else {
            0
        };
    let method_ok = if len(method_text) == 4 { 1 } else { 0 };
    let net_ok = if len(bind_addr) > 0 && len(local_addr) > 0 && closed == 1 { 1 } else { 0 };

    if normalized_ok + method_ok + h_ok + req_ok + resp_ok + net_ok == 6 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_url_http_malformed_inputs_return_deterministic_errors() {
    let src = r#"
import std.http;
import std.io;
import std.url;
import std.vec;

fn url_err_code(err: UrlError) -> Int {
    match err {
        InvalidUrl => 1,
        InvalidScheme => 2,
        InvalidHost => 3,
        InvalidPort => 4,
        InvalidPath => 5,
        InvalidInput => 6,
        Internal => 7,
    }
}

fn http_err_code(err: HttpError) -> Int {
    match err {
        InvalidMethod => 1,
        InvalidStatus => 2,
        InvalidHeaderName => 3,
        InvalidHeaderValue => 4,
        InvalidTarget => 5,
        InvalidInput => 6,
        Internal => 7,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let bad_scheme = match parse("1tp://example.com") {
        Ok(_) => 0,
        Err(err) => url_err_code(err),
    };
    let bad_host = match parse("http:///x") {
        Ok(_) => 0,
        Err(err) => url_err_code(err),
    };
    let bad_port = match parse("http://example.com:99999/x") {
        Ok(_) => 0,
        Err(err) => url_err_code(err),
    };
    let port_80: UInt16 = 80;
    let bad_path = match normalize(UrlView {
        scheme: "http",
        host: "example.com",
        port: Some(port_80),
        path: "bad-path",
        query: "",
        fragment: "",
    }) {
        Ok(_) => 0,
        Err(err) => url_err_code(err),
    };

    let bad_method = match parse_method("TRACE") {
        Ok(_) => 0,
        Err(err) => http_err_code(err),
    };
    let bad_status = match status_reason(99) {
        Ok(_) => 0,
        Err(err) => http_err_code(err),
    };
    let bad_header_name = match header("Bad Header", "ok") {
        Ok(_) => 0,
        Err(err) => http_err_code(err),
    };
    let bad_header_value = match header("X-Test", "ok\nbad") {
        Ok(_) => 0,
        Err(err) => http_err_code(err),
    };
    let empty_headers: Vec[HttpHeader] = Vec { ptr: 0, len: 0, cap: 0 };
    let bad_target = match request(Get(), "bad-target", empty_headers, "") {
        Ok(_) => 0,
        Err(err) => http_err_code(err),
    };

    let score =
        bad_scheme * 100000000 +
        bad_host * 10000000 +
        bad_port * 1000000 +
        bad_path * 100000 +
        bad_method * 10000 +
        bad_status * 1000 +
        bad_header_name * 100 +
        bad_header_value * 10 +
        bad_target;
    print_int(score);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "234512345\n");
}

#[test]
fn exec_fixed_width_bridge_helpers_enforce_boundaries_across_std_modules() {
    let src = r#"
import std.env;
import std.http;
import std.http_server;
import std.io;
import std.proc;
import std.url;

fn proc_invalid_input(err: ProcError) -> Int {
    match err {
        InvalidInput => 1,
        _ => 0,
    }
}

fn url_invalid_port(err: UrlError) -> Int {
    match err {
        InvalidPort => 1,
        _ => 0,
    }
}

fn http_invalid_status(err: HttpError) -> Int {
    match err {
        InvalidStatus => 1,
        _ => 0,
    }
}

fn http_server_internal(err: ServerError) -> Int {
    match err {
        Internal => 1,
        _ => 0,
    }
}

fn proc_handle_ok(v: Result[ProcHandle, ProcError], expected: ProcHandle) -> Int {
    match v {
        Ok(value) => if value == expected { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn proc_handle_roundtrip(v: Result[ProcHandle, ProcError], expected: Int) -> Int {
    match v {
        Ok(value) => if proc_handle_to_int(value) == expected { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn proc_status_ok(v: Result[ProcExitStatus, ProcError], expected: ProcExitStatus) -> Int {
    match v {
        Ok(value) => if value == expected { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn url_port_roundtrip(v: Result[Option[UInt16], UrlError], expected: Int) -> Int {
    match v {
        Ok(port) => if url_port_to_int(port) == expected { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn http_status_roundtrip(v: Result[UInt16, HttpError], expected: Int) -> Int {
    match v {
        Ok(status) => if http_status_to_int(status) == expected { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn http_server_status_roundtrip(v: Result[UInt16, ServerError], expected: Int) -> Int {
    match v {
        Ok(status) => if http_server_status_to_int(status) == expected { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let zero_i32 = proc_nonnegative_int_to_i32(0);
    let one_i32 = proc_nonnegative_int_to_i32(1);
    let max_i32 = proc_nonnegative_int_to_i32(2147483647);
    let min_i32 = (zero_i32 - max_i32) - one_i32;
    let max_handle: ProcHandle = 4294967295u32;

    let proc_handle_zero = proc_handle_ok(proc_handle_from_int(0), 0u32);
    let proc_handle_mid = proc_handle_roundtrip(proc_handle_from_int(65535), 65535);
    let proc_handle_max = proc_handle_ok(proc_handle_from_int(4294967295), max_handle);
    let proc_handle_negative = match proc_handle_from_int(-1) {
        Ok(_) => 0,
        Err(err) => proc_invalid_input(err),
    };
    let proc_handle_overflow = match proc_handle_from_int(4294967296) {
        Ok(_) => 0,
        Err(err) => proc_invalid_input(err),
    };

    let proc_status_min = proc_status_ok(proc_exit_status_from_int(-2147483648), min_i32);
    let proc_status_max = proc_status_ok(proc_exit_status_from_int(2147483647), max_i32);
    let proc_status_too_low = match proc_exit_status_from_int(-2147483649) {
        Ok(_) => 0,
        Err(err) => proc_invalid_input(err),
    };
    let proc_status_too_high = match proc_exit_status_from_int(2147483648) {
        Ok(_) => 0,
        Err(err) => proc_invalid_input(err),
    };

    let env_zero = if env_i32_to_int(zero_i32) == 0 { 1 } else { 0 };
    let env_pos = if env_i32_to_int(proc_nonnegative_int_to_i32(32767)) == 32767 { 1 } else { 0 };
    let env_negative_input = zero_i32 - proc_nonnegative_int_to_i32(32768);
    let env_neg = if env_i32_to_int(env_negative_input) == -32768 { 1 } else { 0 };

    let url_none = url_port_roundtrip(url_port_from_int(-1), -1);
    let url_zero = url_port_roundtrip(url_port_from_int(0), 0);
    let url_max = url_port_roundtrip(url_port_from_int(65535), 65535);
    let url_over = match url_port_from_int(65536) {
        Ok(_) => 0,
        Err(err) => url_invalid_port(err),
    };

    let http_zero = http_status_roundtrip(http_status_from_int(0), 0);
    let http_max = http_status_roundtrip(http_status_from_int(65535), 65535);
    let http_neg = match http_status_from_int(-1) {
        Ok(_) => 0,
        Err(err) => http_invalid_status(err),
    };
    let http_over = match http_status_from_int(65536) {
        Ok(_) => 0,
        Err(err) => http_invalid_status(err),
    };

    let server_zero = http_server_status_roundtrip(http_server_status_from_int(0), 0);
    let server_max = http_server_status_roundtrip(http_server_status_from_int(65535), 65535);
    let server_neg = match http_server_status_from_int(-1) {
        Ok(_) => 0,
        Err(err) => http_server_internal(err),
    };
    let server_over = match http_server_status_from_int(65536) {
        Ok(_) => 0,
        Err(err) => http_server_internal(err),
    };

    let score =
        proc_handle_zero + proc_handle_mid + proc_handle_max + proc_handle_negative + proc_handle_overflow +
        proc_status_min + proc_status_max + proc_status_too_low + proc_status_too_high +
        env_zero + env_pos + env_neg +
        url_none + url_zero + url_max + url_over +
        http_zero + http_max + http_neg + http_over +
        server_zero + server_max + server_neg + server_over;
    if score == 24 {
        print_int(42);
    } else {
        print_int(score);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_io_read_variants_and_prompt_behaviors() {
    let src = r#"
import std.io;
import std.string;

fn io_err_code(err: IoError) -> Int {
    match err {
        EndOfInput => 1,
        InvalidInput => 2,
        Io => 3,
    }
}

fn ok_len(v: Result[String, IoError], n: Int) -> Int {
    match v {
        Ok(text) => if len(text) == n { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn ok_char(v: Result[String, IoError], expected: String) -> Int {
    match v {
        Ok(text) => if len(text) == len(expected) && string.starts_with(text, expected) { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn expect_err(v: Result[String, IoError], code: Int) -> Int {
    match v {
        Err(err) => if io_err_code(err) == code { 1 } else { 0 },
        _ => 0,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let name_ok = ok_len(prompt("prompt> "), 5);
    let char_ok = ok_char(read_char(), "Q");
    let invalid_char = expect_err(read_char(), 2);
    let eof_line = expect_err(read_line(), 1);

    if name_ok + char_ok + invalid_char + eof_line == 4 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run_with_input(src, "Alice\r\nQ\nab\r\n");
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "prompt> 42\n");
    assert_eq!(stderr, "");
}

#[test]
fn exec_io_read_int_reports_invalid_and_end_of_input() {
    let src = r#"
import std.io;

fn io_err_code(err: IoError) -> Int {
    match err {
        EndOfInput => 1,
        InvalidInput => 2,
        Io => 3,
    }
}

fn expect_ok(v: Result[Int, IoError], expected: Int) -> Int {
    match v {
        Ok(n) => if n == expected { 1 } else { 0 },
        Err(_) => 0,
    }
}

fn expect_err(v: Result[Int, IoError], code: Int) -> Int {
    match v {
        Err(err) => if io_err_code(err) == code { 1 } else { 0 },
        _ => 0,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let ok_value = expect_ok(read_int(), 99);
    let invalid = expect_err(read_int(), 2);
    let eof = expect_err(read_int(), 1);

    if ok_value + invalid + eof == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run_with_input(src, " 99 \nnope\n");
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
    assert_eq!(stderr, "");
}

#[test]
fn exec_io_stderr_stdout_separation_and_newline_semantics() {
    let src = r#"
import std.io;

fn main() -> Int effects { io } capabilities { io  } {
    eprint_str("err:");
    eprint_int(7);
    flush_stderr();

    println_str("alpha");
    println_int(12);
    print_bool(true);
    print_bool(false);
    println_bool(true);
    flush_stdout();
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "alpha\n12\ntruefalsetrue\n");
    assert_eq!(stderr, "err:7");
}

#[test]
fn exec_io_error_conversion_helpers_are_stable() {
    let src = r#"
import std.io;
import std.fs;
import std.net;
import std.proc;
import std.env;

fn io_code(err: IoError) -> Int {
    match err {
        EndOfInput => 1,
        InvalidInput => 2,
        Io => 3,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let fs_missing: FsError = NotFound();
    let fs_invalid: FsError = InvalidInput();
    let net_timeout: NetError = Timeout();
    let proc_invalid: ProcError = InvalidInput();
    let env_missing: EnvError = NotFound();

    let score =
        io_code(from_fs_error(fs_missing)) * 10000 +
        io_code(from_fs_error(fs_invalid)) * 1000 +
        io_code(from_net_error(net_timeout)) * 100 +
        io_code(from_proc_error(proc_invalid)) * 10 +
        io_code(from_env_error(env_missing));
    print_int(score);
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "12121\n");
    assert_eq!(stderr, "");
}

#[test]
fn exec_io_stream_helpers_cover_stdin_stdout_and_stderr() {
    let src = r#"
import std.io;
import std.string;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn main() -> Int effects { io } capabilities { io  } {
    let reader = stdin_reader();
    let out = stdout_writer();
    let err = stderr_writer();

    let read_ok = match read_stream(reader) {
        Ok(text) => if len(text) == 5 && string.starts_with(text, "Alice") { 1 } else { 0 },
        Err(_) => 0,
    };
    let eof_ok = match read_stream_optional(reader) {
        Ok(value) => match value {
            None => 1,
            Some(_) => 0,
        },
        Err(_) => 0,
    };

    let out_ok = match write_stream(out, "hello-") {
        Ok(n) => if n == 6 { 1 } else { 0 },
        Err(_) => 0,
    };
    let err_ok = match write_stream(err, "warn") {
        Ok(n) => if n == 4 { 1 } else { 0 },
        Err(_) => 0,
    };
    let flush_ok =
        match flush_stream(out) {
            Ok(v) => bool_to_int(v),
            Err(_) => 0,
        } +
        match flush_stream(err) {
            Ok(v) => bool_to_int(v),
            Err(_) => 0,
        };

    if read_ok + eof_ok + out_ok + err_ok + flush_ok == 6 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run_with_input(src, "Alice\n");
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "hello-42\n");
    assert_eq!(stderr, "warn");
}

#[test]
fn exec_io_stream_copy_supports_file_and_tcp_streams() {
    let src = r#"
import std.io;
import std.fs;
import std.net;
import std.string;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn close_reader_ok(reader: Reader) -> Int effects { io, fs, net } capabilities { io, fs, net  } {
    match close_reader(reader) {
        Ok(value) => bool_to_int(value),
        Err(_) => 0,
    }
}

fn close_writer_ok(writer: Writer) -> Int effects { io, fs, net } capabilities { io, fs, net  } {
    match close_writer(writer) {
        Ok(value) => bool_to_int(value),
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, fs, net } capabilities { io, fs, net  } {
    let file_reader_stream = match file_reader("stream_input.txt") {
        Ok(value) => value,
        Err(_) => stdin_reader(),
    };
    let file_writer_stream = match file_writer("stream_output.txt") {
        Ok(value) => value,
        Err(_) => stdout_writer(),
    };
    let copied_ok = match stream_copy(file_reader_stream, file_writer_stream) {
        Ok(written) => if written == 10 { 1 } else { 0 },
        Err(_) => 0,
    };
    let copied_text_ok = match fs.read_text("stream_output.txt") {
        Ok(text) => if len(text) == 10 && string.starts_with(text, "payload-42") { 1 } else { 0 },
        Err(_) => 0,
    };
    let file_close_ok = close_reader_ok(file_reader_stream) + close_writer_ok(file_writer_stream);

    let listener = match net.tcp_listen("127.0.0.1:0") {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let local_addr = match net.tcp_local_addr(listener) {
        Ok(addr) => addr,
        Err(_) => "",
    };
    let client = match net.tcp_connect(local_addr, 1000) {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let server = match net.tcp_accept(listener, 1000) {
        Ok(handle) => handle,
        Err(_) => 0,
    };

    let writer_stream = tcp_writer(client);
    let reader_stream = tcp_reader(server, 64, 1000);
    let sent_ok = match write_stream(writer_stream, "echo") {
        Ok(written) => if written == 4 { 1 } else { 0 },
        Err(_) => 0,
    };
    let recv_ok = match read_stream(reader_stream) {
        Ok(text) => if len(text) == 4 && string.starts_with(text, "echo") { 1 } else { 0 },
        Err(_) => 0,
    };
    let tcp_close_ok =
        close_writer_ok(writer_stream) +
        close_reader_ok(reader_stream) +
        match net.tcp_close(listener) {
            Ok(value) => bool_to_int(value),
            Err(_) => 0,
        };

    if copied_ok + copied_text_ok + file_close_ok + sent_ok + recv_ok + tcp_close_ok == 9 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run_with_setup(src, |root| {
        fs::write(root.join("stream_input.txt"), "payload-42").expect("write stream input");
    });
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
    assert_eq!(stderr, "");
}

#[test]
fn exec_debug_build_reports_panic_source_line() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("panic_line_map.aic");
    let source = r#"import std.io;

fn main() -> Int effects { io } capabilities { io  } {
    panic("boom");
    0
}
"#;
    fs::write(&src, source).expect("write source");

    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm_with_options(
        &lowered,
        &src.to_string_lossy(),
        CodegenOptions { debug_info: true },
    )
    .expect("emit llvm");

    let exe = dir.path().join("panic_line_map");
    compile_with_clang_artifact_with_options(
        &llvm.llvm_ir,
        &exe,
        dir.path(),
        ArtifactKind::Exe,
        CompileOptions {
            debug_info: true,
            ..CompileOptions::default()
        },
    )
    .expect("clang build");

    let output = Command::new(exe).output().expect("run exe");
    assert_ne!(output.status.code().unwrap_or(0), 0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("AICore panic at 4:"),
        "stderr did not contain mapped source line:\n{stderr}"
    );
}

#[test]
fn exec_o2_outperforms_o0_on_deterministic_workload() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("opt_bench.aic");
    let source = r#"import std.io;

fn mix(x: Int, y: Int) -> Int {
    let mixed = ((x * 1103515245) + (y * 12345)) ^ (x >> 7);
    mixed + 17
}

fn main() -> Int effects { io } capabilities { io  } {
    let mut i = 0;
    let mut acc = 1;
    while i < 12000000 {
        acc = mix(acc, i);
        i = i + 1;
    };
    print_int(acc);
    0
}
"#;
    fs::write(&src, source).expect("write source");

    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &src.to_string_lossy()).expect("emit llvm");

    let exe_o0 = dir.path().join("bench_o0");
    let exe_o2 = dir.path().join("bench_o2");

    compile_with_clang_artifact_with_options(
        &llvm.llvm_ir,
        &exe_o0,
        dir.path(),
        ArtifactKind::Exe,
        CompileOptions {
            opt_level: OptimizationLevel::O0,
            ..CompileOptions::default()
        },
    )
    .expect("clang build o0");

    compile_with_clang_artifact_with_options(
        &llvm.llvm_ir,
        &exe_o2,
        dir.path(),
        ArtifactKind::Exe,
        CompileOptions {
            opt_level: OptimizationLevel::O2,
            ..CompileOptions::default()
        },
    )
    .expect("clang build o2");

    let _ = run_binary_best_of(&exe_o0, dir.path(), 1);
    let _ = run_binary_best_of(&exe_o2, dir.path(), 1);

    let (o0_best, o0_stdout, o0_stderr) = run_binary_best_of(&exe_o0, dir.path(), 4);
    let (o2_best, o2_stdout, o2_stderr) = run_binary_best_of(&exe_o2, dir.path(), 4);

    assert_eq!(o0_stdout, o2_stdout, "workload output mismatch");
    assert_eq!(o0_stderr, "", "o0 stderr={o0_stderr}");
    assert_eq!(o2_stderr, "", "o2 stderr={o2_stderr}");

    let speedup = o0_best.as_secs_f64() / o2_best.as_secs_f64();
    assert!(
        speedup > 1.05,
        "expected O2 speedup >5%; o0={o0_best:?} o2={o2_best:?} speedup={speedup:.3}"
    );
}

#[test]
fn exec_tail_call_fibonacci_handles_one_million_steps() {
    let src = r#"
import std.io;

fn fib_tail(n: Int, a: Int, b: Int) -> Int {
    if n == 0 {
        a
    } else {
        fib_tail(n - 1, b % 1000000007, (a + b) % 1000000007)
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let fib_mod = fib_tail(1000000, 0, 1);
    if fib_mod == 918091266 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_tail_call_mutual_recursion_handles_one_million_steps() {
    let src = r#"
import std.io;

fn is_even(n: Int) -> Bool {
    if n == 0 {
        true
    } else {
        is_odd(n - 1)
    }
}

fn is_odd(n: Int) -> Bool {
    if n == 0 {
        false
    } else {
        is_even(n - 1)
    }
}

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn main() -> Int effects { io } capabilities { io  } {
    let even_big = bool_to_int(is_even(1000000));
    let odd_big = bool_to_int(is_odd(999999));
    if even_big + odd_big == 2 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_non_tail_recursion_semantics_are_unchanged() {
    let src = r#"
import std.io;

fn countdown(n: Int) -> Int {
    if n == 0 {
        0
    } else {
        1 + countdown(n - 1)
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let value = countdown(1000);
    if value == 1000 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_optimization_levels_preserve_semantics_across_o0_to_o3() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("opt_semantics.aic");
    let source = r#"import std.io;

fn branchy(x: Int) -> Int {
    if x % 2 == 0 {
        (x * 3) - 7
    } else {
        (x * 5) + 11
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let mut i = 0;
    let mut acc = 0;
    while i < 10000 {
        acc = acc + branchy(i);
        i = i + 1;
    };
    print_int(acc);
    0
}
"#;
    fs::write(&src, source).expect("write source");

    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &src.to_string_lossy()).expect("emit llvm");

    let levels = [
        OptimizationLevel::O0,
        OptimizationLevel::O1,
        OptimizationLevel::O2,
        OptimizationLevel::O3,
    ];
    let mut observed = Vec::new();
    for (idx, level) in levels.iter().enumerate() {
        let exe = dir.path().join(format!("opt_semantics_{idx}"));
        compile_with_clang_artifact_with_options(
            &llvm.llvm_ir,
            &exe,
            dir.path(),
            ArtifactKind::Exe,
            CompileOptions {
                opt_level: *level,
                ..CompileOptions::default()
            },
        )
        .expect("clang build");
        let output = Command::new(&exe).output().expect("run exe");
        assert_eq!(
            output.status.code(),
            Some(0),
            "opt level {level:?} failed: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        observed.push(String::from_utf8_lossy(&output.stdout).to_string());
    }

    for window in observed.windows(2) {
        assert_eq!(window[0], window[1], "output mismatch across opt levels");
    }
}

#[test]
fn exec_contract_failure() {
    let src = r#"
import std.io;

fn bad(x: Int) -> Int ensures result >= 0 {
    x
}

fn main() -> Int effects { io } capabilities { io  } {
    print_int(bad(-5));
    0
}
"#;
    let (code, _stdout, stderr) = compile_and_run(src);
    assert_ne!(code, 0);
    assert!(stderr.contains("ensures failed in bad"), "stderr={stderr}");
}

#[test]
fn exec_contract_failure_on_explicit_return() {
    let src = r#"
fn bad_return(x: Int) -> Int ensures result >= 0 {
    return x;
    x
}

fn main() -> Int {
    bad_return(-3)
}
"#;
    let (code, _stdout, stderr) = compile_and_run(src);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("ensures failed in bad_return"),
        "stderr={stderr}"
    );
}

#[test]
fn exec_object_artifact_links_from_external_c_harness() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("math.aic");
    let llvm_ir = compile_to_llvm(
        &src,
        r#"
fn add(x: Int, y: Int) -> Int {
    x + y
}
"#,
    );

    let obj = dir.path().join("math.o");
    compile_with_clang_artifact(&llvm_ir, &obj, dir.path(), ArtifactKind::Obj)
        .expect("build object");

    let harness = dir.path().join("harness_obj.c");
    fs::write(
        &harness,
        r#"#include <stdio.h>

long aic_add(long x, long y);

int main(void) {
    printf("%ld\n", aic_add(20, 22));
    return 0;
}
"#,
    )
    .expect("write harness");

    let exe = dir.path().join("harness_obj");
    let link = Command::new("clang")
        .arg(&harness)
        .arg(&obj)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("link object");
    assert!(
        link.status.success(),
        "link stderr={}",
        String::from_utf8_lossy(&link.stderr)
    );

    let output = Command::new(&exe).output().expect("run harness");
    assert_eq!(
        output.status.code().unwrap_or(1),
        0,
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
}

#[test]
fn exec_static_library_artifact_links_from_external_c_harness() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("math.aic");
    let llvm_ir = compile_to_llvm(
        &src,
        r#"
fn add(x: Int, y: Int) -> Int {
    x + y
}
"#,
    );

    let lib = dir.path().join("libmath.a");
    compile_with_clang_artifact(&llvm_ir, &lib, dir.path(), ArtifactKind::Lib)
        .expect("build static library");

    let harness = dir.path().join("harness_lib.c");
    fs::write(
        &harness,
        r#"#include <stdio.h>

long aic_add(long x, long y);

int main(void) {
    printf("%ld\n", aic_add(17, 25));
    return 0;
}
"#,
    )
    .expect("write harness");

    let exe = dir.path().join("harness_lib");
    let link = Command::new("clang")
        .arg(&harness)
        .arg(&lib)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("link static library");
    assert!(
        link.status.success(),
        "link stderr={}",
        String::from_utf8_lossy(&link.stderr)
    );

    let output = Command::new(&exe).output().expect("run harness");
    assert_eq!(
        output.status.code().unwrap_or(1),
        0,
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn exec_signal_wait_for_sigterm_is_deterministic() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("signal_wait.aic");
    let source = r#"
import std.io;
import std.signal;

fn score(sig: Signal) -> Int {
    match sig {
        SigInt => 10,
        SigTerm => 42,
        SigHup => 30,
    }
}

fn main() -> Int effects { io, proc } capabilities { io, proc  } {
    register_shutdown_handlers();
    println_str("ready");
    flush_stdout();
    let out = match wait_for_signal() {
        Ok(sig) => score(sig),
        Err(_) => 0,
    };
    print_int(out);
    0
}
"#;
    fs::write(&src, source).expect("write source");

    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let llvm = emit_llvm(&lowered, &src.to_string_lossy()).expect("emit llvm");

    let exe = dir.path().join("signal_wait");
    compile_with_clang(&llvm.llvm_ir, &exe, dir.path()).expect("clang build");

    let mut child = Command::new(&exe)
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run exe");
    let stdout_pipe = child.stdout.take().expect("child stdout");
    let mut stdout_reader = BufReader::new(stdout_pipe);
    let mut stderr_pipe = child.stderr.take().expect("child stderr");

    let mut ready_line = String::new();
    stdout_reader
        .read_line(&mut ready_line)
        .expect("read ready line");
    assert_eq!(ready_line, "ready\n");

    let signal_status = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .expect("send SIGTERM");
    assert!(
        signal_status.success(),
        "kill status was not successful: {signal_status:?}"
    );

    let status = child.wait().expect("wait child");
    let mut stdout_rest = String::new();
    stdout_reader
        .read_to_string(&mut stdout_rest)
        .expect("read remaining stdout");
    let mut stderr = String::new();
    stderr_pipe
        .read_to_string(&mut stderr)
        .expect("read stderr");

    assert_eq!(status.code().unwrap_or(1), 0, "stderr={stderr}");
    assert_eq!(format!("{ready_line}{stdout_rest}"), "ready\n42\n");
    assert_eq!(stderr, "");
}

#[test]
fn exec_fs_bytes_binary_roundtrip_and_conversion_semantics() {
    let src = r#"
import std.io;
import std.fs;
import std.bytes;
import std.string;

fn ok_bool(v: Result[Bool, FsError]) -> Int {
    match v {
        Ok(_) => 1,
        Err(_) => 0,
    }
}

fn bytes_err_code(v: Result[String, BytesError]) -> Int {
    match v {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

fn main() -> Int effects { io, fs } capabilities { io, fs  } {
    let input = match read_bytes("blob.bin") {
        Ok(value) => value,
        Err(_) => bytes.empty(),
    };

    let input_utf8_err_ok = if bytes_err_code(bytes.to_string(input)) == 1 { 1 } else { 0 };
    let lossy_len_ok = if string.len(bytes.to_string_lossy(input)) >= 1 { 1 } else { 0 };

    let wrote = ok_bool(write_bytes("roundtrip.bin", input));
    let appended = ok_bool(append_bytes("roundtrip.bin", bytes.from_string("Z")));

    let output = match read_bytes("roundtrip.bin") {
        Ok(value) => value,
        Err(_) => bytes.empty(),
    };
    let output_utf8_err_ok = if bytes_err_code(bytes.to_string(output)) == 1 { 1 } else { 0 };
    let output_lossy_len_ok = if string.len(bytes.to_string_lossy(output)) >= 1 { 1 } else { 0 };
    let concat_ok =
        if bytes_err_code(bytes.to_string(bytes.concat(bytes.empty(), bytes.from_string("Z")))) == 0 {
            1
        } else {
            0
        };

    if input_utf8_err_ok + lossy_len_ok + wrote + appended + output_utf8_err_ok + output_lossy_len_ok + concat_ok == 7 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run_with_setup(src, |root| {
        fs::write(root.join("blob.bin"), [0x66_u8, 0x6f_u8, 0x80_u8, 0x00_u8])
            .expect("write blob bytes");
    });
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_bytes_and_buffer_random_access_helpers() {
    let src = r#"
import std.io;
import std.bytes;
import std.buffer;
import std.vec;

fn int_or(v: Result[UInt8, BytesError], fallback: Int) -> Int {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn int_or_buf(v: Result[UInt8, BufferError], fallback: Int) -> Int {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn main() -> Int effects { io } capabilities { io } {
    let mut values: Vec[UInt8] = vec.new_vec();
    values = vec.push(values, 65);
    values = vec.push(values, 66);
    values = vec.push(values, 67);
    values = vec.push(values, 255);

    let payload = match bytes.from_byte_values(values) {
        Ok(data) => data,
        Err(_) => bytes.empty(),
    };

    let b0 = int_or(bytes.byte_at(payload, 0), -1);
    let b3 = int_or(bytes.byte_at(payload, 3), -1);
    let oob_ok = match bytes.byte_at(payload, 10) {
        Ok(_) => 0,
        Err(_) => 1,
    };

    let slice = match bytes.byte_slice(payload, 1, 3) {
        Ok(data) => data,
        Err(_) => bytes.empty(),
    };
    let slice_ok = if bytes.compare_bytes(slice, bytes.from_string("BC")) == 0 { 1 } else { 0 };
    let find_ff = match bytes.find_byte(payload, 255) {
        Some(index) => index,
        None => -1,
    };
    let prefix_ok = bool_to_int(bytes.starts_with(payload, bytes.from_string("AB")));
    let suffix_ok = bool_to_int(bytes.ends_with(slice, bytes.from_string("C")));

    let as_vec = bytes.to_byte_values(payload);
    let vec_len_ok = bool_to_int(as_vec.len == 4);
    let rebuilt = match bytes.from_byte_values(as_vec) {
        Ok(data) => data,
        Err(_) => bytes.empty(),
    };
    let vec_last = int_or(bytes.byte_at(rebuilt, 3), -1);

    let mut edge_values: Vec[UInt8] = vec.new_vec();
    edge_values = vec.push(edge_values, 0);
    edge_values = vec.push(edge_values, 255);
    let edge_payload = match bytes.from_byte_values(edge_values) {
        Ok(data) => data,
        Err(_) => bytes.empty(),
    };
    let edge_first = int_or(bytes.byte_at(edge_payload, 0), -1);
    let edge_last = int_or(bytes.byte_at(edge_payload, 1), -1);

    let buf = buffer_from_bytes(payload);
    let peek = int_or_buf(buf_peek_u8(buf, 2), -1);
    let pos_after_peek = buf_position(buf);
    let size = buf_size(buf);
    let sliced_buf = match buf_slice(buf, 1, 2) {
        Ok(value) => value,
        Err(_) => new_buffer(1),
    };
    let sliced_buf_ok =
        if bytes.compare_bytes(buffer_to_bytes(sliced_buf), bytes.from_string("BC")) == 0 { 1 } else { 0 };

    let score =
        bool_to_int(b0 == 65) +
        bool_to_int(b3 == 255) +
        oob_ok +
        slice_ok +
        bool_to_int(find_ff == 3) +
        prefix_ok +
        suffix_ok +
        vec_len_ok +
        bool_to_int(vec_last == 255) +
        bool_to_int(edge_first == 0) +
        bool_to_int(edge_last == 255) +
        bool_to_int(peek == 67) +
        bool_to_int(pos_after_peek == 0) +
        bool_to_int(size == 4) +
        sliced_buf_ok;

    if score == 15 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_tcp_bytes_pipeline_roundtrip() {
    let src = r#"
import std.io;
import std.net;
import std.bytes;
import std.string;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn decode_ok(v: Result[String, BytesError], expected: String) -> Int {
    match v {
        Ok(text) => if string.contains(text, expected) && string.len(text) == string.len(expected) {
            1
        } else {
            0
        },
        Err(_) => 0,
    }
}

fn main() -> Int effects { io, net } capabilities { io, net  } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(h) => h,
        Err(_) => 0,
    };
    let listen_addr = match tcp_local_addr(listener) {
        Ok(addr) => addr,
        Err(_) => "",
    };

    let client = match tcp_connect(listen_addr, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };
    let server = match tcp_accept(listener, 1000) {
        Ok(h) => h,
        Err(_) => 0,
    };

    let outbound = bytes.concat(bytes.from_string("wire"), bytes.from_string("-bytes"));
    let sent = match tcp_send(client, outbound) {
        Ok(n) => if n == 10 { 1 } else { 0 },
        Err(_) => 0,
    };

    let received = match tcp_recv(server, 64, 1000) {
        Ok(data) => data,
        Err(_) => bytes.empty(),
    };
    let recv_decode = decode_ok(bytes.to_string(received), "wire-bytes");

    let echoed = match tcp_send(server, bytes.from_string("ack")) {
        Ok(n) => if n == 3 { 1 } else { 0 },
        Err(_) => 0,
    };
    let ack = match tcp_recv(client, 64, 1000) {
        Ok(data) => data,
        Err(_) => bytes.empty(),
    };
    let ack_decode = decode_ok(bytes.to_string(ack), "ack");

    let closed =
        match tcp_close(client) {
            Ok(value) => bool_to_int(value),
            Err(_) => 0,
        } +
        match tcp_close(server) {
            Ok(value) => bool_to_int(value),
            Err(_) => 0,
        } +
        match tcp_close(listener) {
            Ok(value) => bool_to_int(value),
            Err(_) => 0,
        };

    if sent + recv_decode + echoed + ack_decode + closed == 7 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_bitwise_and_shift_with_hex_literals_and_compound_assignments() {
    let src = r#"
import std.io;

fn main() -> Int effects { io } capabilities { io  } {
    let mut x = 0xFF;
    x &= 0x0F;
    x |= 0x20;
    x ^= 0x0A;
    x <<= 2;
    x >>= 3;
    x >>>= 1;

    let neg = ~0x0F;
    let logical = (-1) >>> 63;

    if x == 9 && neg == -16 && logical == 1 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_named_arguments_reorder_and_mixed_positional_are_stable() {
    let src = r#"
import std.io;

fn connect(host: Int, port: Int, timeout_ms: Int, retry: Bool) -> Int {
    if retry {
        host + port + timeout_ms
    } else {
        0
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let named = connect(timeout_ms: 30, retry: true, host: 10, port: 2);
    let mixed = connect(10, retry: true, timeout_ms: 30, port: 2);

    if named == 42 && mixed == 42 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_buffer_binary_protocol_roundtrip() {
    let src = r#"
import std.io;
import std.buffer;
import std.bytes;
import std.string;

fn read_i32_or(v: Result[Int32, BufferError], fallback: Int32) -> Int32 {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn read_i16_or(v: Result[Int16, BufferError], fallback: Int16) -> Int16 {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn read_string_or(v: Result[String, BufferError], fallback: String) -> String {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn read_bytes_or_empty(v: Result[Bytes, BufferError]) -> Bytes {
    match v {
        Ok(value) => value,
        Err(_) => bytes.empty(),
    }
}

fn bytes_to_text_or(v: Bytes, fallback: String) -> String {
    match bytes.to_string(v) {
        Ok(text) => text,
        Err(_) => fallback,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let buf = new_buffer(128);
    let placeholder: Int32 = 0;
    let little_value: Int16 = 0x1234;
    let frame_len_patch: Int32 = 23;
    let wire_len_fallback: Int32 = -1;
    let little_fallback: Int16 = -1;
    let write_placeholder = buf_write_i32_be(buf, placeholder);
    let write_little = buf_write_i16_le(buf, little_value);
    let write_cstring = buf_write_cstring(buf, "hello");
    let write_payload = buf_write_string_prefixed(buf, "payload");

    let frame_len = buf_position(buf);
    let seek_to_header = buf_seek(buf, 0);
    let backpatch_len = buf_write_i32_be(buf, frame_len_patch);
    let seek_to_tail = buf_seek(buf, 23);
    let _write_status = write_placeholder;
    let _little_status = write_little;
    let _cstring_status = write_cstring;
    let _payload_status = write_payload;
    let _seek_header_status = seek_to_header;
    let _backpatch_status = backpatch_len;
    let _seek_tail_status = seek_to_tail;

    buf_reset(buf);
    let wire_len = read_i32_or(buf_read_i32_be(buf), wire_len_fallback);
    let little = read_i16_or(buf_read_i16_le(buf), little_fallback);
    let cstring = read_string_or(buf_read_cstring(buf), "bad");
    let payload = read_bytes_or_empty(buf_read_length_prefixed(buf));
    let payload_text = bytes_to_text_or(payload, "bad");

    let cstring_ok = string.len(cstring) == 5 && string.contains(cstring, "hello");
    let payload_ok = string.len(payload_text) == 7 && string.contains(payload_text, "payload");

    if frame_len == 23 && wire_len == frame_len_patch && little == little_value && cstring_ok && payload_ok {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_buffer_negative_paths_are_typed_and_deterministic() {
    let src = r#"
import std.io;
import std.buffer;

fn is_underflow(v: Result[UInt8, BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            Underflow => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_overflow(v: Result[(), BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            Overflow => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_invalid_utf8(v: Result[String, BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            InvalidUtf8 => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_invalid_input(v: Result[(), BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            InvalidInput => true,
            _ => false,
        },
        _ => false,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let tiny = new_buffer(2);
    let overflow_probe: Int32 = 7;
    let invalid_utf8_byte: UInt8 = 255;
    let nul_byte: UInt8 = 0;
    let overflow_ok = is_overflow(buf_write_i32_be(tiny, overflow_probe));
    let underflow_ok = is_underflow(buf_read_u8(tiny));
    let invalid_seek_ok = is_invalid_input(buf_seek(tiny, 99));

    let utf = new_buffer(4);
    let write_invalid_utf8_byte = buf_write_u8(utf, invalid_utf8_byte);
    let write_nul = buf_write_u8(utf, nul_byte);
    buf_reset(utf);
    let _utf_write_status = write_invalid_utf8_byte;
    let _nul_status = write_nul;
    let invalid_utf8_ok = is_invalid_utf8(buf_read_cstring(utf));

    if overflow_ok && underflow_ok && invalid_seek_ok && invalid_utf8_ok {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_buffer_growable_mode_and_explicit_close_are_deterministic() {
    let src = r#"
import std.io;
import std.buffer;
import std.bytes;

fn is_ok_unit(v: Result[(), BufferError]) -> Bool {
    match v {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn is_ok_true(v: Result[Bool, BufferError]) -> Bool {
    match v {
        Ok(value) => value,
        Err(_) => false,
    }
}

fn is_overflow(v: Result[(), BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            Overflow => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_invalid_input(v: Result[(), BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            InvalidInput => true,
            _ => false,
        },
        _ => false,
    }
}

fn unwrap_growable(v: Result[ByteBuffer, BufferError]) -> ByteBuffer {
    match v {
        Ok(buf) => buf,
        Err(_) => new_buffer(1),
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let growable = unwrap_growable(new_growable_buffer(2, 12));
    let marker_value: UInt8 = 66;
    let cap_probe: Int32 = 7;
    let post_close_probe: UInt8 = 1;
    let payload = bytes.from_string("ABCDEFGH");
    let write_payload_ok = is_ok_unit(buf_write_bytes(growable, payload));
    let write_marker_ok = is_ok_unit(buf_write_u8(growable, marker_value));
    let cap_limit_hit = is_overflow(buf_write_i32_be(growable, cap_probe));

    let close_ok = is_ok_true(buf_close(growable));
    let write_after_close_invalid = is_invalid_input(buf_write_u8(growable, post_close_probe));

    if write_payload_ok && write_marker_ok && cap_limit_hit && close_ok && write_after_close_invalid {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_buffer_unsigned_codecs_and_patch_helpers_are_stable() {
    let src = r#"
import std.io;
import std.buffer;

fn u16_or(v: Result[UInt16, BufferError], fallback: UInt16) -> UInt16 {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn u32_or(v: Result[UInt32, BufferError], fallback: UInt32) -> UInt32 {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn u64_or(v: Result[UInt64, BufferError], fallback: UInt64) -> UInt64 {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn is_invalid_input_unit(v: Result[(), BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            InvalidInput => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_invalid_input_u64(v: Result[UInt64, BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            InvalidInput => true,
            _ => false,
        },
        _ => false,
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let max_u16: UInt16 = 65535;
    let zero_u32: UInt32 = 0;
    let sample_u64: UInt64 = 9007199254740991;
    let patched_len_expected: UInt32 = 14;
    let closed_write_value: UInt16 = 7;
    let invalid_patch_value: UInt32 = 7;
    let signed_minus_one: Int64 = -1;
    let fallback_u16: UInt16 = 0;
    let fallback_u32: UInt32 = 0;
    let fallback_u64: UInt64 = 0;
    let frame = new_buffer(64);
    let _a = buf_write_u16_be(frame, max_u16);
    let _b = buf_write_u32_be(frame, zero_u32);
    let _c = buf_write_u64_le(frame, sample_u64);
    let end = buf_position(frame);
    let _patch_ok = buf_patch_u32_be(frame, 2, patched_len_expected);

    buf_reset(frame);
    let u16 = u16_or(buf_read_u16_be(frame), fallback_u16);
    let patched_len = u32_or(buf_read_u32_be(frame), fallback_u32);
    let u64 = u64_or(buf_read_u64_le(frame), fallback_u64);

    let closed = new_buffer(8);
    let _closed_once = buf_close(closed);
    let invalid_u16_write = is_invalid_input_unit(buf_write_u16_be(closed, closed_write_value));
    let invalid_patch_offset = is_invalid_input_unit(buf_patch_u32_be(frame, 200, invalid_patch_value));

    let signed = new_buffer(16);
    let _signed_write = buf_write_i64_be(signed, signed_minus_one);
    buf_reset(signed);
    let invalid_u64_read = is_invalid_input_u64(buf_read_u64_be(signed));

    if end == 14
        && u16 == max_u16
        && patched_len == patched_len_expected
        && u64 == sample_u64
        && invalid_u16_write
        && invalid_patch_offset
        && invalid_u64_read {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_buffer_u32_wrappers_and_int_compatibility_paths_are_deterministic() {
    let src = r#"
import std.io;
import std.buffer;
import std.bytes;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn u32_or(v: Result[UInt32, BufferError], fallback: UInt32) -> UInt32 {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn is_ok_unit(v: Result[(), BufferError]) -> Bool {
    match v {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn is_invalid_input_unit(v: Result[(), BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            InvalidInput => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_invalid_input_bytes(v: Result[Bytes, BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            InvalidInput => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_invalid_input_buf(v: Result[ByteBuffer, BufferError]) -> Bool {
    match v {
        Err(err) => match err {
            InvalidInput => true,
            _ => false,
        },
        _ => false,
    }
}

fn unwrap_buf(v: Result[ByteBuffer, BufferError]) -> ByteBuffer {
    match v {
        Ok(buf) => buf,
        Err(_) => new_buffer(1),
    }
}

fn main() -> Int effects { io } capabilities { io  } {
    let base = unwrap_buf(new_buffer_u32(16u32));
    let a: UInt8 = 65;
    let b: UInt8 = 66;
    let _w0 = buf_write_u8(base, a);
    let _w1 = buf_write_u8(base, b);
    let pos_u32 = u32_or(buf_position_u32(base), 0u32);
    let size_u32 = u32_or(buf_size_u32(base), 0u32);
    let seek_u32_ok = bool_to_int(is_ok_unit(buf_seek_u32(base, 0u32)));
    let read_two = match buf_read_bytes_u32(base, 2u32) {
        Ok(value) => value,
        Err(_) => bytes.empty(),
    };
    let read_two_ok = bool_to_int(bytes.compare_bytes(read_two, bytes.from_string("AB")) == 0);

    buf_reset(base);
    let sliced = match buf_slice_u32(base, 0u32, 2u32) {
        Ok(value) => value,
        Err(_) => new_buffer(1),
    };
    let sliced_ok = bool_to_int(
        bytes.compare_bytes(buffer_to_bytes(sliced), bytes.from_string("AB")) == 0
    );

    let growable = unwrap_buf(new_growable_buffer_u32(2u32, 8u32));
    let grow_write_ok = bool_to_int(is_ok_unit(buf_write_bytes(growable, bytes.from_string("WXYZ"))));
    let grow_pos_u32 = u32_or(buf_position_u32(growable), 0u32);

    let too_large: UInt32 = 2147483648u32;
    let invalid_new = bool_to_int(is_invalid_input_buf(new_buffer_u32(too_large)));
    let invalid_new_growable = bool_to_int(is_invalid_input_buf(new_growable_buffer_u32(1u32, too_large)));
    let invalid_seek = bool_to_int(is_invalid_input_unit(buf_seek_u32(base, too_large)));
    let invalid_read = bool_to_int(is_invalid_input_bytes(buf_read_bytes_u32(base, too_large)));
    let invalid_slice = bool_to_int(is_invalid_input_buf(buf_slice_u32(base, too_large, 1u32)));

    let compat = new_buffer(8);
    let z: UInt8 = 90;
    let _compat_write = buf_write_u8(compat, z);
    let compat_pos = buf_position(compat);
    let compat_size = buf_size(compat);
    let compat_seek_ok = bool_to_int(is_ok_unit(buf_seek(compat, 0)));
    let compat_read_ok = match buf_read_bytes(compat, 1) {
        Ok(value) => bool_to_int(bytes.byte_len(value) == 1),
        Err(_) => 0,
    };

    let score =
        bool_to_int(pos_u32 == 2u32) +
        bool_to_int(size_u32 == 2u32) +
        seek_u32_ok +
        read_two_ok +
        sliced_ok +
        grow_write_ok +
        bool_to_int(grow_pos_u32 == 4u32) +
        invalid_new +
        invalid_new_growable +
        invalid_seek +
        invalid_read +
        invalid_slice +
        bool_to_int(compat_pos == 1) +
        bool_to_int(compat_size == 1) +
        compat_seek_ok +
        compat_read_ok;

    if score == 16 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_crypto_vectors_roundtrip_and_secure_compare_paths() {
    let src = r#"
import std.io;
import std.crypto;
import std.bytes;

fn bool_to_int(value: Bool) -> Int {
    if value { 1 } else { 0 }
}

fn bytes_or_empty(v: Result[Bytes, CryptoError]) -> Bytes {
    match v {
        Ok(value) => value,
        Err(_) => bytes.empty(),
    }
}

fn bytes_match_hex(data: Bytes, expected_hex: String) -> Bool {
    match hex_decode(expected_hex) {
        Ok(expected) => secure_eq(data, expected),
        Err(_) => false,
    }
}

fn digest_matches_hex(digest_hex: String, expected_hex: String) -> Bool {
    match hex_decode(digest_hex) {
        Ok(actual) => bytes_match_hex(actual, expected_hex),
        Err(_) => false,
    }
}

fn main() -> Int effects { io, rand } capabilities { io, rand } {
    let md5_ok = bool_to_int(
        digest_matches_hex(md5("hello"), "5d41402abc4b2a76b9719d911017c592")
    );
    let md5_empty_ok = bool_to_int(
        digest_matches_hex(md5(""), "d41d8cd98f00b204e9800998ecf8427e")
    );
    let sha_ok = bool_to_int(
        digest_matches_hex(
            sha256("hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        )
    );
    let sha_empty_ok = bool_to_int(
        digest_matches_hex(
            sha256(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        )
    );
    let hmac_ok = bool_to_int(
        digest_matches_hex(
            hmac_sha256("key", "The quick brown fox jumps over the lazy dog"),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
        )
    );
    let hmac_rfc_ok = bool_to_int(
        bytes_match_hex(
            hmac_sha256_raw(
                bytes_or_empty(hex_decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b")),
                bytes.from_string("Hi There"),
            ),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
        )
    );

    let pb = bytes_or_empty(
        pbkdf2_sha256("password", bytes.from_string("salt"), 1, 32)
    );
    let pb_ok = bool_to_int(bytes_match_hex(pb, "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"));
    let pb_rfc_ok = bool_to_int(
        bytes_match_hex(
            bytes_or_empty(
                pbkdf2_sha256("password", bytes.from_string("salt"), 4096, 32)
            ),
            "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a",
        )
    );

    let sha_raw_len_ok = bool_to_int(bytes.byte_len(sha256_raw("hello")) == 32);
    let hmac_raw_len_ok = bool_to_int(
        bytes.byte_len(
            hmac_sha256_raw(bytes.from_string("key"), bytes.from_string("message"))
        ) == 32
    );

    let sample = bytes.from_string("aicore");
    let hex_roundtrip = match hex_decode(hex_encode(sample)) {
        Ok(decoded) => secure_eq(decoded, sample),
        Err(_) => false,
    };
    let b64_roundtrip = match base64_decode(base64_encode(sample)) {
        Ok(decoded) => secure_eq(decoded, sample),
        Err(_) => false,
    };

    let random_a = random_bytes(16);
    let random_b = random_bytes(16);
    let random_len_ok = bool_to_int(bytes.byte_len(random_a) == 16 && bytes.byte_len(random_b) == 16);

    let secure_eq_ok = bool_to_int(
        secure_eq(bytes.from_string("same"), bytes.from_string("same"))
            && !secure_eq(bytes.from_string("same"), bytes.from_string("diff"))
    );

    let score =
        md5_ok +
        md5_empty_ok +
        sha_ok +
        sha_empty_ok +
        hmac_ok +
        hmac_rfc_ok +
        pb_ok +
        pb_rfc_ok +
        sha_raw_len_ok +
        hmac_raw_len_ok +
        bool_to_int(hex_roundtrip) +
        bool_to_int(b64_roundtrip) +
        random_len_ok +
        secure_eq_ok;

    if score == 14 {
        print_int(42);
    } else {
        print_int(score);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_crypto_invalid_inputs_return_stable_error_variants() {
    let src = r#"
import std.io;
import std.crypto;
import std.bytes;

fn is_invalid_input(v: Result[Bytes, CryptoError]) -> Bool {
    match v {
        Err(err) => match err {
            InvalidInput => true,
            _ => false,
        },
        _ => false,
    }
}

fn pbkdf2_invalid(iterations: Int, key_len: Int) -> Bool {
    match pbkdf2_sha256("password", bytes.from_string("salt"), iterations, key_len) {
        Err(err) => match err {
            InvalidInput => true,
            _ => false,
        },
        _ => false,
    }
}

fn main() -> Int effects { io } capabilities { io } {
    let bad_hex_len_ok = is_invalid_input(hex_decode("0"));
    let bad_hex_char_ok = is_invalid_input(hex_decode("zz"));
    let bad_b64_ok = is_invalid_input(base64_decode("%%%="));
    let bad_pbkdf2_iterations_ok = pbkdf2_invalid(0, 32);
    let bad_pbkdf2_key_len_ok = pbkdf2_invalid(1, 0);

    if bad_hex_len_ok &&
        bad_hex_char_ok &&
        bad_b64_ok &&
        bad_pbkdf2_iterations_ok &&
        bad_pbkdf2_key_len_ok {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_tls_local_server_handshake_and_certificate_paths() {
    let backend_enabled = tls_backend_enabled_for_tests();
    let openssl_cli_available = openssl_cli_available_for_tests();
    if backend_enabled && !openssl_cli_available {
        return;
    }

    let src = r#"
import std.io;
import std.tls;
import std.env;
import std.net;
import std.string;
import std.bytes;
import std.net;
import std.vec;

fn bool_to_int(value: Bool) -> Int {
    if value { 1 } else { 0 }
}

fn read_env_or(key: String, fallback: String) -> String effects { env } capabilities { env } {
    match env.get(key) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn cert_failure(err: TlsError) -> Bool {
    match err {
        CertificateInvalid => true,
        CertificateExpired => true,
        HostnameMismatch => true,
        HandshakeFailed => true,
        _ => false,
    }
}

fn protocol_error(err: TlsError) -> Bool {
    match err {
        ProtocolError => true,
        _ => false,
    }
}

fn none_string() -> Option[String] {
    None()
}

fn san_entries_ok(entries: Vec[String]) -> Bool effects { net } capabilities { net } {
    if vec.vec_len(entries) == 0 {
        true
    } else {
        let mut found = false;
        for entry in entries {
            found = if string.contains(entry, "localhost") {
                true
            } else {
                found
            };
        };
        found
    }
}

fn score_connected(stream: TlsStream, addr: String, secure: TlsConfig) -> Int effects { net } capabilities { net } {
    let write_ok = match tls_send_bytes(
        stream,
        bytes.from_string("GET / HTTP/1.0\nHost: localhost\n\n"),
    ) {
        Ok(sent) => sent > 0,
        Err(_) => false,
    };
    let recv = tls_recv_bytes(stream, 2048, 5000);
    let response_ok = match recv {
        Ok(payload) => string.contains(bytes.to_string_lossy(payload), "HTTP/"),
        Err(_) => false,
    };
    let subject_ok = match tls_peer_subject(stream) {
        Ok(subject) => string.contains(subject, "localhost"),
        Err(_) => false,
    };
    let cn_ok = match tls_peer_cn(stream) {
        Ok(cn) => string.contains(cn, "localhost"),
        Err(_) => false,
    };
    let issuer_ok = match tls_peer_issuer(stream) {
        Ok(issuer) => string.contains(issuer, "localhost"),
        Err(_) => false,
    };
    let fingerprint_ok = match tls_peer_fingerprint_sha256(stream) {
        Ok(value) => string.contains(value, ":") && len(value) >= 64,
        Err(_) => false,
    };
    let san_ok = match tls_peer_san_entries(stream) {
        Ok(entries) => san_entries_ok(entries),
        Err(_) => false,
    };
    let version_ok = match tls_version(stream) {
        Ok(v) => match v {
            Tls12 => true,
            Tls13 => true,
        },
        Err(_) => false,
    };
    let version_code_ok = match tls_version_code(stream) {
        Ok(code) => if code == tls_version_to_code(Tls12()) {
            true
        } else {
            code == tls_version_to_code(Tls13())
        },
        Err(_) => false,
    };
    let version_bridge_ok = match tls_version_code(stream) {
        Ok(code) => match tls_version_from_code(code) {
            Ok(v) => match v {
                Tls12 => true,
                Tls13 => true,
            },
            Err(_) => false,
        },
        Err(_) => false,
    };
    let close_ok = match tls_close(stream) {
        Ok(closed) => closed,
        Err(_) => false,
    };

    let secure_ok = match tls_connect_addr(addr, secure, 5000) {
        Ok(stream2) => match tls_close(stream2) {
            Ok(closed) => closed,
            Err(_) => false,
        },
        Err(_) => false,
    };

    let default_cert_reject = match tls_connect_addr(addr, default_tls_config(), 5000) {
        Ok(stream3) => match tls_close(stream3) {
            Ok(_) => false,
            Err(_) => false,
        },
        Err(err) => cert_failure(err),
    };

    let wrapped_ok = match tcp_connect(addr, 5000) {
        Ok(handle) => match tls_connect(handle, "localhost", secure) {
            Ok(stream4) => match tls_close(stream4) {
                Ok(closed) => closed,
                Err(_) => false,
            },
            Err(_) => false,
        },
        Err(_) => false,
    };

    let upgraded_ok = match tcp_connect(addr, 5000) {
        Ok(handle) => match tls_upgrade(handle, "localhost", secure) {
            Ok(stream5) => match tls_close(stream5) {
                Ok(closed) => closed,
                Err(_) => false,
            },
            Err(_) => false,
        },
        Err(_) => false,
    };

    let score = bool_to_int(write_ok)
        + bool_to_int(response_ok)
        + bool_to_int(subject_ok)
        + bool_to_int(cn_ok)
        + bool_to_int(issuer_ok)
        + bool_to_int(fingerprint_ok)
        + bool_to_int(san_ok)
        + bool_to_int(version_ok)
        + bool_to_int(version_code_ok)
        + bool_to_int(version_bridge_ok)
        + bool_to_int(close_ok)
        + bool_to_int(secure_ok)
        + bool_to_int(default_cert_reject)
        + bool_to_int(wrapped_ok)
        + bool_to_int(upgraded_ok);
    if score == 15 { 42 } else { score }
}

fn main() -> Int effects { io, net, env } capabilities { io, net, env } {
    let addr = read_env_or("AIC_TLS_ADDR", "127.0.0.1:65535");
    let ca_path = read_env_or("AIC_TLS_CA_PATH", "tls_cert.pem");

    let insecure = TlsConfig {
        verify_server: false,
        ca_cert_path: none_string(),
        client_cert_path: none_string(),
        client_key_path: none_string(),
        server_name: Some("localhost"),
    };

    let secure = TlsConfig {
        verify_server: true,
        ca_cert_path: Some(ca_path),
        client_cert_path: none_string(),
        client_key_path: none_string(),
        server_name: Some("localhost"),
    };

    let result = match tls_connect_addr(addr, insecure, 5000) {
        Err(err) => if protocol_error(err) { 43 } else { 0 },
        Ok(stream) => score_connected(stream, addr, secure),
    };
    print_int(result);
    0
}
"#;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind tls probe listener");
    let port = listener.local_addr().expect("listener addr").port();
    drop(listener);

    let addr_env = format!("127.0.0.1:{port}");
    let cert_env = "tls_cert.pem".to_string();
    let envs = [
        ("AIC_TLS_ADDR", addr_env.as_str()),
        ("AIC_TLS_CA_PATH", cert_env.as_str()),
    ];
    let (code, stdout, stderr) =
        compile_and_run_with_server_setup_and_args_and_input_and_env(src, &[], "", &envs, |root| {
            if !(backend_enabled && openssl_cli_available) {
                return None;
            }

            generate_local_tls_cert(root);
            let mut server = spawn_local_tls_server(root, port, true);
            wait_for_local_tls_server(port, &mut server);
            Some(server)
        });
    assert_eq!(code, 0, "stderr={stderr}");
    if backend_enabled && openssl_cli_available {
        assert_eq!(stdout, "42\n", "stderr={stderr}");
    } else {
        assert_eq!(stdout, "43\n", "stderr={stderr}");
    }
}

#[test]
#[ignore = "Flaky with OpenSSL 3.6 process lifecycle; pending deterministic harness fix"]
fn exec_tls_recv_timeout_then_connection_closed_is_deterministic() {
    let backend_enabled = tls_backend_enabled_for_tests();
    let openssl_cli_available = openssl_cli_available_for_tests();
    if !(backend_enabled && openssl_cli_available) {
        return;
    }

    let src = r#"
import std.io;
import std.tls;
import std.net;
import std.bytes;
import std.vec;
import std.vec;
import std.env;

fn read_env_or(key: String, fallback: String) -> String effects { env } capabilities { env } {
    match env.get(key) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn none_string() -> Option[String] {
    None()
}

fn tls_code(err: TlsError) -> Int {
    match err {
        HandshakeFailed => 1,
        CertificateInvalid => 2,
        CertificateExpired => 3,
        HostnameMismatch => 4,
        ProtocolError => 5,
        ConnectionClosed => 6,
        Io => 7,
        Timeout => 8,
        Cancelled => 9,
    }
}

fn score_connected(stream: TlsStream) -> Int effects { net } capabilities { net } {
    let timeout_code = match tls_recv_bytes(stream, 64, 20) {
        Ok(_) => 0,
        Err(err) => tls_code(err),
    };
    let send_ok = match tls_send_bytes(
        stream,
        bytes.from_string("GET / HTTP/1.0\nHost: localhost\nConnection: close\n\n"),
    ) {
        Ok(sent) => if sent > 0 { 1 } else { 0 },
        Err(_) => 0,
    };
    let local_close_ok = match tls_close(stream) {
        Ok(closed) => if closed { 1 } else { 0 },
        Err(_) => 0,
    };

    if timeout_code == 8 && send_ok == 1 && local_close_ok == 1 {
        42
    } else {
        timeout_code * 100 + send_ok * 10 + local_close_ok
    }
}

fn main() -> Int effects { io, net, env } capabilities { io, net, env } {
    let addr = read_env_or("AIC_TLS_ADDR", "127.0.0.1:65535");
    let cfg = TlsConfig {
        verify_server: false,
        ca_cert_path: none_string(),
        client_cert_path: none_string(),
        client_key_path: none_string(),
        server_name: Some("localhost"),
    };

    let result = match tls_connect_addr(addr, cfg, 5000) {
        Err(err) => tls_code(err),
        Ok(stream) => score_connected(stream),
    };

    print_int(result);
    0
}
"#;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind tls close listener");
    let port = listener.local_addr().expect("listener addr").port();
    drop(listener);

    let addr_env = format!("127.0.0.1:{port}");
    let envs = [("AIC_TLS_ADDR", addr_env.as_str())];
    let (code, stdout, stderr) =
        compile_and_run_with_server_setup_and_args_and_input_and_env(src, &[], "", &envs, |root| {
            generate_local_tls_cert(root);
            let mut server = spawn_local_tls_server(root, port, true);
            wait_for_local_tls_server(port, &mut server);
            Some(server)
        });
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[test]
fn exec_tls_async_cancel_reports_typed_cancelled_error() {
    let backend_enabled = tls_backend_enabled_for_tests();
    let openssl_cli_available = openssl_cli_available_for_tests();
    if !(backend_enabled && openssl_cli_available) {
        return;
    }

    let src = r#"
import std.io;
import std.tls;
import std.net;
import std.time;
import std.env;
import std.vec;

fn bool_to_int(value: Bool) -> Int {
    if value { 1 } else { 0 }
}

fn read_env_or(key: String, fallback: String) -> String effects { env } capabilities { env } {
    match env.get(key) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn none_string() -> Option[String] {
    None()
}

fn tls_code(err: TlsError) -> Int {
    match err {
        HandshakeFailed => 1,
        CertificateInvalid => 2,
        CertificateExpired => 3,
        HostnameMismatch => 4,
        ProtocolError => 5,
        ConnectionClosed => 6,
        Io => 7,
        Timeout => 8,
        Cancelled => 9,
    }
}

fn main() -> Int effects { io, net, env, concurrency, time } capabilities { io, net, env, concurrency, time } {
    let addr = read_env_or("AIC_TLS_ADDR", "127.0.0.1:65535");
    let cfg = TlsConfig {
        verify_server: false,
        ca_cert_path: none_string(),
        client_cert_path: none_string(),
        client_key_path: none_string(),
        server_name: Some("localhost"),
    };

    let result = match tls_connect_addr(addr, cfg, 5000) {
        Err(_) => 0,
        Ok(stream) => if true {
            let timeout_op = match tls_async_recv_submit(stream, 64, 2000) {
                Ok(value) => value,
                Err(_) => AsyncStringOp { handle: 0 },
            };
            let timeout_code = match tls_async_wait_many_string(vec.vec_of(timeout_op), 20) {
                Ok(_) => 0,
                Err(err) => tls_code(err),
            };
            let ignored_timeout_cancel = tls_async_cancel_string(timeout_op);
            let ignored_timeout_wait = tls_async_wait_string(timeout_op, 500);

            let op = match tls_async_recv_submit(stream, 64, 2000) {
                Ok(value) => value,
                Err(_) => AsyncStringOp { handle: 0 },
            };
            let handle = op.handle;
            let cancel_ok = match tls_async_cancel_string(AsyncStringOp { handle: handle }) {
                Ok(value) => bool_to_int(value),
                Err(_) => 0,
            };
            let wait_code = match tls_async_wait_many_string(vec.vec_of(AsyncStringOp { handle: handle }), 500) {
                Ok(_) => 0,
                Err(err) => tls_code(err),
            };
            sleep_ms(100);
            let close_ok = match tls_close(stream) {
                Ok(value) => bool_to_int(value),
                Err(_) => 0,
            };
            let timeout_ok = if timeout_code == 8 || timeout_code == 6 { 1 } else { 0 };
            if timeout_ok == 1 && cancel_ok == 1 && wait_code == 9 && close_ok == 1 {
                42
            } else {
                timeout_ok * 1000 + cancel_ok * 100 + wait_code * 10 + close_ok
            }
        } else {
            0
        },
    };

    print_int(result);
    0
}
"#;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind tls cancel listener");
    let port = listener.local_addr().expect("listener addr").port();
    drop(listener);

    let addr_env = format!("127.0.0.1:{port}");
    let envs = [("AIC_TLS_ADDR", addr_env.as_str())];
    let (code, stdout, stderr) =
        compile_and_run_with_server_setup_and_args_and_input_and_env(src, &[], "", &envs, |root| {
            generate_local_tls_cert(root);
            let mut server = spawn_local_tls_server(root, port, true);
            wait_for_local_tls_server(port, &mut server);
            Some(server)
        });
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[test]
fn exec_tls_invalid_handle_paths_are_typed() {
    let src = r#"
import std.io;
import std.tls;
import std.bytes;

fn is_protocol(v: Result[Int, TlsError]) -> Bool {
    match v {
        Err(err) => match err {
            ProtocolError => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_protocol_version(v: Result[TlsVersion, TlsError]) -> Bool {
    match v {
        Err(err) => match err {
            ProtocolError => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_protocol_version_code(v: Result[TlsVersionCode, TlsError]) -> Bool {
    match v {
        Err(err) => match err {
            ProtocolError => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_protocol_string(v: Result[String, TlsError]) -> Bool {
    match v {
        Err(err) => match err {
            ProtocolError => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_protocol_string_vec(v: Result[Vec[String], TlsError]) -> Bool {
    match v {
        Err(err) => match err {
            ProtocolError => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_protocol_bool(v: Result[Bool, TlsError]) -> Bool {
    match v {
        Err(err) => match err {
            ProtocolError => true,
            _ => false,
        },
        _ => false,
    }
}

fn is_protocol_byte_stream(v: Result[Bytes, ByteStreamError]) -> Bool {
    match v {
        Err(err) => match err {
            Tls(tls) => match tls {
                ProtocolError => true,
                _ => false,
            },
            _ => false,
        },
        _ => false,
    }
}

fn bool_to_int(value: Bool) -> Int {
    if value { 1 } else { 0 }
}

fn main() -> Int effects { io, net, time } capabilities { io, net, time } {
    let bad = TlsStream { handle: 9999 };
    let send_ok = is_protocol(tls_send_bytes(bad, bytes.from_string("x")));
    let recv_ok = match tls_recv_bytes(bad, 8, 10) {
        Err(err) => match err {
            ProtocolError => true,
            _ => false,
        },
        _ => false,
    };
    let recv_exact_ok = match tls_recv_exact(bad, 8, 10) {
        Err(err) => match err {
            ProtocolError => true,
            _ => false,
        },
        _ => false,
    };
    let recv_framed_ok = match tls_recv_framed(bad, 64, 10) {
        Err(err) => match err {
            ProtocolError => true,
            _ => false,
        },
        _ => false,
    };
    let byte_stream_exact_ok =
        is_protocol_byte_stream(byte_stream_recv_exact(byte_stream_from_tls(bad), 8, 10));
    let byte_stream_framed_ok =
        is_protocol_byte_stream(byte_stream_recv_framed(byte_stream_from_tls(bad), 64, 10));
    let subject_ok = is_protocol_string(tls_peer_subject(bad));
    let cn_ok = is_protocol_string(tls_peer_cn(bad));
    let issuer_ok = is_protocol_string(tls_peer_issuer(bad));
    let fingerprint_ok = is_protocol_string(tls_peer_fingerprint_sha256(bad));
    let san_ok = is_protocol_string_vec(tls_peer_san_entries(bad));
    let version_ok = is_protocol_version(tls_version(bad));
    let version_code_ok = is_protocol_version_code(tls_version_code(bad));
    let close_ok = is_protocol_bool(tls_close(bad));
    let accept_ok = match tls_accept(9999, default_tls_config()) {
        Err(err) => match err {
            ProtocolError => true,
            _ => false,
        },
        _ => false,
    };

    let score = bool_to_int(send_ok)
        + bool_to_int(recv_ok)
        + bool_to_int(recv_exact_ok)
        + bool_to_int(recv_framed_ok)
        + bool_to_int(byte_stream_exact_ok)
        + bool_to_int(byte_stream_framed_ok)
        + bool_to_int(close_ok)
        + bool_to_int(subject_ok)
        + bool_to_int(cn_ok)
        + bool_to_int(issuer_ok)
        + bool_to_int(fingerprint_ok)
        + bool_to_int(san_ok)
        + bool_to_int(version_ok)
        + bool_to_int(version_code_ok)
        + bool_to_int(accept_ok);
    if score == 15 {
        print_int(42);
    } else {
        print_int(score);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_tls_async_invalid_handle_paths_are_typed() {
    let src = r#"
import std.io;
import std.tls;
import std.net;
import std.bytes;
import std.vec;

fn tls_code(err: TlsError) -> Int {
    match err {
        HandshakeFailed => 1,
        CertificateInvalid => 2,
        CertificateExpired => 3,
        HostnameMismatch => 4,
        ProtocolError => 5,
        ConnectionClosed => 6,
        Io => 7,
        Timeout => 8,
        Cancelled => 9,
    }
}

fn bool_to_int(value: Bool) -> Int {
    if value { 1 } else { 0 }
}

fn main() -> Int effects { io, net, concurrency, time } capabilities { io, net, concurrency, time } {
    let invalid_int_wait = match tls_async_wait_int(AsyncIntOp { handle: 0 }, 10) {
        Ok(_) => 0,
        Err(err) => tls_code(err),
    };
    let invalid_string_wait = match tls_async_wait_string(AsyncStringOp { handle: 0 }, 10) {
        Ok(_) => 0,
        Err(err) => tls_code(err),
    };
    let invalid_int_wait_many = match tls_async_wait_many_int(vec.vec_of(AsyncIntOp { handle: 0 }), 10) {
        Ok(_) => 0,
        Err(err) => tls_code(err),
    };
    let invalid_string_wait_many = match tls_async_wait_many_string(vec.vec_of(AsyncStringOp { handle: 0 }), 10) {
        Ok(_) => 0,
        Err(err) => tls_code(err),
    };

    let bad = TlsStream { handle: 9999 };
    let send_path = match tls_async_send_submit(bad, bytes.from_string("x"), 100) {
        Ok(op) => match tls_async_wait_int(op, 1000) {
            Ok(_) => 0,
            Err(err) => tls_code(err),
        },
        Err(err) => tls_code(err),
    };
    let recv_path = match tls_async_recv_submit(bad, 8, 100) {
        Ok(op) => match tls_async_wait_string(op, 1000) {
            Ok(_) => 0,
            Err(err) => tls_code(err),
        },
        Err(err) => tls_code(err),
    };
    let shutdown_ok = match tls_async_shutdown() {
        Ok(value) => bool_to_int(value),
        Err(_) => 0,
    };

    let score = (if invalid_int_wait == 5 { 1 } else { 0 })
        + (if invalid_string_wait == 5 { 1 } else { 0 })
        + (if invalid_int_wait_many == 5 { 1 } else { 0 })
        + (if invalid_string_wait_many == 5 { 1 } else { 0 })
        + (if send_path == 5 { 1 } else { 0 })
        + (if recv_path == 5 { 1 } else { 0 })
        + shutdown_ok;
    if score == 7 {
        print_int(42);
    } else {
        print_int(score);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_secure_error_contract_maps_cross_module_negative_paths() {
    let src = r#"
import std.io;
import std.buffer;
import std.bytes;
import std.crypto;
import std.tls;
import std.secure_errors;
import std.string;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn main() -> Int effects { io, net } capabilities { io, net } {
    let b = new_buffer(1);
    let buffer_info = match buf_read_u8(b) {
        Ok(_) => secure_error_info("none", "NONE", "none", false),
        Err(err) => buffer_error_info(err),
    };

    let crypto_info = match base64_decode("%%%=") {
        Ok(_) => secure_error_info("none", "NONE", "none", false),
        Err(err) => crypto_error_info(err),
    };

    let bad = TlsStream { handle: 9999 };
    let tls_info = match tls_send_bytes(bad, bytes.from_string("x")) {
        Ok(_) => secure_error_info("none", "NONE", "none", false),
        Err(err) => tls_error_info(err),
    };

    let pool_err: PoolErrorContract = Timeout();
    let pool_info = pool_error_info(pool_err);

    let score = bool_to_int(string.contains(buffer_info.code, "BUF_"))
        + bool_to_int(string.contains(crypto_info.code, "CRYPTO_"))
        + bool_to_int(string.contains(tls_info.code, "TLS_"))
        + bool_to_int(pool_info.retryable);
    if score == 4 {
        print_int(42);
    } else {
        print_int(score);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn exec_postgres_tls_scram_replay_scenarios_are_deterministic() {
    let src = fs::read_to_string("examples/io/postgres_tls_scram_reference.aic")
        .expect("read examples/io/postgres_tls_scram_reference.aic");
    let replay_text = fs::read_to_string("docs/security-ops/postgres-tls-scram-replay.v1.json")
        .expect("read docs/security-ops/postgres-tls-scram-replay.v1.json");
    let replay: Value =
        serde_json::from_str(&replay_text).expect("parse postgres tls scram replay json");
    let scenarios = replay
        .get("scenarios")
        .and_then(|v| v.as_array())
        .expect("scenarios array");
    assert!(
        !scenarios.is_empty(),
        "replay contract must provide deterministic scenario cases"
    );

    for scenario in scenarios {
        let arg = scenario
            .get("arg")
            .and_then(|v| v.as_str())
            .expect("scenario arg");
        let expected = scenario
            .get("expected_print_int")
            .and_then(|v| v.as_i64())
            .expect("expected_print_int") as i32;
        let (code, stdout, stderr) = compile_and_run_with_args(&src, &[arg]);
        assert_eq!(code, 0, "scenario={arg} stderr={stderr}");
        assert_eq!(
            stdout,
            format!("{expected}\n"),
            "scenario={arg} expected deterministic replay output"
        );
    }

    let (suite_code, suite_stdout, suite_stderr) = compile_and_run(&src);
    assert_eq!(suite_code, 0, "stderr={suite_stderr}");
    assert_eq!(suite_stdout, "42\n");
}

#[test]
fn exec_tls_httpbin_https_handshake_or_deterministic_fallback() {
    let src = r#"
import std.io;
import std.tls;
import std.bytes;
import std.string;

fn none_string() -> Option[String] {
    None()
}

fn fallback(err: TlsError) -> Bool {
    match err {
        ProtocolError => true,
        Io => true,
        Timeout => true,
        _ => false,
    }
}

fn score_https(stream: TlsStream) -> Int effects { net } capabilities { net } {
    let wrote = match tls_send_bytes(
        stream,
        bytes.from_string("HEAD /anything HTTP/1.0\nHost: httpbin.org\n\n"),
    ) {
        Ok(sent) => sent > 0,
        Err(_) => false,
    };
    let recv_ok = match tls_recv_bytes(stream, 2048, 5000) {
        Ok(payload) => string.contains(bytes.to_string_lossy(payload), "HTTP/"),
        Err(_) => false,
    };
    let close_ok = match tls_close(stream) {
        Ok(closed) => closed,
        Err(_) => false,
    };
    if wrote && recv_ok && close_ok { 42 } else { 0 }
}

fn main() -> Int effects { io, net } capabilities { io, net } {
    let cfg = TlsConfig {
        verify_server: true,
        ca_cert_path: none_string(),
        client_cert_path: none_string(),
        client_key_path: none_string(),
        server_name: Some("httpbin.org"),
    };
    let result = match tls_connect_addr("httpbin.org:443", cfg, 5000) {
        Ok(stream) => score_https(stream),
        Err(err) => if fallback(err) { 43 } else { 0 },
    };
    print_int(result);
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(
        stdout == "42\n" || stdout == "43\n",
        "expected handshake success (42) or deterministic fallback (43), got stdout={stdout:?} stderr={stderr}"
    );
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_runtime_net_handle_limit_override_is_enforced() {
    let src = r#"
import std.io;
import std.net;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn net_code(err: NetError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        Refused => 3,
        Timeout => 4,
        AddressInUse => 5,
        InvalidInput => 6,
        Io => 7,
        ConnectionClosed => 8,
        Cancelled => 9,
    }
}

fn main() -> Int effects { io, net } capabilities { io, net } {
    let first = match tcp_listen("127.0.0.1:0") {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    let second_code = match tcp_listen("127.0.0.1:0") {
        Ok(handle) => if true {
            let ignored_close = tcp_close(handle);
            0
        } else {
            0
        },
        Err(err) => net_code(err),
    };
    let close_first = match tcp_close(first) {
        Ok(value) => bool_to_int(value),
        Err(_) => 0,
    };

    if first > 0 && second_code == 7 && close_first == 1 {
        print_int(42);
    } else {
        print_int(second_code * 10 + close_first);
    };
    0
}
"#;

    let envs = [("AIC_RT_LIMIT_NET_HANDLES", "1")];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_runtime_proc_handle_limit_override_is_enforced() {
    let src = r#"
import std.io;
import std.proc;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn proc_code(err: ProcError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        InvalidInput => 3,
        Io => 4,
        UnknownProcess => 5,
    }
}

fn main() -> Int effects { io, proc, env } capabilities { io, proc, env } {
    let first = spawn("sleep 5");

    let second_code = match spawn("sleep 5") {
        Ok(handle) => if true {
            let ignored_kill = kill(handle);
            0
        } else {
            0
        },
        Err(err) => proc_code(err),
    };

    let cleanup = match first {
        Ok(handle) => if true {
            let ignored_kill_first = kill(handle);
            1
        } else {
            1
        },
        Err(_) => 0,
    };

    if cleanup == 1 && second_code == 4 {
        print_int(42);
    } else {
        print_int(second_code * 10 + cleanup);
    };
    0
}
"#;

    let envs = [("AIC_RT_LIMIT_PROC_HANDLES", "1")];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_runtime_concurrency_limits_override_is_enforced() {
    let src = r#"
import std.io;
import std.concurrent;

fn conc_code(err: ConcurrencyError) -> Int {
    match err {
        NotFound => 1,
        Timeout => 2,
        Cancelled => 3,
        InvalidInput => 4,
        Panic => 5,
        Closed => 6,
        Io => 7,
    }
}

fn main() -> Int effects { io, concurrency } capabilities { io, concurrency } {
    let first_task = spawn_task(7, 2000);
    let second_task_code = match spawn_task(9, 2000) {
        Ok(task) => if true {
            let ignored_cancel = cancel_task(task);
            0
        } else {
            0
        },
        Err(err) => conc_code(err),
    };

    let first_channel = channel_int(1);
    let second_channel_code = match channel_int(1) {
        Ok(ch) => if true {
            let ignored_close_channel = close_channel(ch);
            0
        } else {
            0
        },
        Err(err) => conc_code(err),
    };

    let first_mutex = mutex_int(1);
    let second_mutex_code = match mutex_int(2) {
        Ok(m) => if true {
            let ignored_close_mutex = close_mutex(m);
            0
        } else {
            0
        },
        Err(err) => conc_code(err),
    };

    let cleanup_task = match first_task {
        Ok(task) => if true {
            let ignored_cleanup_cancel = cancel_task(task);
            1
        } else {
            0
        },
        Err(_) => 0,
    };
    let cleanup_channel = match first_channel {
        Ok(ch) => if true {
            let ignored_cleanup_channel = close_channel(ch);
            1
        } else {
            0
        },
        Err(_) => 0,
    };
    let cleanup_mutex = match first_mutex {
        Ok(m) => if true {
            let ignored_cleanup_mutex = close_mutex(m);
            1
        } else {
            0
        },
        Err(_) => 0,
    };

    if second_task_code == 7 && second_channel_code == 7 && second_mutex_code == 7 &&
        cleanup_task + cleanup_channel + cleanup_mutex == 3 {
        print_int(42);
    } else {
        print_int(second_task_code * 100 + second_channel_code * 10 + second_mutex_code);
    };
    0
}
"#;

    let envs = [
        ("AIC_RT_LIMIT_CONC_TASKS", "1"),
        ("AIC_RT_LIMIT_CONC_CHANNELS", "1"),
        ("AIC_RT_LIMIT_CONC_MUTEXES", "1"),
    ];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_runtime_net_async_lifecycle_sustained_churn_is_leak_free() {
    let src = r#"
import std.io;
import std.net;

fn bool_to_int(value: Bool) -> Int {
    if value { 1 } else { 0 }
}

fn net_code(err: NetError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        Refused => 3,
        Timeout => 4,
        AddressInUse => 5,
        InvalidInput => 6,
        Io => 7,
        ConnectionClosed => 8,
        Cancelled => 9,
    }
}

fn cycle_once() -> Int effects { net, concurrency } capabilities { net, concurrency } {
    let listener = match tcp_listen("127.0.0.1:0") {
        Ok(handle) => handle,
        Err(_) => 0,
    };
    if listener == 0 {
        0
    } else {
        let timeout_ok = match async_accept_submit(listener, 10) {
            Ok(op) => match async_wait_int(op, 200) {
                Ok(_) => 0,
                Err(err) => if net_code(err) == 4 { 1 } else { 0 },
            },
            Err(_) => 0,
        };

        let cancel_ok = match async_accept_submit(listener, 1000) {
            Ok(op) => if true {
                let cancelled = match async_cancel_int(op) {
                    Ok(value) => bool_to_int(value),
                    Err(_) => 0,
                };
                let waited = match async_wait_int(op, 200) {
                    Ok(_) => 0,
                    Err(err) => if net_code(err) == 9 { 1 } else { 0 },
                };
                if cancelled == 1 && waited == 1 { 1 } else { 0 }
            } else {
                0
            },
            Err(_) => 0,
        };

        let close_ok = match tcp_close(listener) {
            Ok(value) => bool_to_int(value),
            Err(_) => 0,
        };

        if timeout_ok + cancel_ok + close_ok == 3 { 1 } else { 0 }
    }
}

fn main() -> Int effects { io, net, concurrency } capabilities { io, net, concurrency } {
    let mut ok = 0;
    let mut i = 0;
    while i < 64 {
        ok = ok + cycle_once();
        i = i + 1;
    };

    let shutdown_ok = match async_shutdown() {
        Ok(value) => bool_to_int(value),
        Err(_) => 0,
    };

    if ok == 64 && shutdown_ok == 1 {
        print_int(42);
    } else {
        print_int(ok * 10 + shutdown_ok);
    };
    0
}
"#;

    let envs = [
        ("AIC_RT_LIMIT_NET_HANDLES", "8"),
        ("AIC_RT_LIMIT_NET_ASYNC_OPS", "8"),
        ("AIC_RT_LIMIT_NET_ASYNC_QUEUE", "8"),
    ];
    let (code, stdout, stderr) =
        compile_and_run_with_setup_and_args_and_input_and_env(src, &[], "", &envs, |_| {});
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[test]
fn exec_runtime_tls_async_lifecycle_sustained_churn_is_leak_free() {
    let backend_enabled = tls_backend_enabled_for_tests();
    let openssl_cli_available = openssl_cli_available_for_tests();
    if !(backend_enabled && openssl_cli_available) {
        return;
    }

    let src = r#"
import std.io;
import std.tls;
import std.env;

fn bool_to_int(value: Bool) -> Int {
    if value { 1 } else { 0 }
}

fn read_env_or(key: String, fallback: String) -> String effects { env } capabilities { env } {
    match env.get(key) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn none_string() -> Option[String] {
    None()
}

fn tls_code(err: TlsError) -> Int {
    match err {
        HandshakeFailed => 1,
        CertificateInvalid => 2,
        CertificateExpired => 3,
        HostnameMismatch => 4,
        ProtocolError => 5,
        ConnectionClosed => 6,
        Io => 7,
        Timeout => 8,
        Cancelled => 9,
    }
}

fn main() -> Int effects { io, env, net, concurrency } capabilities { io, env, net, concurrency } {
    let mut ok = 0;
    let mut i = 0;
    while i < 24 {
        let addr = read_env_or("AIC_TLS_ADDR", "127.0.0.1:65535");
        let cfg = TlsConfig {
            verify_server: false,
            ca_cert_path: none_string(),
            client_cert_path: none_string(),
            client_key_path: none_string(),
            server_name: Some("localhost"),
        };

        let cycle_ok = match tls_connect_addr(addr, cfg, 5000) {
            Err(_) => 0,
            Ok(stream) => if true {
                let op = match tls_async_recv_submit(stream, 64, 2000) {
                    Ok(value) => value,
                    Err(_) => AsyncStringOp { handle: 0 },
                };
                let handle = op.handle;
                let cancel_ok = match tls_async_cancel_string(AsyncStringOp { handle: handle }) {
                    Ok(value) => bool_to_int(value),
                    Err(_) => 0,
                };
                let wait_code = match tls_async_wait_string(AsyncStringOp { handle: handle }, 500) {
                    Ok(_) => 0,
                    Err(err) => tls_code(err),
                };
                let close_ok = match tls_close(stream) {
                    Ok(value) => bool_to_int(value),
                    Err(_) => 0,
                };
                if cancel_ok == 1 && wait_code == 9 && close_ok == 1 {
                    1
                } else {
                    0
                }
            } else {
                0
            },
        };

        ok = ok + cycle_ok;
        i = i + 1;
    };

    let shutdown_ok = match tls_async_shutdown() {
        Ok(value) => bool_to_int(value),
        Err(_) => 0,
    };

    if ok == 24 && shutdown_ok == 1 {
        print_int(42);
    } else {
        print_int(ok * 10 + shutdown_ok);
    };
    0
}
"#;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind tls churn listener");
    let port = listener.local_addr().expect("listener addr").port();
    drop(listener);

    let addr_env = format!("127.0.0.1:{port}");
    let envs = [
        ("AIC_TLS_ADDR", addr_env.as_str()),
        ("AIC_RT_LIMIT_TLS_HANDLES", "4"),
        ("AIC_RT_LIMIT_TLS_ASYNC_OPS", "4"),
        ("AIC_RT_LIMIT_NET_HANDLES", "8"),
    ];
    let (code, stdout, stderr) =
        compile_and_run_with_server_setup_and_args_and_input_and_env(src, &[], "", &envs, |root| {
            generate_local_tls_cert(root);
            let mut server = spawn_local_tls_server(root, port, true);
            wait_for_local_tls_server(port, &mut server);
            Some(server)
        });
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_runtime_pool_churn_sustained_cycles_are_leak_free() {
    let src = r#"
import std.io;
import std.pool;
import std.concurrent;

struct FakeConn {
    id: Int,
    healthy: Bool,
}

fn wait_ms(ms: Int) -> () effects { concurrency } capabilities { concurrency } {
    match spawn_task(1, ms) {
        Ok(task) => if true {
            let _joined = join_task(task);
            ()
        } else {
            ()
        },
        Err(_) => (),
    }
}

fn main() -> Int effects { io, concurrency } capabilities { io, concurrency } {
    let created: AtomicInt = atomic_int(0);
    let create_cb: Fn() -> Result[FakeConn, PoolError] = || -> Result[FakeConn, PoolError] {
        let prior = atomic_add(created, 1);
        Ok(FakeConn {
            id: prior + 1,
            healthy: true,
        })
    };
    let check_cb: Fn(FakeConn) -> Bool = |conn: FakeConn| -> Bool { conn.healthy };
    let destroy_cb: Fn(FakeConn) -> () = |conn: FakeConn| -> () { () };

    let pool_result: Result[Pool[FakeConn], PoolError] = new_pool(
        PoolConfig {
            min_size: 1,
            max_size: 2,
            acquire_timeout_ms: 30,
            idle_timeout_ms: 0,
            max_lifetime_ms: 0,
            health_check_ms: 0,
        },
        create_cb,
        check_cb,
        destroy_cb,
    );
    let pool: Pool[FakeConn] = match pool_result {
        Ok(p) => p,
        Err(_) => Pool { handle: 0 },
    };

    let mut release_ok = 0;
    let mut i = 0;
    while i < 100 {
        let released = match acquire(pool) {
            Ok(conn) => if true {
                release(conn);
                1
            } else {
                0
            },
            Err(_) => 0,
        };
        release_ok = release_ok + released;
        i = i + 1;
    };

    let stats = pool_stats(pool);
    close_pool(pool);

    if release_ok == 100 && stats.in_use == 0 && stats.total <= 2 {
        print_int(42);
    } else {
        print_int(release_ok * 10 + stats.total);
    };
    0
}
"#;

    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[test]
fn exec_frontend_accepts_int128_uint128_boundary_literals() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("int128_frontend_ok.aic");
    let source = r#"
fn main() -> Int {
    let signed_min: Int128 = -170141183460469231731687303715884105728i128;
    let unsigned_max: UInt128 = 340282366920938463463374607431768211455u128;
    let signed_step: Int128 = signed_min + 1i128;
    let unsigned_step: UInt128 = unsigned_max - 1u128;
    if signed_step < 0i128 && unsigned_step > 0u128 {
        0
    } else {
        1
    }
}
"#;
    fs::write(&src, source).expect("write source");
    let front = run_frontend(&src).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics: {:#?}",
        front.diagnostics
    );
}

#[test]
fn exec_frontend_reports_int128_uint128_range_failures() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("int128_frontend_fail.aic");
    let source = r#"
fn main() -> Int {
    let bad_signed: Int128 = 170141183460469231731687303715884105728i128;
    let bad_unsigned_neg: UInt128 = -1u128;
    if bad_signed == 0i128 || bad_unsigned_neg == 0u128 {
        1
    } else {
        0
    }
}
"#;
    fs::write(&src, source).expect("write source");
    let front = run_frontend(&src).expect("frontend");
    assert!(
        has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    assert!(
        front
            .diagnostics
            .iter()
            .any(|d| d.code == "E1204" && d.message.contains("Int128")),
        "diagnostics={:#?}",
        front.diagnostics
    );
    assert!(
        front
            .diagnostics
            .iter()
            .any(|d| d.code == "E1204" && d.message.contains("UInt128")),
        "diagnostics={:#?}",
        front.diagnostics
    );
}

#[test]
fn exec_size_integer_family_runtime_and_uint_alias_behavior() {
    let src = r#"
import std.io;

fn main() -> Int effects { io } capabilities { io  } {
    let signed: ISize = -5;
    let signed_abs: ISize = 5;
    let zero: ISize = 0;
    let unsigned: USize = 47u64;
    let alias_unsigned: UInt = unsigned;
    let one: USize = 1;
    let expected_sum: USize = 48;
    let sum: USize = alias_unsigned + one;

    if signed + signed_abs == zero && sum == expected_sum {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n", "stderr={stderr}");
}

#[test]
fn exec_frontend_rejects_size_integer_sign_change_and_kind_mismatch() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("size_integer_frontend_fail.aic");
    let source = r#"
fn main(a: USize, b: Int) -> Int {
    let _bad_to_int: Int = a;
    let _bad_to_usize: USize = b;
    let _bad_op = a + b;
    0
}
"#;
    fs::write(&src, source).expect("write source");
    let front = run_frontend(&src).expect("frontend");
    assert!(
        has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    assert!(
        front.diagnostics.iter().any(|d| d.code == "E1204"),
        "diagnostics={:#?}",
        front.diagnostics
    );
    assert!(
        front.diagnostics.iter().any(|d| d.code == "E1230"),
        "diagnostics={:#?}",
        front.diagnostics
    );
}
