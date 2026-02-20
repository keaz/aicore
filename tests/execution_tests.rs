use std::fs;
use std::path::Path;
use std::process::Command;

use aicore::codegen::{
    compile_with_clang, compile_with_clang_artifact, compile_with_clang_artifact_with_options,
    emit_llvm, emit_llvm_with_options, ArtifactKind, CodegenOptions, CompileOptions,
};
use aicore::contracts::lower_runtime_asserts;
use aicore::driver::{has_errors, run_frontend};
use tempfile::tempdir;

fn compile_and_run_with_setup<F>(source: &str, setup: F) -> (i32, String, String)
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
    let output = Command::new(exe)
        .current_dir(dir.path())
        .output()
        .expect("run exe");
    (
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn compile_and_run(source: &str) -> (i32, String, String) {
    compile_and_run_with_setup(source, |_| {})
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

#[test]
fn exec_option_match() {
    let src = r#"
import std.io;

fn maybe_even(x: Int) -> Option[Int] {
    if x % 2 == 0 { Some(x) } else { None() }
}

fn main() -> Int effects { io } {
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
fn exec_async_ping_flow() {
    let src = r#"
import std.io;

async fn ping(x: Int) -> Int {
    x + 1
}

async fn main() -> Int effects { io } {
    let value = await ping(41);
    print_int(value);
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

fn main() -> Int effects { io } {
    print_int(pick(42, 7));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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
fn exec_abs_if_expression() {
    let src = r#"
import std.io;

fn abs(x: Int) -> Int {
    if x >= 0 { x } else { 0 - x }
}

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
    print_int(len("abc"));
    0
}
"#;
    let (code, stdout, stderr) = compile_and_run(src);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "3\n");
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

fn main() -> Int effects { io, fs } {
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

fn main() -> Int effects { io, fs } {
    let tmp_file = match temp_file("aic_io_test_") {
        Ok(path) => path,
        Err(_) => "",
    };
    let tmp_dir = match temp_dir("aic_io_test_") {
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

fn main() -> Int effects { io, fs } {
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

fn main() -> Int effects { io, fs } {
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

fn main() -> Int effects { io, env, fs } {
    let original = match cwd() {
        Ok(path) => path,
        Err(_) => "",
    };
    let set_ok = ok_bool(set("AIC_EXEC_ENV_KEY", "value-xyz"));
    let got_len = match get("AIC_EXEC_ENV_KEY") {
        Ok(value) => len(value),
        Err(_) => 0,
    };
    let rm_ok = ok_bool(remove("AIC_EXEC_ENV_KEY"));
    let missing_ok = match get("AIC_EXEC_ENV_KEY") {
        Ok(_) => 0,
        Err(err) => match err {
            NotFound => 1,
            _ => 0,
        },
    };
    let cwd_set_ok = ok_bool(set_cwd("."));
    let now = match cwd() {
        Ok(path) => path,
        Err(_) => "",
    };
    let joined = join(now, "alpha.txt");
    let base_len = len(basename(joined));
    let dir_len = len(dirname(joined));
    let ext_len = len(extension(joined));
    let abs_ok = if is_abs(now) { 1 } else { 0 };
    let restore_ok = ok_bool(set_cwd(original));

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

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_proc_run_pipe_spawn_wait_and_kill() {
    let src = r#"
import std.io;
import std.proc;
import std.string;

fn main() -> Int effects { io, proc, env } {
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
fn exec_debug_build_reports_panic_source_line() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("panic_line_map.aic");
    let source = r#"import std.io;

fn main() -> Int effects { io } {
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
        CompileOptions { debug_info: true },
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
fn exec_contract_failure() {
    let src = r#"
import std.io;

fn bad(x: Int) -> Int ensures result >= 0 {
    x
}

fn main() -> Int effects { io } {
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
