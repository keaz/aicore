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
fn exec_while_and_continue_flow() {
    let src = r#"
import std.io;

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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
fn exec_regex_compile_match_find_replace() {
    let src = r#"
import std.io;
import std.regex;
import std.string;

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

fn main() -> Int effects { io } {
    let re = match compile_with_flags("^error [0-9]+$", flag_case_insensitive() + flag_multiline()) {
        Ok(value) => value,
        Err(_) => Regex { pattern: "", flags: 0 },
    };

    let match_yes = bool_result(is_match(re, "ERROR 42"));
    let match_no = bool_result(is_match(re, "info 42"));
    let found_len = string_len(find(re, "warn\nerror 17\nok"));
    let replaced_len = string_len(replace(re, "warn\nERROR 17\nok", "<redacted>"));
    let replace_nomatch_len = string_len(replace(re, "all good", "<x>"));

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

fn main() -> Int effects { io } {
    let bad = invalid_pattern(compile_with_flags("[unterminated", no_flags()));
    let unsupported = unsupported_flags(
        compile_with_flags("a.*b", flag_multiline() + flag_dot_matches_newline())
    );
    let re = match compile("error") {
        Ok(value) => value,
        Err(_) => Regex { pattern: "error", flags: 0 },
    };
    let miss = no_match(find(re, "all good"));

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

#[test]
fn exec_time_helpers_are_predictable() {
    let src = r#"
import std.io;
import std.time;

fn main() -> Int effects { io, time } {
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

fn main() -> Int effects { io, time } {
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

fn main() -> Int effects { io, time } {
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

fn main() -> Int effects { io, rand } {
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

fn main() -> Int effects { io, concurrency } {
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

fn main() -> Int effects { io, concurrency } {
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

#[cfg(not(target_os = "windows"))]
#[test]
fn exec_net_tcp_loopback_echo() {
    let src = r#"
import std.io;
import std.net;
import std.string;

fn main() -> Int effects { io, net } {
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
    let sent = match tcp_send(client, "ping") {
        Ok(n) => n,
        Err(_) => 0,
    };
    let received = match tcp_recv(server, 16, 1000) {
        Ok(text) => text,
        Err(_) => "",
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

    if sent == 4 && len(received) == 4 && closed_client + closed_server + closed_listener == 3 {
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

fn main() -> Int effects { io, net } {
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
    let sent = match udp_send_to(sender, receiver_addr, "pong") {
        Ok(n) => n,
        Err(_) => 0,
    };
    let packet = match udp_recv_from(receiver, 64, 1000) {
        Ok(value) => value,
        Err(_) => UdpPacket { from: "", payload: "" },
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

    if sent == 4 && len(packet.payload) == 4 && len(packet.from) > 0 &&
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

fn main() -> Int effects { io, net } {
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

#[test]
fn exec_json_roundtrip_and_object_operations() {
    let src = r#"
import std.io;
import std.json;
import std.string;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io, rand } {
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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

fn main() -> Int effects { io } {
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
    let expected_stdout = format!("{expected}\n{expected}\n");
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

fn main() -> Int effects { io, net } {
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

fn main() -> Int effects { io } {
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
