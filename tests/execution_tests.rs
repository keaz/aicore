use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use aicore::codegen::{
    compile_with_clang, compile_with_clang_artifact, compile_with_clang_artifact_with_options,
    emit_llvm, emit_llvm_with_options, ArtifactKind, CodegenOptions, CompileOptions,
    OptimizationLevel,
};
use aicore::contracts::lower_runtime_asserts;
use aicore::driver::{has_errors, run_frontend};
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

fn assert_set_ops_succeeds_or_reports_string_key_limit(source: &str) {
    match compile_and_run_or_backend_diags(source) {
        Ok((code, stdout, stderr)) => {
            assert_eq!(code, 0, "stderr={stderr}");
            assert_eq!(stdout, "42\n");
        }
        Err(diags) => {
            assert!(
                diags.iter().any(|d| {
                    d.code == "E5011"
                        && (d.message.contains("String key")
                            || d.message.contains("String keys only"))
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

fn err_code(err: NetError) -> Int {
    match err {
        NotFound => 1,
        PermissionDenied => 2,
        Refused => 3,
        Timeout => 4,
        AddressInUse => 5,
        InvalidInput => 6,
        Io => 7,
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
    let ok0 = if len(out0) == 10 && starts_with(out0, "plain") { 1 } else { 0 };

    let out1 = format("x{0}y", split("A", ","));
    let ok1 = if len(out1) == 3 && string.contains(out1, "A") { 1 } else { 0 };

    let out2 = format("{0}-{1}", split("left,right", ","));
    let ok2 = if starts_with(out2, "left-") && ends_with(out2, "right") { 1 } else { 0 };

    let out5 = format("{0}{1}{2}{3}{4}", split("a,b,c,d,e", ","));
    let ok5 = if len(out5) == 5 && starts_with(out5, "ab") && ends_with(out5, "de") {
        1
    } else {
        0
    };

    let missing = format("x{0}-{2}-z", split("left,right", ","));
    let missing_ok =
        if starts_with(missing, "xleft-") && string.contains(missing, "{2}") && ends_with(missing, "-z") {
            1
        } else {
            0
        };

    let int_text = int_to_string(-2048);
    let int_direct_ok = if len(int_text) == 5 && starts_with(int_text, "-") { 1 } else { 0 };
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
    let bool_compose_ok = if starts_with(format("{0}|{1}", bool_args), "up|") { 1 } else { 0 };

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
        if len(basic) == 10 && starts_with(basic, "Hello,") && ends_with(basic, "Ada") {
            1
        } else {
            0
        };

    let nested = f"sum={int_to_string(20 + 22)}";
    let nested_ok = if len(nested) == 6 && starts_with(nested, "sum=") && ends_with(nested, "42") {
        1
    } else {
        0
    };

    let escaped = f"left \{literal\} right";
    let escaped_ok =
        if len(escaped) == 20 && starts_with(escaped, "left {") && ends_with(escaped, "} right") {
            1
        } else {
            0
        };

    let mixed = $"<{name}:{int_to_string(7)}>";
    let mixed_ok = if len(mixed) == 7 && starts_with(mixed, "<Ada:") && ends_with(mixed, "7>") {
        1
    } else {
        0
    };
    if basic_ok + nested_ok + escaped_ok + mixed_ok == 4 {
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
    let starts_ok = if starts_with("alpha beta", "alpha") { 1 } else { 0 };
    let ends_ok = if ends_with("alpha beta", "beta") { 1 } else { 0 };

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
        if len(invalid_lossy) == 6 && starts_with(invalid_lossy, "fo") && ends_with(invalid_lossy, "o") {
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
    let method_ok = if starts_with(request_line, "GET ") { 1 } else { 0 };
    let target_ok = if string.contains(request_line, "/api/users") { 1 } else { 0 };
    let version_ok = if ends_with(request_line, "HTTP/1.1") { 1 } else { 0 };

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
    let payload_ok = if len(payload_text) == 6 && starts_with(payload_text, "ab") && ends_with(payload_text, "XYZ") {
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
            Some(line) => if starts_with(line, prefix) { 1 } else { 0 },
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
            if starts_with(text, "alpha")
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
        if starts_with(text, "BA")
            && ends_with(text, "CD")
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
        Some(v) => if len(v) == 5 && starts_with(v, "alpha") { 1 } else { 0 },
        None => 0,
    };
    let third_ok = match arg_at(2) {
        Some(v) => if len(v) == 4 && starts_with(v, "beta") { 1 } else { 0 },
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
    let os_linux = if len(os) == 5 && starts_with(os, "linux") { 1 } else { 0 };
    let os_macos = if len(os) == 5 && starts_with(os, "macos") { 1 } else { 0 };
    let os_windows = if len(os) == 7 && starts_with(os, "windows") { 1 } else { 0 };
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
fn exec_set_int_ops_work_or_emit_string_key_diagnostic() {
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

    assert_set_ops_succeeds_or_reports_string_key_limit(src);
}

#[test]
fn exec_set_bool_ops_work_or_emit_string_key_diagnostic() {
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

    assert_set_ops_succeeds_or_reports_string_key_limit(src);
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

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_concurrency_worker_pool_is_deterministic() {
    let src = r#"
import std.io;
import std.concurrent;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn unwrap_task(v: Result[Task, ConcurrencyError]) -> Task {
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

fn main() -> Int effects { io, concurrency } capabilities { io, concurrency  } {
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

fn unwrap_task(v: Result[Task, ConcurrencyError]) -> Task {
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

fn main() -> Int effects { io, concurrency } capabilities { io, concurrency  } {
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

fn main() -> Int effects { io, concurrency } capabilities { io, concurrency  } {
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

fn unwrap_task(v: Result[Task, ConcurrencyError]) -> Task {
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
    let mut race: Vec[Task] = vec.new_vec();
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

    let empty_tasks: Vec[Task] = vec.new_vec();
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
    let mut tasks: Vec[Task] = vec.new_vec();
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
        Err(_) => ProcOutput { status: 99, stdout: "", stderr: "" },
    };
    let pipe_out = match pipe("echo 42", "cat") {
        Ok(out) => out,
        Err(_) => ProcOutput { status: 99, stdout: "", stderr: "" },
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

    let run_status_ok = if run_out.status == 7 { 1 } else { 0 };
    let run_stdout_ok = if len(run_out.stdout) > 0 { 1 } else { 0 };
    let run_stderr_ok = if len(run_out.stderr) > 0 { 1 } else { 0 };
    let pipe_ok = if pipe_out.status == 0 && len(pipe_out.stdout) > 0 { 1 } else { 0 };
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
        Err(_) => ProcOutput { status: 99, stdout: "", stderr: "" },
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
        Err(_) => ProcOutput { status: 99, stdout: "", stderr: "" },
    };
    let chain_ok = if chained.status == 0 && string.contains(chained.stdout, "BETA") { 1 } else { 0 };
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

#[cfg(not(target_os = "windows"))]
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
fn exec_net_udp_and_dns_helpers() {
    let src = r#"
import std.io;
import std.net;
import std.string;
import std.bytes;

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
    let reverse_checked = match dns_reverse("127.0.0.1") {
        Ok(name) => if len(name) > 0 { 1 } else { 0 },
        Err(err) => match err {
            NotFound => 1,
            _ => 0,
        },
    };

    if sent == 4 && bytes.byte_len(packet.payload) == 4 && len(packet.from) > 0 &&
        len(lookup) > 0 && reverse_checked == 1 && closed_sender + closed_receiver == 2 {
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
        Io => 7,
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
        Io => 7,
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

#[cfg(not(target_os = "windows"))]
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
    let close_ok = if close_count == 3 { 1 } else { 0 };
    let score = invalid_int_ok + invalid_string_ok + accept_timeout_ok +
        accept_rewait_ok + recv_timeout_ok + sent_ok + recv_ok + recv_rewait_ok +
        shutdown_ok + close_ok;

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
        ends_with(wire_text, "ok") {
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
        Err(_) => 0,
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
    let a = 3.5;
    let b = 2.0;
    let sum = a + b;
    let total = sum * 2.0;

    let gt_ok = if total > 10.0 { 1 } else { 0 };
    let eq_ok = if total == 11.0 { 1 } else { 0 };
    let lt_ok = if a < b { 0 } else { 1 };
    let ge_ok = if total >= 11.0 { 1 } else { 0 };

    if gt_ok + eq_ok + lt_ok + ge_ok == 4 {
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
import std.string;

fn main() -> Int effects { io } capabilities { io  } {
    let parsed = match parse_float("3.125") {
        Ok(v) => v,
        Err(_) => 0.0,
    };
    let text = float_to_string(parsed);
    let reparsed = match parse_float(text) {
        Ok(v) => v,
        Err(_) => 0.0,
    };
    let from_json = match decode_float(encode_float(reparsed)) {
        Ok(v) => v,
        Err(_) => 0.0,
    };
    let good_value = if from_json == 3.125 { 1 } else { 0 };
    let parse_bad = match parse_float("nan") {
        Ok(_) => 0,
        Err(_) => 1,
    };
    let decode_bad = match decode_float(encode_string("abc")) {
        Ok(_) => 0,
        Err(_) => 1,
    };

    if good_value == 1 && parse_bad == 1 && decode_bad == 1 {
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
    let fallback = Url {
        scheme: "",
        host: "",
        port: -1,
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
            normalized_url.port == -1 &&
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
    let bad_path = match normalize(Url {
        scheme: "http",
        host: "example.com",
        port: 80,
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
        Ok(text) => if len(text) == len(expected) && starts_with(text, expected) { 1 } else { 0 },
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
        Ok(text) => if len(text) == 5 && starts_with(text, "Alice") { 1 } else { 0 },
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
        Ok(text) => if len(text) == 10 && starts_with(text, "payload-42") { 1 } else { 0 },
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
        Ok(text) => if len(text) == 4 && starts_with(text, "echo") { 1 } else { 0 },
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
