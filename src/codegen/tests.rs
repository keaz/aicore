use std::fs;

use sha2::{Digest, Sha256};
use tempfile::tempdir;

use crate::{
    contracts::lower_runtime_asserts,
    driver::{has_errors, run_frontend},
    ir_builder::build,
    parser::parse,
};

use super::{
    emit_llvm, emit_llvm_with_options, ensure_supported_toolchain,
    ensure_supported_toolchain_with_pin, instrument_llvm_for_leak_tracking, normalize_macos_uuid,
    parse_llvm_major, rewrite_wasm_entry_wrapper, runtime_c_source, runtime_compile_flags,
    target_is_wasm, CodegenOptions, CompileOptions, OptimizationLevel,
    RuntimeInstrumentationOptions, ToolchainInfo,
};

#[test]
fn emits_basic_llvm() {
    let src = "import std.io; fn main() -> Int effects { io } { print_int(1); 0 }";
    let (program, d) = parse(src, "test.aic");
    assert!(d.is_empty());
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "test.aic").expect("llvm");
    assert!(output.llvm_ir.contains("define i64 @aic_main()"));
}

#[test]
fn fixed_width_integer_functions_keep_exact_llvm_types() {
    let src = r#"
fn add_i8(a: Int8, b: Int8) -> Int8 { a + b }
fn add_u16(a: UInt16, b: UInt16) -> UInt16 { a + b }
fn div_u32(a: UInt32, b: UInt32) -> UInt32 { a / b }
fn div_i16(a: Int16, b: Int16) -> Int16 { a / b }
fn cmp_u32(a: UInt32, b: UInt32) -> Bool { a < b }
fn shr_u16(a: UInt16, b: UInt16) -> UInt16 { a >> b }

fn main() -> Int {
    let _a: Int8 = add_i8(1, 2);
    let _b: UInt16 = add_u16(3, 4);
    let c: UInt32 = div_u32(8, 2);
    let _d: Int16 = div_i16(8, 2);
    let _e: UInt16 = shr_u16(8, 1);
    if cmp_u32(c, 1) { 1 } else { 0 }
}
"#;
    let (program, diags) = parse(src, "fixed_width_ops.aic");
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "fixed_width_ops.aic").expect("llvm");
    assert!(output.llvm_ir.contains("define i8 @aic_add_i8(i8"));
    assert!(output.llvm_ir.contains("define i16 @aic_add_u16(i16"));
    assert!(output.llvm_ir.contains("define i32 @aic_div_u32(i32"));
    assert!(output.llvm_ir.contains("define i16 @aic_div_i16(i16"));
    assert!(output.llvm_ir.contains("udiv i32"));
    assert!(output.llvm_ir.contains("sdiv i16"));
    assert!(output.llvm_ir.contains("icmp ult i32"));
    assert!(output.llvm_ir.contains("lshr i16"));
}

#[test]
fn extern_fixed_width_signatures_emit_exact_llvm_decls() {
    let src = r#"
extern "C" fn c_add_u16(a: UInt16, b: UInt16) -> UInt16;
extern "C" fn c_neg_i8(a: Int8) -> Int8;

fn main() -> Int { 0 }
"#;
    let (program, diags) = parse(src, "extern_fixed_width.aic");
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "extern_fixed_width.aic").expect("llvm");
    assert!(output.llvm_ir.contains("declare i16 @c_add_u16(i16, i16)"));
    assert!(output.llvm_ir.contains("declare i8 @c_neg_i8(i8)"));
}

#[test]
fn entry_wrapper_casts_fixed_width_main_returns_to_i32() {
    let src = r#"
fn main() -> UInt8 { 7 }
"#;
    let (program, diags) = parse(src, "main_u8_return.aic");
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "main_u8_return.aic").expect("llvm");
    assert!(output.llvm_ir.contains("define i8 @aic_main()"));
    assert!(output.llvm_ir.contains("zext i8"));
    assert!(output.llvm_ir.contains("ret i32"));
}

#[test]
fn struct_literal_fields_provide_builtin_type_hints_without_outer_expected_layout() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("struct_holder.aic");
    fs::write(
        &file,
        r#"
import std.vec;

struct Holder {
    ints: Vec[Int],
    texts: Vec[String],
}

fn build_holder() -> Holder {
    Holder {
        ints: vec.new_vec(),
        texts: vec.new_vec(),
    }
}

fn main() -> Int {
    let holder = build_holder();
    holder.ints.len
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");
    assert!(output.llvm_ir.contains("define i64 @aic_main()"));
}

#[test]
fn tail_return_fibonacci_emits_musttail() {
    let src = r#"
fn fib_tail(n: Int, a: Int, b: Int) -> Int {
if n == 0 {
    return a;
} else {
    return fib_tail(n - 1, b % 1000000007, (a + b) % 1000000007);
};
0
}

fn main() -> Int {
fib_tail(10, 0, 1)
}
"#;
    let (program, diags) = parse(src, "tail_self.aic");
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "tail_self.aic").expect("llvm");
    assert!(
        output
            .llvm_ir
            .contains("musttail call i64 @aic_fib_tail(i64"),
        "llvm missing musttail for fibonacci self recursion:\n{}",
        output.llvm_ir
    );
}

#[test]
fn tail_expr_fibonacci_emits_musttail() {
    let src = r#"
fn fib_tail(n: Int, a: Int, b: Int) -> Int {
if n == 0 {
    a
} else {
    fib_tail(n - 1, b % 1000000007, (a + b) % 1000000007)
}
}

fn main() -> Int {
fib_tail(10, 0, 1)
}
"#;
    let (program, diags) = parse(src, "tail_expr_fib.aic");
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "tail_expr_fib.aic").expect("llvm");
    assert!(
        output
            .llvm_ir
            .contains("musttail call i64 @aic_fib_tail(i64"),
        "llvm missing musttail for tail-expression fibonacci:\n{}",
        output.llvm_ir
    );
}

#[test]
fn tail_return_mutual_recursion_emits_musttail() {
    let src = r#"
fn is_even(n: Int) -> Bool {
if n == 0 {
    return true;
} else {
    return is_odd(n - 1);
};
false
}

fn is_odd(n: Int) -> Bool {
if n == 0 {
    return false;
} else {
    return is_even(n - 1);
};
false
}

fn main() -> Int {
if is_even(10) { 1 } else { 0 }
}
"#;
    let (program, diags) = parse(src, "tail_mutual.aic");
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "tail_mutual.aic").expect("llvm");
    assert!(
        output.llvm_ir.contains("musttail call i1 @aic_is_odd(i64"),
        "llvm missing musttail for even->odd:\n{}",
        output.llvm_ir
    );
    assert!(
        output.llvm_ir.contains("musttail call i1 @aic_is_even(i64"),
        "llvm missing musttail for odd->even:\n{}",
        output.llvm_ir
    );
}

#[test]
fn non_tail_recursion_does_not_emit_musttail() {
    let src = r#"
fn countdown(n: Int) -> Int {
if n == 0 {
    0
} else {
    1 + countdown(n - 1)
}
}

fn main() -> Int {
countdown(10)
}
"#;
    let (program, diags) = parse(src, "tail_non_tail.aic");
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "tail_non_tail.aic").expect("llvm");
    assert!(
        !output
            .llvm_ir
            .contains("musttail call i64 @aic_countdown(i64"),
        "non-tail recursion unexpectedly emitted musttail:\n{}",
        output.llvm_ir
    );
}

#[test]
fn parses_clang_major_from_common_version_strings() {
    let llvm = "clang version 18.1.2 (https://github.com/llvm/llvm-project.git ...)";
    let apple = "Apple clang version 17.0.0 (clang-1700.3.19.1)";
    assert_eq!(parse_llvm_major(llvm), Some(18));
    assert_eq!(parse_llvm_major(apple), Some(17));
}

