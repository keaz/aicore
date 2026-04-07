use std::fs;
use std::process::{Command, Stdio};

use aicore::codegen::{compile_with_clang, emit_llvm};
use aicore::contracts::lower_runtime_asserts;
use aicore::diagnostics::Diagnostic;
use aicore::driver::{has_errors, run_frontend};
use tempfile::tempdir;

fn compile_and_run(source: &str) -> (i32, String, String) {
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

    let output = Command::new(exe)
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run exe");

    (
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn backend_diagnostics(source: &str) -> Vec<Diagnostic> {
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
    match emit_llvm(&lowered, &src.to_string_lossy()) {
        Ok(_) => panic!("expected backend diagnostics"),
        Err(diags) => diags,
    }
}

fn assert_prints_42(source: &str) {
    let (code, stdout, stderr) = compile_and_run(source);
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(stdout, "42\n");
}

#[test]
fn map_u64_bool_backend_paths_work() {
    let src = r#"
import std.io;
import std.map;
import std.option;
import std.vec;

fn bool_to_int(v: Bool) -> Int {
    if v { 1 } else { 0 }
}

fn opt_bool_to_int(v: Option[Bool], fallback: Int) -> Int {
    match v {
        Some(value) => bool_to_int(value),
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let m0: Map[UInt64, Bool] = map.new_map();
    let k7: UInt64 = 7u64;
    let k42: UInt64 = 42u64;
    let k99: UInt64 = 99u64;
    let m1 = map.insert(m0, k7, true);
    let m2 = map.insert(m1, k42, false);
    let m3 = map.insert(m2, k99, true);
    let m4 = map.remove(m3, k42);

    let has_99 = if map.contains_key(m4, k99) { 1 } else { 0 };
    let missing_42 = if map.contains_key(m4, k42) { 0 } else { 1 };
    let value_99 = opt_bool_to_int(map.get(m4, k99), 0);

    let keys = map.keys(m4);
    let key0 = match vec.get(keys, 0) { Some(v) => if v == k7 { 1 } else { 0 }, None => 0 };
    let key1 = match vec.get(keys, 1) { Some(v) => if v == k99 { 1 } else { 0 }, None => 0 };

    let values = map.values(m4);
    let values_len_ok = if vec.vec_len(values) == 2 { 1 } else { 0 };

    let entries = map.entries(m4);
    let entries_len_ok = if vec.vec_len(entries) == 2 { 1 } else { 0 };

    if has_99 + missing_42 + value_99 + key0 + key1 + values_len_ok + entries_len_ok == 7 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    assert_prints_42(src);
}

#[test]
fn map_bytes_string_backend_paths_work() {
    let src = r#"
import std.bytes;
import std.io;
import std.map;
import std.option;
import std.string;
import std.vec;

fn opt_text_or(v: Option[String], fallback: String) -> String {
    match v {
        Some(value) => value,
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let m0: Map[Bytes, String] = map.new_map();
    let m1 = map.insert(m0, bytes.from_string("bb"), "two");
    let m2 = map.insert(m1, bytes.from_string("aa"), "one");
    let m3 = map.insert(m2, bytes.from_string("cc"), "three");
    let m4 = map.remove(m3, bytes.from_string("bb"));

    let has_aa = if map.contains_key(m4, bytes.from_string("aa")) { 1 } else { 0 };
    let missing_bb = if map.contains_key(m4, bytes.from_string("bb")) { 0 } else { 1 };
    let value_aa = opt_text_or(map.get(m4, bytes.from_string("aa")), "");

    let keys = map.keys(m4);
    let key0 = match vec.get(keys, 0) {
        Some(value) => if bytes.compare_bytes(value, bytes.from_string("aa")) == 0 { 1 } else { 0 },
        None => 0,
    };
    let key1 = match vec.get(keys, 1) {
        Some(value) => if bytes.compare_bytes(value, bytes.from_string("cc")) == 0 { 1 } else { 0 },
        None => 0,
    };

    let values = map.values(m4);
    let value0 = match vec.get(values, 0) { Some(v) => if v == "one" { 1 } else { 0 }, None => 0 };
    let value1 = match vec.get(values, 1) { Some(v) => if v == "three" { 1 } else { 0 }, None => 0 };

    let entries = map.entries(m4);
    let entry0 = match vec.get(entries, 0) {
        Some(entry) => if bytes.compare_bytes(entry.key, bytes.from_string("aa")) == 0 && entry.value == "one" { 1 } else { 0 },
        None => 0,
    };
    let entry1 = match vec.get(entries, 1) {
        Some(entry) => if bytes.compare_bytes(entry.key, bytes.from_string("cc")) == 0 && entry.value == "three" { 1 } else { 0 },
        None => 0,
    };

    if has_aa + missing_bb + key0 + key1 + value0 + value1 + entry0 + entry1 == 8 && len(value_aa) == 3 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    assert_prints_42(src);
}

#[test]
fn map_string_bytes_backend_paths_work() {
    let src = r#"
import std.bytes;
import std.io;
import std.map;
import std.option;
import std.vec;

fn opt_bytes_eq_one(v: Option[Bytes], fallback: Int) -> Int {
    match v {
        Some(value) => if bytes.compare_bytes(value, bytes.from_string("one")) == 0 { 1 } else { 0 },
        None => fallback,
    }
}

fn main() -> Int effects { io, env } capabilities { io, env } {
    let m0: Map[String, Bytes] = map.new_map();
    let m1 = map.insert(m0, "bb", bytes.from_string("two"));
    let m2 = map.insert(m1, "aa", bytes.from_string("one"));
    let m3 = map.insert(m2, "cc", bytes.from_string("three"));
    let m4 = map.remove(m3, "bb");

    let has_aa = if map.contains_key(m4, "aa") { 1 } else { 0 };
    let missing_bb = if map.contains_key(m4, "bb") { 0 } else { 1 };
    let value_aa = opt_bytes_eq_one(map.get(m4, "aa"), 0);

    let keys = map.keys(m4);
    let key0 = match vec.get(keys, 0) { Some(v) => if v == "aa" { 1 } else { 0 }, None => 0 };
    let key1 = match vec.get(keys, 1) { Some(v) => if v == "cc" { 1 } else { 0 }, None => 0 };

    let values = map.values(m4);
    let value0 = match vec.get(values, 0) {
        Some(v) => if bytes.compare_bytes(v, bytes.from_string("one")) == 0 { 1 } else { 0 },
        None => 0,
    };
    let value1 = match vec.get(values, 1) {
        Some(v) => if bytes.compare_bytes(v, bytes.from_string("three")) == 0 { 1 } else { 0 },
        None => 0,
    };

    let entries = map.entries(m4);
    let entry0 = match vec.get(entries, 0) {
        Some(entry) => if entry.key == "aa" && bytes.compare_bytes(entry.value, bytes.from_string("one")) == 0 { 1 } else { 0 },
        None => 0,
    };
    let entry1 = match vec.get(entries, 1) {
        Some(entry) => if entry.key == "cc" && bytes.compare_bytes(entry.value, bytes.from_string("three")) == 0 { 1 } else { 0 },
        None => 0,
    };

    if has_aa + missing_bb + value_aa + key0 + key1 + value0 + value1 + entry0 + entry1 == 9 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    assert_prints_42(src);
}

#[test]
fn map_bytes_bytes_backend_paths_work() {
    let src = r#"
import std.bytes;
import std.io;
import std.map;
import std.option;
import std.vec;

fn main() -> Int effects { io, env } capabilities { io, env } {
    let m0: Map[Bytes, Bytes] = map.new_map();
    let m1 = map.insert(m0, bytes.from_string("bb"), bytes.from_string("two"));
    let m2 = map.insert(m1, bytes.from_string("aa"), bytes.from_string("one"));
    let m3 = map.insert(m2, bytes.from_string("cc"), bytes.from_string("three"));
    let m4 = map.remove(m3, bytes.from_string("bb"));

    let has_aa = if map.contains_key(m4, bytes.from_string("aa")) { 1 } else { 0 };
    let missing_bb = if map.contains_key(m4, bytes.from_string("bb")) { 0 } else { 1 };
    let value_aa = match map.get(m4, bytes.from_string("aa")) {
        Some(value) => if bytes.compare_bytes(value, bytes.from_string("one")) == 0 { 1 } else { 0 },
        None => 0,
    };

    let keys = map.keys(m4);
    let key0 = match vec.get(keys, 0) {
        Some(value) => if bytes.compare_bytes(value, bytes.from_string("aa")) == 0 { 1 } else { 0 },
        None => 0,
    };
    let key1 = match vec.get(keys, 1) {
        Some(value) => if bytes.compare_bytes(value, bytes.from_string("cc")) == 0 { 1 } else { 0 },
        None => 0,
    };

    let values = map.values(m4);
    let value0 = match vec.get(values, 0) {
        Some(value) => if bytes.compare_bytes(value, bytes.from_string("one")) == 0 { 1 } else { 0 },
        None => 0,
    };
    let value1 = match vec.get(values, 1) {
        Some(value) => if bytes.compare_bytes(value, bytes.from_string("three")) == 0 { 1 } else { 0 },
        None => 0,
    };

    let entries = map.entries(m4);
    let entry0 = match vec.get(entries, 0) {
        Some(entry) => if bytes.compare_bytes(entry.key, bytes.from_string("aa")) == 0 && bytes.compare_bytes(entry.value, bytes.from_string("one")) == 0 { 1 } else { 0 },
        None => 0,
    };
    let entry1 = match vec.get(entries, 1) {
        Some(entry) => if bytes.compare_bytes(entry.key, bytes.from_string("cc")) == 0 && bytes.compare_bytes(entry.value, bytes.from_string("three")) == 0 { 1 } else { 0 },
        None => 0,
    };

    if has_aa + missing_bb + value_aa + key0 + key1 + value0 + value1 + entry0 + entry1 == 9 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    assert_prints_42(src);
}

#[test]
fn map_u64_bytes_backend_paths_work() {
    let src = r#"
import std.bytes;
import std.io;
import std.map;
import std.option;
import std.vec;

fn main() -> Int effects { io, env } capabilities { io, env } {
    let m0: Map[UInt64, Bytes] = map.new_map();
    let k7: UInt64 = 7u64;
    let k42: UInt64 = 42u64;
    let k99: UInt64 = 99u64;
    let m1 = map.insert(m0, k7, bytes.from_string("two"));
    let m2 = map.insert(m1, k42, bytes.from_string("one"));
    let m3 = map.insert(m2, k99, bytes.from_string("three"));
    let m4 = map.remove(m3, k42);

    let has_99 = if map.contains_key(m4, k99) { 1 } else { 0 };
    let missing_42 = if map.contains_key(m4, k42) { 0 } else { 1 };
    let value_7 = match map.get(m4, k7) {
        Some(value) => if bytes.compare_bytes(value, bytes.from_string("two")) == 0 { 1 } else { 0 },
        None => 0,
    };

    let keys = map.keys(m4);
    let key0 = match vec.get(keys, 0) { Some(v) => if v == k7 { 1 } else { 0 }, None => 0 };
    let key1 = match vec.get(keys, 1) { Some(v) => if v == k99 { 1 } else { 0 }, None => 0 };

    let values = map.values(m4);
    let value0 = match vec.get(values, 0) {
        Some(v) => if bytes.compare_bytes(v, bytes.from_string("two")) == 0 { 1 } else { 0 },
        None => 0,
    };
    let value1 = match vec.get(values, 1) {
        Some(v) => if bytes.compare_bytes(v, bytes.from_string("three")) == 0 { 1 } else { 0 },
        None => 0,
    };

    let entries = map.entries(m4);
    let entry0 = match vec.get(entries, 0) {
        Some(entry) => if entry.key == k7 && bytes.compare_bytes(entry.value, bytes.from_string("two")) == 0 { 1 } else { 0 },
        None => 0,
    };
    let entry1 = match vec.get(entries, 1) {
        Some(entry) => if entry.key == k99 && bytes.compare_bytes(entry.value, bytes.from_string("three")) == 0 { 1 } else { 0 },
        None => 0,
    };

    if has_99 + missing_42 + value_7 + key0 + key1 + value0 + value1 + entry0 + entry1 == 9 {
        print_int(42);
    } else {
        print_int(0);
    };
    0
}
"#;

    assert_prints_42(src);
}

#[test]
fn map_unsupported_shape_keeps_e5011_deterministic() {
    let src = r#"
import std.io;
import std.map;

fn main() -> Int effects { io, env } capabilities { io, env } {
    let _m: Map[String, Float64] = map.new_map();
    0
}
"#;

    let diags1 = backend_diagnostics(src);
    let diags2 = backend_diagnostics(src);
    let summary1: Vec<(String, String)> = diags1
        .iter()
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect();
    let summary2: Vec<(String, String)> = diags2
        .iter()
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect();

    assert_eq!(summary1, summary2);
    assert!(summary1.iter().any(|(code, message)| {
        code == "E5011"
            && message.contains("supports only map values String, Bytes, Int, Bool, and UInt64")
    }));
    assert!(
        !summary1
            .iter()
            .any(|(code, message)| code == "E5001" && message.contains("unknown local variable")),
        "unsupported map shapes must not cascade into unknown-local diagnostics: {summary1:#?}"
    );
}

#[test]
fn set_unsupported_float_keys_report_e5011_without_unknown_local_cascade() {
    let src = r#"
import std.io;
import std.set;

fn main() -> Int effects { io } capabilities { io } {
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

    let diags = backend_diagnostics(src);
    assert!(diags.iter().any(|d| {
        d.code == "E5011"
            && d.message
                .contains("supports only map keys String, Bytes, Int, UInt64, and Bool")
    }));
    assert!(
        !diags
            .iter()
            .any(|d| d.code == "E5001" && d.message.contains("unknown local variable")),
        "unsupported set key diagnostics must stay focused: {diags:#?}"
    );
}