#[test]
fn rejects_unsupported_llvm_major() {
    let info = ToolchainInfo {
        clang_version: "clang version 10.0.0".to_string(),
        llvm_major: 10,
    };
    let err = ensure_supported_toolchain(&info).expect_err("expected unsupported toolchain");
    assert!(err
        .to_string()
        .contains("unsupported LLVM/clang major version"));
}

#[test]
fn accepts_matching_toolchain_pin() {
    let info = ToolchainInfo {
        clang_version: "clang version 18.1.0".to_string(),
        llvm_major: 18,
    };
    ensure_supported_toolchain_with_pin(&info, Some(18))
        .expect("matching toolchain pin should pass");
}

#[test]
fn rejects_mismatched_toolchain_pin() {
    let info = ToolchainInfo {
        clang_version: "clang version 18.1.0".to_string(),
        llvm_major: 18,
    };
    let err = ensure_supported_toolchain_with_pin(&info, Some(17))
        .expect_err("mismatched toolchain pin should fail");
    assert!(err.to_string().contains("toolchain pin mismatch"));
}

#[test]
fn runtime_compile_flags_include_asan_and_leak_switches() {
    let flags = runtime_compile_flags(RuntimeInstrumentationOptions {
        check_leaks: true,
        asan: true,
    });
    assert!(flags.contains(&"-DAIC_RT_CHECK_LEAKS=1"));
    assert!(flags.contains(&"-fsanitize=address"));
    assert!(flags.contains(&"-fno-omit-frame-pointer"));
}

#[test]
fn optimization_levels_map_to_expected_clang_flags() {
    assert_eq!(OptimizationLevel::O0.clang_flag(), "-O0");
    assert_eq!(OptimizationLevel::O1.clang_flag(), "-O1");
    assert_eq!(OptimizationLevel::O2.clang_flag(), "-O2");
    assert_eq!(OptimizationLevel::O3.clang_flag(), "-O3");
}

#[test]
fn compile_options_default_to_o0() {
    assert_eq!(CompileOptions::default().opt_level, OptimizationLevel::O0);
}

#[test]
fn llvm_leak_instrumentation_rewrites_malloc_symbol() {
    let llvm = "declare i8* @malloc(i64)\n%1 = call i8* @malloc(i64 32)\n";
    let instrumented = instrument_llvm_for_leak_tracking(llvm);
    assert!(instrumented.contains("@aic_rt_heap_alloc("));
    assert!(!instrumented.contains("@malloc("));
}

#[test]
fn wasm_entry_wrapper_removes_env_args_bridge() {
    let llvm = concat!(
        "define i32 @main(i32 %argc, i8** %argv) {\n",
        "entry:\n",
        "  call void @aic_rt_env_set_args(i32 %argc, i8** %argv)\n",
        "  ret i32 0\n",
        "}\n",
    );
    let rewritten = rewrite_wasm_entry_wrapper(llvm);
    assert!(!rewritten.contains("@aic_rt_env_set_args"));
    assert!(rewritten.contains("ret i32 0"));
}

#[test]
fn detects_wasm_target_triple() {
    assert!(target_is_wasm(Some("wasm32-unknown-unknown")));
    assert!(target_is_wasm(Some("wasm32-wasi")));
    assert!(!target_is_wasm(Some("x86_64-unknown-linux-gnu")));
    assert!(!target_is_wasm(None));
}

#[test]
fn emits_nested_adt_layout_snapshot() {
    let src = r#"
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

fn main() -> Int {
fold(Full(Pair { left: 20, right: 22 }))
}
"#;
    let (program, diags) = parse(src, "layout.aic");
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "layout.aic").expect("llvm");
    assert!(output.llvm_ir.contains("{ i32, i8, { i64, i64 } }"));
    assert!(output.llvm_ir.contains("switch i32"));
}

#[test]
fn monomorphized_generic_symbols_are_deduped_and_stable() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("generic.aic");
    fs::write(
        &file,
        r#"
fn id[T](x: T) -> T {
x
}

fn main() -> Int {
let a = id(40);
let b = id(2);
let c = id(true);
if c { a + b } else { 0 }
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );

    let lowered = lower_runtime_asserts(&front.ir);
    let out1 = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");
    let out2 = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");

    assert_eq!(out1.llvm_ir, out2.llvm_ir, "codegen must be deterministic");

    let int_defs = out1
        .llvm_ir
        .lines()
        .filter(|line| line.starts_with("define i64 @aic_fn_id_Int("))
        .count();
    let bool_defs = out1
        .llvm_ir
        .lines()
        .filter(|line| line.starts_with("define i1 @aic_fn_id_Bool("))
        .count();
    assert_eq!(int_defs, 1, "Int instantiation should be deduped");
    assert_eq!(bool_defs, 1, "Bool instantiation should be emitted");
}

#[test]
fn deterministic_codegen_is_stable_across_100_iterations() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("determinism_100x.aic");
    fs::write(
        &file,
        r#"
fn add(x: Int, y: Int) -> Int {
x + y
}

fn main() -> Int {
let a = add(20, 22);
if a == 42 { 1 } else { 0 }
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let expected = emit_llvm(&lowered, &file.to_string_lossy())
        .expect("llvm")
        .llvm_ir;

    for _ in 0..100 {
        let current = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");
        assert_eq!(
            current.llvm_ir, expected,
            "codegen must remain byte-identical across repeated compiles"
        );
    }
}

#[test]
fn normalize_macos_uuid_is_deterministic_and_idempotent() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("fake-macho.bin");
    let mut bytes = vec![0u8; 32 + 24];

    bytes[0..4].copy_from_slice(&0xfeed_facf_u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
    bytes[32..36].copy_from_slice(&0x1b_u32.to_le_bytes());
    bytes[36..40].copy_from_slice(&24u32.to_le_bytes());
    bytes[40..56].copy_from_slice(&[
        0xde, 0xad, 0xbe, 0xef, 0x11, 0x22, 0x33, 0x44, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x12,
        0x34,
    ]);

    fs::write(&path, &bytes).expect("write fake macho");

    normalize_macos_uuid(&path).expect("normalize first run");
    let first = fs::read(&path).expect("read first");
    normalize_macos_uuid(&path).expect("normalize second run");
    let second = fs::read(&path).expect("read second");

    assert_eq!(first, second, "uuid normalization must be idempotent");
    assert_ne!(&first[40..56], &bytes[40..56], "uuid should change");

    let mut normalized = first.clone();
    normalized[40..56].fill(0);
    let digest = Sha256::digest(&normalized);
    let mut expected_uuid = [0u8; 16];
    expected_uuid.copy_from_slice(&digest[..16]);
    expected_uuid[6] = (expected_uuid[6] & 0x0f) | 0x40;
    expected_uuid[8] = (expected_uuid[8] & 0x3f) | 0x80;
    assert_eq!(&first[40..56], expected_uuid.as_slice());
}

#[test]
fn lexical_drop_emits_reverse_lexical_lifetime_end() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("lexical_drop_order.aic");
    fs::write(
        &file,
        r#"
fn main() -> Int {
let outer = "outer";
let inner = "inner";
if true { 0 } else { 1 }
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");
    let lines: Vec<&str> = output.llvm_ir.lines().collect();

    let mut drop_order: Vec<String> = Vec::new();
    for pair in lines.windows(2) {
        let bitcast = pair[0].trim();
        let lifetime_end = pair[1].trim();
        if !bitcast.contains("bitcast { i8*, i64, i64 }* %") || !bitcast.ends_with(" to i8*") {
            continue;
        }
        if !lifetime_end.contains("call void @llvm.lifetime.end.p0i8(i64 -1, i8*") {
            continue;
        }

        let Some(start) = bitcast.find("* %") else {
            continue;
        };
        let tail = &bitcast[start + 3..];
        let Some(end) = tail.find(" to i8*") else {
            continue;
        };
        drop_order.push(tail[..end].to_string());
    }

    let lexical_locals: Vec<String> = lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.contains(" = alloca { i8*, i64, i64 }") {
                return None;
            }
            let name = trimmed
                .split('=')
                .next()
                .expect("alloca lhs")
                .trim()
                .trim_start_matches('%')
                .to_string();
            Some(name)
        })
        .collect();
    assert!(
        lexical_locals.len() >= 2,
        "expected at least two runtime-drop locals; llvm={}",
        output.llvm_ir
    );

    let filtered: Vec<String> = drop_order
        .iter()
        .filter(|name| lexical_locals.iter().any(|local| local == *name))
        .cloned()
        .collect();
    let expected: Vec<String> = lexical_locals.iter().rev().cloned().collect();
    assert_eq!(
        filtered,
        expected,
        "expected reverse lexical lifetime.end order for locals; lexical={lexical_locals:?}; drop_order={drop_order:?}\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn file_handle_drop_emits_reverse_lexical_close_order() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("file_drop_order.aic");
    fs::write(
        &file,
        r#"
import std.fs;

fn main() -> Int {
let first = FileHandle { handle: 1 };
let second = FileHandle { handle: 2 };
if first.handle == 1 && second.handle == 2 { 0 } else { 1 }
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");
    let lines: Vec<&str> = output.llvm_ir.lines().collect();
    let main_start = lines
        .iter()
        .position(|line| line.starts_with("define i64 @aic_main() {"))
        .expect("aic_main function");
    let main_end = lines[main_start + 1..]
        .iter()
        .position(|line| line.trim() == "}")
        .map(|idx| main_start + 1 + idx)
        .expect("aic_main closing brace");
    let main_lines = &lines[main_start..=main_end];

    let lexical_locals: Vec<String> = main_lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.contains(" = alloca { i64 }") {
                return None;
            }
            Some(
                trimmed
                    .split('=')
                    .next()
                    .expect("alloca lhs")
                    .trim()
                    .trim_start_matches('%')
                    .to_string(),
            )
        })
        .collect();
    assert!(
        lexical_locals.len() >= 2,
        "expected at least two FileHandle locals; llvm={}",
        output.llvm_ir
    );

    let mut load_to_alloca: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut extract_to_load: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut close_order: Vec<String> = Vec::new();
    for line in main_lines {
        let trimmed = line.trim();
        if let Some((lhs, tail)) = trimmed.split_once(" = load { i64 }, { i64 }* %") {
            let src = tail
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches(',')
                .to_string();
            load_to_alloca.insert(lhs.trim_start_matches('%').to_string(), src);
            continue;
        }
        if let Some((lhs, tail)) = trimmed.split_once(" = extractvalue { i64 } %") {
            let src = tail
                .split(',')
                .next()
                .unwrap_or_default()
                .trim()
                .trim_start_matches('%')
                .to_string();
            extract_to_load.insert(lhs.trim_start_matches('%').to_string(), src);
            continue;
        }
        if let Some((_, tail)) = trimmed.split_once("@aic_rt_fs_file_close(i64 %") {
            let handle_reg = tail
                .split(')')
                .next()
                .unwrap_or_default()
                .trim()
                .trim_start_matches('%');
            if let Some(load_reg) = extract_to_load.get(handle_reg) {
                if let Some(alloca) = load_to_alloca.get(load_reg) {
                    close_order.push(alloca.clone());
                }
            }
        }
    }

    let filtered: Vec<String> = close_order
        .iter()
        .filter(|name| lexical_locals.iter().any(|local| local == *name))
        .cloned()
        .collect();
    let expected: Vec<String> = lexical_locals.iter().rev().cloned().collect();
    assert_eq!(
        filtered,
        expected,
        "expected reverse lexical close order; lexical={lexical_locals:?}; close_order={close_order:?}\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn file_handle_drop_runs_before_early_return() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("file_drop_early_return.aic");
    fs::write(
        &file,
        r#"
import std.fs;

fn helper() -> () {
let file = FileHandle { handle: 3 };
if file.handle == 3 {
    return;
} else {
    ()
};
}

fn main() -> Int {
helper();
0
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");
    let mut checked = false;
    for block in output.llvm_ir.split("\ndefine ") {
        if !block.contains("@aic_rt_fs_file_close(") || !block.contains("ret void") {
            continue;
        }
        let close_idx = block
            .find("@aic_rt_fs_file_close(")
            .expect("file close call index");
        let ret_idx = block.find("ret void").expect("ret void index");
        assert!(
            close_idx < ret_idx,
            "expected close before return; close_idx={close_idx}; ret_idx={ret_idx}\nllvm={}",
            output.llvm_ir
        );
        checked = true;
        break;
    }
    assert!(
        checked,
        "expected a function block with file-close call followed by ret void\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn file_handle_drop_runs_before_question_mark_error_return() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("file_drop_try_return.aic");
    fs::write(
        &file,
        r#"
import std.fs;

fn helper() -> Result[Int, FsError] effects { fs } capabilities { fs } {
let file = FileHandle { handle: 3 };
read_text("")?;
if file.handle == 3 {
    Ok(1)
} else {
    Ok(0)
}
}

fn main() -> Int effects { fs } capabilities { fs } {
match helper() {
    Ok(v) => v,
    Err(_) => 0,
}
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");
    let mut checked = false;
    for block in output.llvm_ir.split("\ndefine ") {
        let is_helper = block
            .lines()
            .next()
            .map(|line| line.contains("@aic_helper("))
            .unwrap_or(false);
        if !is_helper || !block.contains("@aic_rt_fs_file_close(") {
            continue;
        }
        let close_idx = block
            .find("@aic_rt_fs_file_close(")
            .expect("file close call index");
        let ret_idx = block[close_idx..]
            .find("ret ")
            .map(|idx| close_idx + idx)
            .expect("ret after file close");
        assert!(
            close_idx < ret_idx,
            "expected close before `?` return; close_idx={close_idx}; ret_idx={ret_idx}\nllvm={}",
            output.llvm_ir
        );
        checked = true;
        break;
    }
    assert!(
        checked,
        "expected helper block with file-close call followed by return\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn supported_handle_drop_is_lifo_and_skips_moved_sources() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("handle_drop_lifo_and_moves.aic");
    fs::write(
        &file,
        r#"
import std.fs;
import std.concurrent;

fn make_file() -> FileHandle {
let file = FileHandle { handle: 1 };
file
}

fn make_channel() -> IntChannel {
let channel = IntChannel { handle: 2 };
channel
}

fn make_mutex() -> IntMutex {
let mutex = IntMutex { handle: 3 };
mutex
}

fn main() -> Int {
let file = make_file();
let channel = make_channel();
let mutex = make_mutex();
if file.handle + channel.handle + mutex.handle == 6 { 0 } else { 1 }
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");

    let make_file_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| block.starts_with("{ i64 } @aic_make_file("))
        .expect("make_file block");
    assert!(
        !make_file_block.contains("@aic_rt_fs_file_close("),
        "moved source in make_file should not be closed in callee\nllvm={}",
        output.llvm_ir
    );
    let make_channel_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| block.starts_with("{ i64 } @aic_make_channel("))
        .expect("make_channel block");
    assert!(
        !make_channel_block.contains("@aic_rt_conc_close_channel("),
        "moved source in make_channel should not be closed in callee\nllvm={}",
        output.llvm_ir
    );
    let make_mutex_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| block.starts_with("{ i64 } @aic_make_mutex("))
        .expect("make_mutex block");
    assert!(
        !make_mutex_block.contains("@aic_rt_conc_mutex_close("),
        "moved source in make_mutex should not be closed in callee\nllvm={}",
        output.llvm_ir
    );

    let main_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| block.starts_with("i64 @aic_main("))
        .expect("aic_main block");
    let mutex_close = main_block
        .find("@aic_rt_conc_mutex_close(")
        .expect("mutex close in main");
    let channel_close = main_block
        .find("@aic_rt_conc_close_channel(")
        .expect("channel close in main");
    let file_close = main_block
        .find("@aic_rt_fs_file_close(")
        .expect("file close in main");
    assert!(
        mutex_close < channel_close && channel_close < file_close,
        "expected LIFO drop order in main (mutex -> channel -> file)\nllvm={}",
        output.llvm_ir
    );
    assert_eq!(
        main_block.matches("@aic_rt_conc_mutex_close(").count(),
        1,
        "expected one mutex close in main\nllvm={}",
        output.llvm_ir
    );
    assert_eq!(
        main_block.matches("@aic_rt_conc_close_channel(").count(),
        1,
        "expected one channel close in main\nllvm={}",
        output.llvm_ir
    );
    assert_eq!(
        main_block.matches("@aic_rt_fs_file_close(").count(),
        1,
        "expected one file close in main\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn map_set_tcp_reader_drop_is_emitted_and_lifo() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("map_set_tcp_reader_drop_lifo.aic");
    fs::write(
        &file,
        r#"
import std.map;
import std.set;
import std.io;
import std.tls;

fn main() -> Int {
let map: Map[Int, Int] = Map { handle: 11 };
let set: Set[Int] = Set { items: Map { handle: 22 } };
let reader = TcpReader { handle: 33, max_bytes: 64, timeout_ms: 10 };
let tls = TlsStream { handle: 44 };
if map.handle + set.items.handle + reader.handle + tls.handle == 110 { 0 } else { 1 }
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");
    let main_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| block.starts_with("i64 @aic_main("))
        .expect("aic_main block");

    let tls_close = main_block
        .find("@aic_rt_tls_close(")
        .expect("tls close call in main");
    let tcp_close = main_block
        .find("@aic_rt_net_tcp_close(")
        .expect("tcp close call in main");
    let map_close_positions = main_block
        .match_indices("@aic_rt_map_close(")
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>();
    assert_eq!(
        map_close_positions.len(),
        2,
        "expected two map-close calls in main (Set inner map + Map local)\nllvm={}",
        output.llvm_ir
    );
    assert!(
        tls_close < tcp_close
            && tcp_close < map_close_positions[0]
            && map_close_positions[0] < map_close_positions[1],
        "expected LIFO drop order tls -> reader -> set.items -> map\nllvm={}",
        output.llvm_ir
    );
    assert_eq!(
        main_block.matches("@aic_rt_tls_close(").count(),
        1,
        "expected one tls close in main\nllvm={}",
        output.llvm_ir
    );
    assert_eq!(
        main_block.matches("@aic_rt_net_tcp_close(").count(),
        1,
        "expected one tcp close in main\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn moved_map_and_set_sources_skip_close_in_callee() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("map_set_move_skip_drop.aic");
    fs::write(
        &file,
        r#"
import std.map;
import std.set;

fn make_map() -> Map[Int, Int] {
let map: Map[Int, Int] = Map { handle: 1 };
map
}

fn make_set() -> Set[Int] {
let set: Set[Int] = Set { items: Map { handle: 2 } };
set
}

fn main() -> Int {
let map = make_map();
let set = make_set();
if map.handle + set.items.handle == 3 { 0 } else { 1 }
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");

    let make_map_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| block.starts_with("{ i64 } @aic_make_map("))
        .expect("make_map block");
    assert!(
        !make_map_block.contains("@aic_rt_map_close("),
        "moved map source should not be closed in callee\nllvm={}",
        output.llvm_ir
    );
    let make_set_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| block.contains("@aic_make_set("))
        .expect("make_set block");
    assert!(
        !make_set_block.contains("@aic_rt_map_close("),
        "moved set source should not be closed in callee\nllvm={}",
        output.llvm_ir
    );
    let main_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| block.starts_with("i64 @aic_main("))
        .expect("aic_main block");
    assert_eq!(
        main_block.matches("@aic_rt_map_close(").count(),
        2,
        "expected two map-close calls in main\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn trait_drop_dispatch_is_lifo_and_skips_moved_sources() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("trait_drop_lifo_and_moves.aic");
    fs::write(
        &file,
        r#"
trait Drop[T] {
fn drop(self) -> ();
}

struct Probe {
id: Int,
}

impl Drop[Probe] {
fn drop(self) -> () {
    let _id = self.id;
    ()
}
}

fn make_probe(id: Int) -> Probe {
let probe = Probe { id: id };
probe
}

fn main() -> Int {
let first = Probe { id: 1 };
let second = Probe { id: 2 };
let third = make_probe(3);
if first.id + second.id + third.id == 6 { 0 } else { 1 }
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");
    let lines: Vec<&str> = output.llvm_ir.lines().collect();

    let make_probe_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| block.starts_with("{ i64 } @aic_make_probe("))
        .expect("make_probe block");
    assert!(
        !make_probe_block.contains("@aic_Probe__drop("),
        "moved source in make_probe should not call drop\nllvm={}",
        output.llvm_ir
    );

    let main_start = lines
        .iter()
        .position(|line| line.starts_with("define i64 @aic_main() {"))
        .expect("aic_main function");
    let main_end = lines[main_start + 1..]
        .iter()
        .position(|line| line.trim() == "}")
        .map(|idx| main_start + 1 + idx)
        .expect("aic_main closing brace");
    let main_lines = &lines[main_start..=main_end];

    let lexical_locals: Vec<String> = main_lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.contains(" = alloca { i64 }") {
                return None;
            }
            Some(
                trimmed
                    .split('=')
                    .next()
                    .expect("alloca lhs")
                    .trim()
                    .trim_start_matches('%')
                    .to_string(),
            )
        })
        .collect();
    assert!(
        lexical_locals.len() >= 3,
        "expected at least three Probe locals in main\nllvm={}",
        output.llvm_ir
    );

    let mut load_to_alloca: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut drop_order: Vec<String> = Vec::new();
    for line in main_lines {
        let trimmed = line.trim();
        if let Some((lhs, tail)) = trimmed.split_once(" = load { i64 }, { i64 }* %") {
            let src = tail
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches(',')
                .to_string();
            load_to_alloca.insert(lhs.trim_start_matches('%').to_string(), src);
            continue;
        }
        if let Some((_, tail)) = trimmed.split_once("@aic_Probe__drop({ i64 } %") {
            let load_reg = tail
                .split(')')
                .next()
                .unwrap_or_default()
                .trim()
                .trim_start_matches('%');
            if let Some(alloca) = load_to_alloca.get(load_reg) {
                drop_order.push(alloca.clone());
            }
        }
    }

    let filtered: Vec<String> = drop_order
        .iter()
        .filter(|name| lexical_locals.iter().any(|local| local == *name))
        .cloned()
        .collect();
    let expected: Vec<String> = lexical_locals.iter().rev().cloned().collect();
    assert_eq!(
        filtered,
        expected,
        "expected reverse lexical drop order for Probe locals\nlexical={lexical_locals:?}\ndrop_order={drop_order:?}\nllvm={}",
        output.llvm_ir
    );
    assert_eq!(
        main_lines
            .iter()
            .filter(|line| line.contains("@aic_Probe__drop("))
            .count(),
        3,
        "expected exactly three drop calls in main\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn trait_drop_runs_before_question_mark_error_return() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("trait_drop_try_return.aic");
    fs::write(
        &file,
        r#"
trait Drop[T] {
fn drop(self) -> ();
}

struct Probe {
id: Int,
}

impl Drop[Probe] {
fn drop(self) -> () {
    let _id = self.id;
    ()
}
}

fn helper() -> Result[Int, Int] {
let probe = Probe { id: 9 };
Err(1)?;
Ok(probe.id)
}

fn main() -> Int {
match helper() {
    Ok(v) => v,
    Err(v) => v,
}
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");

    let helper_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| {
            block
                .lines()
                .next()
                .map(|line| line.contains("@aic_helper("))
                .unwrap_or(false)
        })
        .expect("helper block");
    let drop_idx = helper_block
        .find("@aic_Probe__drop(")
        .expect("drop call in helper");
    let ret_idx = helper_block[drop_idx..]
        .find("ret ")
        .map(|idx| drop_idx + idx)
        .expect("return after drop");
    assert!(
        drop_idx < ret_idx,
        "expected drop before `?` return in helper\ndrop_idx={drop_idx}\nret_idx={ret_idx}\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn async_fn_and_await_lower_to_async_value_wrap_and_extract() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("async_lowering_state_shape.aic");
    fs::write(
        &file,
        r#"
import std.io;

async fn ping(x: Int) -> Int {
x + 1
}

async fn main() -> Int effects { io } capabilities { io } {
let value = await ping(41);
value
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");

    assert!(
        output
            .llvm_ir
            .contains("define { i1, i64 } @aic_ping(i64 %arg0)"),
        "async function should lower to Async[Int] return type\nllvm={}",
        output.llvm_ir
    );
    assert!(
        output
            .llvm_ir
            .contains("insertvalue { i1, i64 } undef, i1 1, 0"),
        "async return should wrap ready state\nllvm={}",
        output.llvm_ir
    );
    assert!(
        output.llvm_ir.contains("extractvalue { i1, i64 }"),
        "await should lower to Async value extraction\nllvm={}",
        output.llvm_ir
    );
    assert!(
        output.llvm_ir.contains("call { i1, i64 } @aic_main()"),
        "entry wrapper should call async main and unwrap result\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn await_submit_bridge_lowers_to_reactor_poll_runtime_calls() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("await_submit_bridge_polling.aic");
    fs::write(
        &file,
        r#"
import std.net;

async fn main() -> Int effects { net, concurrency } capabilities { net, concurrency } {
let accepted = await async_accept_submit(0, 25);
let _recv = await async_tcp_recv_submit(0, 8, 25);
let _server = match accepted {
    Ok(v) => v,
    Err(_) => 0,
};
0
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");

    assert!(
        output
            .llvm_ir
            .contains("declare i64 @aic_rt_async_poll_int(i64, i64*)"),
        "await submit bridge should declare async int poll helper\nllvm={}",
        output.llvm_ir
    );
    assert!(
        output
            .llvm_ir
            .contains("declare i64 @aic_rt_async_poll_string(i64, i8**, i64*)"),
        "await submit bridge should declare async string poll helper\nllvm={}",
        output.llvm_ir
    );
    assert!(
        output.llvm_ir.contains("call i64 @aic_rt_async_poll_int("),
        "await submit bridge should poll int reactor operation\nllvm={}",
        output.llvm_ir
    );
    assert!(
        output
            .llvm_ir
            .contains("call i64 @aic_rt_async_poll_string("),
        "await submit bridge should poll string reactor operation\nllvm={}",
        output.llvm_ir
    );
    assert!(
        !output.llvm_ir.contains("call i64 @aic_rt_conc_spawn("),
        "await submit bridge must not lower to thread-per-task spawn\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn struct_init_tail_move_skips_map_close_on_moved_local() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("struct_tail_move_map.aic");
    fs::write(
        &file,
        r#"
import std.map;
import std.set;

fn build() -> Set[Int] {
let m: Map[Int, Int] = Map { handle: 7 };
Set { items: m }
}

fn main() -> Int {
let value = build();
if value.items.handle == 7 { 0 } else { 1 }
}
"#,
    )
    .expect("write source");

    let front = run_frontend(&file).expect("frontend");
    assert!(
        !has_errors(&front.diagnostics),
        "diagnostics={:#?}",
        front.diagnostics
    );
    let lowered = lower_runtime_asserts(&front.ir);
    let output = emit_llvm(&lowered, &file.to_string_lossy()).expect("llvm");

    let build_block = output
        .llvm_ir
        .split("\ndefine ")
        .find(|block| {
            block
                .lines()
                .next()
                .map(|line| line.contains("@aic_build("))
                .unwrap_or(false)
        })
        .expect("build block");
    assert!(
        !build_block.contains("@aic_rt_map_close("),
        "build must not close moved map handle before returning Set\nllvm={}",
        output.llvm_ir
    );
}

#[test]
fn emits_debug_metadata_and_panic_line_mapping() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("panic_line_map.aic");
    let source = r#"fn main() -> Int effects { io } {
panic("boom");
0
}
"#;
    fs::write(&file, source).expect("write source");

    let (program, diags) = parse(source, &file.to_string_lossy());
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm_with_options(
        &lowered,
        &file.to_string_lossy(),
        CodegenOptions { debug_info: true },
    )
    .expect("llvm");

    assert!(output.llvm_ir.contains("!DICompileUnit("));
    assert!(output.llvm_ir.contains("!DISubprogram("));

    let panic_call = output
        .llvm_ir
        .lines()
        .find(|line| line.contains("call void @aic_rt_panic"))
        .expect("panic call line");
    assert!(panic_call.contains("i64 2"), "panic call line={panic_call}");
    assert!(
        panic_call.contains(", !dbg !"),
        "panic call should include debug location"
    );

    let dbg_ref = panic_call.split("!dbg !").nth(1).expect("debug ref").trim();
    let expected = format!("!{} = !DILocation(line: 2,", dbg_ref);
    assert!(
        output.llvm_ir.contains(&expected),
        "missing panic source line location metadata"
    );
}

#[test]
fn release_codegen_omits_debug_metadata() {
    let src = r#"
fn main() -> Int effects { io } {
panic("boom");
0
}
"#;
    let (program, diags) = parse(src, "release_mode.aic");
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "release_mode.aic").expect("llvm");
    assert!(!output.llvm_ir.contains("!DICompileUnit("));
    assert!(!output.llvm_ir.contains("!DILocation("));
}

#[test]
fn panic_runtime_and_ir_abi_match() {
    let src = r#"fn main() -> Int { 0 }"#;
    let (program, diags) = parse(src, "abi_check.aic");
    assert!(diags.is_empty(), "parse diagnostics={diags:#?}");
    let ir = build(&program.expect("program"));
    let lowered = lower_runtime_asserts(&ir);
    let output = emit_llvm(&lowered, "abi_check.aic").expect("llvm");
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_panic(i8*, i64, i64, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_log_emit(i64, i8*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_log_set_level(i64)"));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_log_set_json(i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_fs_read_text(i8*, i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_string_contains(i8*, i64, i64, i8*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_string_parse_int(i8*, i64, i64, i64*, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_string_join(i8*, i64, i64, i8*, i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_vec_new(i8**, i64*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_with_capacity(i64, i64, i8**, i64*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_of(i8*, i64, i8**, i64*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_get(i8*, i64, i64, i64, i64, i8*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_push(i8**, i64*, i64*, i64, i8*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_pop(i8**, i64*, i64*, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_set(i8*, i64, i64, i64, i64, i8*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_insert(i8**, i64*, i64*, i64, i64, i8*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_remove_at(i8**, i64*, i64*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_reserve(i8**, i64*, i64*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_shrink_to_fit(i8**, i64*, i64*, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_contains(i8*, i64, i64, i64, i64, i8*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_index_of(i8*, i64, i64, i64, i64, i8*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_reverse(i8*, i64, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_slice(i8*, i64, i64, i64, i64, i64, i8**, i64*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_vec_append(i8**, i64*, i64*, i64, i8*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_vec_clear(i8**, i64*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_fs_metadata(i8*, i64, i64, i64*, i64*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_new(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_close(i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_insert_string(i64, i8*, i64, i64, i8*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_insert_int(i64, i8*, i64, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_get_string(i64, i8*, i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_get_int(i64, i8*, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_contains(i64, i8*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_remove(i64, i8*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_size(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_keys(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_values_string(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_values_int(i64, i64**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_entries_string(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_map_entries_int(i64, i8**, i64*)"));
    assert!(output.llvm_ir.contains("declare i64 @aic_rt_time_now_ms()"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_time_monotonic_ms()"));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_time_sleep_ms(i64)"));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_time_parse_rfc3339(i8*, i64, i64, i64*, i64*, i64*, i64*, i64*, i64*, i64*, i64*)"
    ));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_time_parse_iso8601(i8*, i64, i64, i64*, i64*, i64*, i64*, i64*, i64*, i64*, i64*)"
    ));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_time_format_rfc3339(i64, i64, i64, i64, i64, i64, i64, i64, i8**, i64*)"
    ));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_time_format_iso8601(i64, i64, i64, i64, i64, i64, i64, i64, i8**, i64*)"
    ));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_signal_register(i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_signal_wait(i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_rand_seed(i64)"));
    assert!(output.llvm_ir.contains("declare i64 @aic_rt_rand_next()"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_rand_range(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_mock_io_set_stdin(i8*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_mock_io_take_stdout(i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_mock_io_take_stderr(i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_spawn(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_join(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_join_timeout(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_spawn_group(i8*, i64, i64, i64, i64**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_select_first(i8*, i64, i64, i64, i64*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_channel_int(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_channel_int_buffered(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_try_send_int(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_try_recv_int(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_select_recv_int(i64, i64, i64, i64*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_arc_new(i8*, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_arc_clone(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_arc_get(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_arc_strong_count(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_arc_release(i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_atomic_int_new(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_atomic_int_load(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_atomic_int_store(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_atomic_int_add(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_atomic_int_sub(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_atomic_int_cas(i64, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_atomic_bool_new(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_atomic_bool_load(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_atomic_bool_store(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_atomic_bool_swap(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_tl_new(i64, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_tl_get(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_tl_set(i64, i8*, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_mutex_lock(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_conc_rwlock_write_lock(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_tcp_listen(i8*, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_tcp_set_nodelay(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_tcp_get_nodelay(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_tcp_set_keepalive(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_tcp_get_keepalive(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_tcp_set_send_buffer_size(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_tcp_get_send_buffer_size(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_tcp_set_recv_buffer_size(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_tcp_get_recv_buffer_size(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_udp_recv_from(i64, i64, i64, i8**, i64*, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_dns_lookup(i8*, i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_dns_lookup_all(i8*, i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_async_accept_submit(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_async_send_submit(i64, i8*, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_async_recv_submit(i64, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_async_wait_int(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_async_wait_string(i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_async_shutdown()"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_net_async_pressure(i64*, i64*, i64*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_connect(i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_connect_addr(i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_accept(i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_send(i64, i8*, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_recv(i64, i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_async_send_submit(i64, i8*, i64, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_async_recv_submit(i64, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_async_wait_int(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_async_wait_string(i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_async_shutdown()"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_async_pressure(i64*, i64*, i64*, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_close(i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_peer_subject(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_peer_issuer(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_peer_fingerprint_sha256(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_peer_san_entries(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_tls_version(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_new(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_new_growable(i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_from_bytes(i8*, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_to_bytes(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_read_u16_be(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_read_u32_be(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_read_u64_be(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_read_u16_le(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_read_u32_le(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_read_u64_le(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_write_u16_be(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_write_u32_be(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_write_u64_be(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_write_u16_le(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_write_u32_le(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_write_u64_le(i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_read_length_prefixed(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_close(i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_write_string_prefixed(i64, i8*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_patch_u32_be(i64, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_buffer_patch_u32_le(i64, i64, i64)"));
    assert!(
        !output.llvm_ir.contains("{ i32, void"),
        "enum payload lowering must not materialize void fields"
    );
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_async_poll_int(i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_async_poll_string(i64, i8**, i64*)"));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_url_parse(i8*, i64, i64, i8**, i64*, i8**, i64*, i64*, i8**, i64*, i8**, i64*, i8**, i64*)"
    ));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_url_normalize(i8*, i64, i64, i8*, i64, i64, i64, i8*, i64, i64, i8*, i64, i64, i8*, i64, i64, i8**, i64*)"
    ));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_url_net_addr(i8*, i64, i64, i8*, i64, i64, i64, i8**, i64*)"
    ));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_http_parse_method(i8*, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_http_status_reason(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_http_validate_header(i8*, i64, i64, i8*, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_http_server_listen(i8*, i64, i64, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_http_server_read_request(i64, i64, i64, i8**, i64*, i8**, i64*, i64*, i64*, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_router_new(i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_router_add(i64, i8*, i64, i64, i8*, i64, i64, i64)"));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_router_match(i64, i8*, i64, i64, i8*, i64, i64, i64*, i64*, i64*)"
    ));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_json_parse(i8*, i64, i64, i8**, i64*, i64*)"));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_json_object_set(i8*, i64, i64, i8*, i64, i64, i8*, i64, i64, i8**, i64*, i64*)"
    ));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_json_object_get(i8*, i64, i64, i8*, i64, i64, i8**, i64*, i64*, i64*)"
    ));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_regex_compile(i8*, i64, i64, i64)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_regex_is_match(i8*, i64, i64, i64, i8*, i64, i64, i64*)"));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_regex_captures(i8*, i64, i64, i64, i8*, i64, i64, i8**, i64*, i8**, i64*, i64*, i64*, i64*)"
    ));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_regex_replace(i8*, i64, i64, i64, i8*, i64, i64, i8*, i64, i64, i8**, i64*)"
    ));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_crypto_md5(i8*, i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_crypto_sha256(i8*, i64, i64, i8**, i64*)"));
    assert!(output.llvm_ir.contains(
        "declare i64 @aic_rt_crypto_pbkdf2_sha256(i8*, i64, i64, i8*, i64, i64, i64, i64, i8**, i64*)"
    ));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_crypto_hex_decode(i8*, i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_crypto_base64_decode(i8*, i64, i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare void @aic_rt_crypto_random_bytes(i64, i8**, i64*)"));
    assert!(output
        .llvm_ir
        .contains("declare i64 @aic_rt_crypto_secure_eq(i8*, i64, i64, i8*, i64, i64)"));
    assert!(runtime_c_source().contains(
        "void aic_rt_panic(const char* ptr, long len, long cap, long line, long column)"
    ));
    assert!(runtime_c_source().contains("AIC_BACKTRACE"));
    assert!(runtime_c_source().contains("void* aic_rt_heap_alloc(long size)"));
    assert!(runtime_c_source().contains("memory_leak_detected"));
    assert!(runtime_c_source().contains("#ifdef AIC_RT_CHECK_LEAKS"));
    assert!(runtime_c_source().contains("stack backtrace:"));
    assert!(runtime_c_source().contains("void aic_rt_log_emit("));
    assert!(runtime_c_source().contains("void aic_rt_log_set_level(long level)"));
    assert!(runtime_c_source().contains("void aic_rt_log_set_json(long enabled)"));
    assert!(runtime_c_source().contains("long aic_rt_fs_read_text("));
    assert!(runtime_c_source().contains("long aic_rt_string_contains("));
    assert!(runtime_c_source().contains("long aic_rt_string_parse_int("));
    assert!(runtime_c_source().contains("void aic_rt_string_join("));
    assert!(runtime_c_source().contains("void aic_rt_vec_new("));
    assert!(runtime_c_source().contains("long aic_rt_vec_with_capacity("));
    assert!(runtime_c_source().contains("long aic_rt_vec_of("));
    assert!(runtime_c_source().contains("long aic_rt_vec_get("));
    assert!(runtime_c_source().contains("long aic_rt_vec_push("));
    assert!(runtime_c_source().contains("long aic_rt_vec_pop("));
    assert!(runtime_c_source().contains("long aic_rt_vec_set("));
    assert!(runtime_c_source().contains("long aic_rt_vec_insert("));
    assert!(runtime_c_source().contains("long aic_rt_vec_remove_at("));
    assert!(runtime_c_source().contains("long aic_rt_vec_reserve("));
    assert!(runtime_c_source().contains("long aic_rt_vec_shrink_to_fit("));
    assert!(runtime_c_source().contains("long aic_rt_vec_contains("));
    assert!(runtime_c_source().contains("long aic_rt_vec_index_of("));
    assert!(runtime_c_source().contains("long aic_rt_vec_reverse("));
    assert!(runtime_c_source().contains("long aic_rt_vec_slice("));
    assert!(runtime_c_source().contains("long aic_rt_vec_append("));
    assert!(runtime_c_source().contains("void aic_rt_vec_clear("));
    assert!(runtime_c_source().contains("long aic_rt_fs_metadata("));
    assert!(runtime_c_source()
        .contains("long aic_rt_map_new(long key_kind, long value_kind, long* out_handle)"));
    assert!(runtime_c_source().contains("long aic_rt_map_close(long handle)"));
    assert!(runtime_c_source().contains("long aic_rt_map_insert_string("));
    assert!(runtime_c_source().contains("long aic_rt_map_insert_int("));
    assert!(runtime_c_source().contains("long aic_rt_map_get_string("));
    assert!(runtime_c_source().contains("long aic_rt_map_get_int("));
    assert!(runtime_c_source().contains("long aic_rt_map_contains("));
    assert!(runtime_c_source().contains("long aic_rt_map_remove("));
    assert!(runtime_c_source().contains("long aic_rt_map_size("));
    assert!(runtime_c_source().contains("long aic_rt_map_keys("));
    assert!(runtime_c_source().contains("long aic_rt_map_values_string("));
    assert!(runtime_c_source().contains("long aic_rt_map_values_int("));
    assert!(runtime_c_source().contains("long aic_rt_map_entries_string("));
    assert!(runtime_c_source().contains("long aic_rt_map_entries_int("));
    assert!(runtime_c_source().contains("#define AIC_RT_SSO_INLINE_MAX 23"));
    assert!(runtime_c_source().contains("char key_inline_buf[AIC_RT_SSO_INLINE_MAX + 1];"));
    assert!(runtime_c_source().contains("static int aic_rt_map_string_storage_replace("));
    assert!(runtime_c_source().contains("AIC_RT_DISABLE_MAP_SSO"));
    assert!(runtime_c_source().contains("long aic_rt_buffer_new(long capacity, long* out_handle)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_buffer_new_growable(long initial_capacity, long max_capacity, long* out_handle)"));
    assert!(runtime_c_source().contains("long aic_rt_buffer_from_bytes("));
    assert!(runtime_c_source()
        .contains("long aic_rt_buffer_to_bytes(long handle, char** out_ptr, long* out_len)"));
    assert!(runtime_c_source().contains(
        "long aic_rt_buffer_read_length_prefixed(long handle, char** out_ptr, long* out_len)"
    ));
    assert!(runtime_c_source().contains("long aic_rt_buffer_close(long handle)"));
    assert!(runtime_c_source().contains("long aic_rt_buffer_write_string_prefixed(long handle, const char* s_ptr, long s_len, long s_cap)"));
    assert!(
        runtime_c_source().contains("long aic_rt_buffer_read_u32_be(long handle, long* out_value)")
    );
    assert!(runtime_c_source().contains("long aic_rt_buffer_write_u32_le(long handle, long value)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_buffer_patch_u32_be(long handle, long offset, long value)"));
    assert!(runtime_c_source().contains("long aic_rt_time_now_ms(void)"));
    assert!(runtime_c_source().contains("long aic_rt_time_monotonic_ms(void)"));
    assert!(runtime_c_source().contains("void aic_rt_time_sleep_ms(long ms)"));
    assert!(runtime_c_source().contains("long aic_rt_time_parse_rfc3339("));
    assert!(runtime_c_source().contains("long aic_rt_time_parse_iso8601("));
    assert!(runtime_c_source().contains("long aic_rt_time_format_rfc3339("));
    assert!(runtime_c_source().contains("long aic_rt_time_format_iso8601("));
    assert!(runtime_c_source().contains("AIC_TEST_TIME_MS"));
    assert!(runtime_c_source().contains("AIC_TEST_MODE"));
    assert!(runtime_c_source().contains("long aic_rt_signal_register(long signal_code)"));
    assert!(runtime_c_source().contains("long aic_rt_signal_wait(long* out_signal_code)"));
    assert!(runtime_c_source().contains("void aic_rt_rand_seed(long seed)"));
    assert!(runtime_c_source().contains("long aic_rt_rand_next(void)"));
    assert!(runtime_c_source().contains("long aic_rt_rand_range(long min_inclusive"));
    assert!(runtime_c_source().contains("AIC_TEST_SEED"));
    assert!(runtime_c_source()
        .contains("long aic_rt_mock_io_set_stdin(const char* ptr, long len, long cap)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_mock_io_take_stdout(char** out_ptr, long* out_len)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_mock_io_take_stderr(char** out_ptr, long* out_len)"));
    assert!(runtime_c_source().contains("AIC_TEST_NO_REAL_IO"));
    assert!(runtime_c_source().contains("AIC_TEST_IO_CAPTURE"));
    assert!(runtime_c_source().contains("long aic_rt_conc_spawn(long value, long delay_ms"));
    assert!(runtime_c_source().contains("long aic_rt_conc_join(long handle, long* out_value)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_join_timeout(long handle, long timeout_ms, long* out_value)"));
    assert!(runtime_c_source().contains("long aic_rt_conc_spawn_group("));
    assert!(runtime_c_source().contains("long aic_rt_conc_select_first("));
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_channel_int(long capacity, long* out_handle)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_channel_int_buffered(long capacity, long* out_handle)"));
    assert!(runtime_c_source().contains("long aic_rt_conc_try_send_int(long handle, long value)"));
    assert!(
        runtime_c_source().contains("long aic_rt_conc_try_recv_int(long handle, long* out_value)")
    );
    assert!(runtime_c_source().contains("long aic_rt_conc_select_recv_int("));
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_mutex_lock(long handle, long timeout_ms, long* out_value)"));
    assert!(runtime_c_source().contains(
        "long aic_rt_conc_rwlock_write_lock(long handle, long timeout_ms, long* out_value)"
    ));
    assert!(runtime_c_source().contains("long aic_rt_conc_arc_new("));
    assert!(
        runtime_c_source().contains("long aic_rt_conc_arc_clone(long handle, long* out_handle)")
    );
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_arc_get(long handle, char** out_ptr, long* out_len)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_arc_strong_count(long handle, long* out_count)"));
    assert!(runtime_c_source().contains("long aic_rt_conc_arc_release(long handle)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_atomic_int_new(long initial, long* out_handle)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_atomic_int_load(long handle, long* out_value)"));
    assert!(
        runtime_c_source().contains("long aic_rt_conc_atomic_int_store(long handle, long value)")
    );
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_atomic_int_add(long handle, long delta, long* out_old)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_atomic_int_sub(long handle, long delta, long* out_old)"));
    assert!(runtime_c_source().contains(
        "long aic_rt_conc_atomic_int_cas(long handle, long expected, long desired, long* out_swapped)"
    ));
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_atomic_bool_new(long initial, long* out_handle)"));
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_atomic_bool_load(long handle, long* out_value)"));
    assert!(
        runtime_c_source().contains("long aic_rt_conc_atomic_bool_store(long handle, long value)")
    );
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_atomic_bool_swap(long handle, long desired, long* out_old)"));
    assert!(runtime_c_source().contains(
        "long aic_rt_conc_tl_new(long entry_fn, long entry_env, long value_size, long* out_handle)"
    ));
    assert!(
        runtime_c_source().contains("long aic_rt_conc_tl_get(long handle, long* out_value_raw)")
    );
    assert!(runtime_c_source()
        .contains("long aic_rt_conc_tl_set(long handle, const char* value_ptr, long value_size)"));
    assert!(runtime_c_source().contains("#include <stdatomic.h>"));
    assert!(runtime_c_source().contains("atomic_fetch_add_explicit(&slot->ref_count"));
    assert!(runtime_c_source().contains("atomic_fetch_sub_explicit(&slot->ref_count"));
    assert!(runtime_c_source().contains("atomic_fetch_add_explicit(&slot->value"));
    assert!(runtime_c_source().contains("atomic_fetch_sub_explicit(&slot->value"));
    assert!(runtime_c_source().contains("long aic_rt_net_tcp_listen("));
    assert!(runtime_c_source().contains("long aic_rt_net_tcp_set_nodelay("));
    assert!(runtime_c_source().contains("long aic_rt_net_tcp_get_nodelay("));
    assert!(runtime_c_source().contains("long aic_rt_net_tcp_set_keepalive("));
    assert!(runtime_c_source().contains("long aic_rt_net_tcp_get_keepalive("));
    assert!(runtime_c_source().contains("long aic_rt_net_tcp_set_send_buffer_size("));
    assert!(runtime_c_source().contains("long aic_rt_net_tcp_get_send_buffer_size("));
    assert!(runtime_c_source().contains("long aic_rt_net_tcp_set_recv_buffer_size("));
    assert!(runtime_c_source().contains("long aic_rt_net_tcp_get_recv_buffer_size("));
    assert!(runtime_c_source().contains("long aic_rt_net_udp_recv_from("));
    assert!(runtime_c_source().contains("long aic_rt_net_dns_lookup("));
    assert!(runtime_c_source().contains("long aic_rt_net_dns_lookup_all("));
    assert!(runtime_c_source().contains("long aic_rt_net_async_accept_submit("));
    assert!(runtime_c_source().contains("long aic_rt_net_async_send_submit("));
    assert!(runtime_c_source().contains("long aic_rt_net_async_recv_submit("));
    assert!(runtime_c_source().contains("long aic_rt_net_async_wait_int("));
    assert!(runtime_c_source().contains("long aic_rt_net_async_wait_string("));
    assert!(runtime_c_source().contains("long aic_rt_net_async_shutdown(void)"));
    assert!(runtime_c_source().contains("long aic_rt_net_async_pressure("));
    assert!(runtime_c_source().contains("long aic_rt_tls_connect("));
    assert!(runtime_c_source().contains("long aic_rt_tls_connect_addr("));
    assert!(runtime_c_source().contains("long aic_rt_tls_accept("));
    assert!(runtime_c_source().contains("long aic_rt_tls_send("));
    assert!(runtime_c_source().contains("long aic_rt_tls_recv("));
    assert!(runtime_c_source().contains("long aic_rt_tls_async_send_submit("));
    assert!(runtime_c_source().contains("long aic_rt_tls_async_recv_submit("));
    assert!(runtime_c_source().contains("long aic_rt_tls_async_wait_int("));
    assert!(runtime_c_source().contains("long aic_rt_tls_async_wait_string("));
    assert!(runtime_c_source().contains("long aic_rt_tls_async_shutdown(void)"));
    assert!(runtime_c_source().contains("long aic_rt_tls_async_pressure("));
    assert!(runtime_c_source().contains("AIC_RT_TLS_ASYNC_OP_CAP"));
    assert!(runtime_c_source().contains("if (wait_rc == ETIMEDOUT)"));
    assert!(runtime_c_source().contains("op->claimed = 0;"));
    assert!(runtime_c_source().contains("long aic_rt_tls_close("));
    assert!(runtime_c_source().contains("long aic_rt_tls_peer_subject("));
    assert!(runtime_c_source().contains("long aic_rt_tls_peer_issuer("));
    assert!(runtime_c_source().contains("long aic_rt_tls_peer_fingerprint_sha256("));
    assert!(runtime_c_source().contains("long aic_rt_tls_peer_san_entries("));
    assert!(runtime_c_source().contains("long aic_rt_tls_version("));
    assert!(runtime_c_source().contains("long aic_rt_async_poll_int(long op_handle"));
    assert!(runtime_c_source().contains("long aic_rt_async_poll_string(long op_handle"));
    assert!(runtime_c_source().contains("long aic_rt_url_parse("));
    assert!(runtime_c_source().contains("long aic_rt_url_normalize("));
    assert!(runtime_c_source().contains("long aic_rt_url_net_addr("));
    assert!(runtime_c_source().contains("long aic_rt_http_parse_method("));
    assert!(runtime_c_source().contains("long aic_rt_http_status_reason("));
    assert!(runtime_c_source().contains("long aic_rt_http_validate_header("));
    assert!(runtime_c_source().contains("long aic_rt_http_validate_target("));
    assert!(runtime_c_source().contains("long aic_rt_http_server_listen("));
    assert!(runtime_c_source().contains("long aic_rt_http_server_read_request("));
    assert!(runtime_c_source().contains("long aic_rt_http_server_write_response("));
    assert!(runtime_c_source().contains("long aic_rt_router_new(long* out_handle)"));
    assert!(runtime_c_source().contains("long aic_rt_router_add("));
    assert!(runtime_c_source().contains("long aic_rt_router_match("));
    assert!(runtime_c_source().contains("long aic_rt_json_parse("));
    assert!(runtime_c_source().contains("long aic_rt_json_stringify("));
    assert!(runtime_c_source().contains("long aic_rt_json_decode_string("));
    assert!(runtime_c_source().contains("long aic_rt_json_object_set("));
    assert!(runtime_c_source().contains("long aic_rt_json_object_get("));
    assert!(runtime_c_source().contains("long aic_rt_regex_compile("));
    assert!(runtime_c_source().contains("long aic_rt_regex_captures("));
    assert!(runtime_c_source().contains("long aic_rt_regex_find("));
    assert!(runtime_c_source().contains("long aic_rt_regex_replace("));
    assert!(runtime_c_source().contains("void aic_rt_crypto_md5("));
    assert!(runtime_c_source().contains("void aic_rt_crypto_sha256("));
    assert!(runtime_c_source().contains("void aic_rt_crypto_hmac_sha256("));
    assert!(runtime_c_source().contains("long aic_rt_crypto_pbkdf2_sha256("));
    assert!(runtime_c_source().contains("long aic_rt_crypto_hex_decode("));
    assert!(runtime_c_source().contains("long aic_rt_crypto_base64_decode("));
    assert!(runtime_c_source().contains("void aic_rt_crypto_random_bytes("));
    assert!(runtime_c_source().contains("long aic_rt_crypto_secure_eq("));
}
