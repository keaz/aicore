use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use aicore::ast::{Item, Stmt, TypeKind};
use aicore::driver::{has_errors, run_frontend};
use aicore::formatter::format_program;
use aicore::ir_builder::build;
use aicore::parser::parse;
use aicore::toolchain::ENV_AIC_STD_ROOT;
use tempfile::tempdir;

fn write_main(root: &std::path::Path, source: &str) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(root.join("src/main.aic"), source).expect("write main.aic");
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock")
}

struct ScopedEnvVar {
    name: &'static str,
    previous: Option<String>,
}

impl ScopedEnvVar {
    fn set(name: &'static str, value: String) -> Self {
        let previous = std::env::var(name).ok();
        std::env::set_var(name, value);
        Self { name, previous }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            std::env::set_var(self.name, value);
        } else {
            std::env::remove_var(self.name);
        }
    }
}

fn with_local_std_root<T>(f: impl FnOnce() -> T) -> T {
    let _guard = env_lock();
    let std_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("std")
        .to_string_lossy()
        .into_owned();
    let _std_root = ScopedEnvVar::set(ENV_AIC_STD_ROOT, std_root);
    f()
}

#[test]
fn fixed_width_primitive_annotations_typecheck() {
    let dir = tempdir().expect("tempdir");
    write_main(
        dir.path(),
        r#"module app.main;

fn narrow_types(
    a: Int8,
    b: Int16,
    c: Int32,
    d: Int64,
    i: Int128,
    e: UInt8,
    f: UInt16,
    g: UInt32,
    h: UInt64,
    j: UInt128,
) -> Int {
    let _a: Int8 = a;
    let _b: Int16 = b;
    let _c: Int32 = c;
    let _d: Int64 = d;
    let _i: Int128 = i;
    let _e: UInt8 = e;
    let _f: UInt16 = f;
    let _g: UInt32 = g;
    let _h: UInt64 = h;
    let _j: UInt128 = j;
    let _sum: Int32 = c + 1;
    let _wide: Int128 = i + 1i128;
    let _shifted: UInt32 = g >>> 1;
    let _wide_shifted: UInt128 = j >>> 1u128;
    0
}

fn main() -> Int {
    narrow_types(1, 2, 3, 4, 5i128, 6, 7, 8, 9u64, 10u128)
}
"#,
    );

    let out = run_frontend(&dir.path().join("src/main.aic")).expect("frontend");
    assert!(
        !has_errors(&out.diagnostics),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn std_buffer_width_specific_signatures_typecheck() {
    with_local_std_root(|| {
        let dir = tempdir().expect("tempdir");
        write_main(
            dir.path(),
            r#"module app.main;
import std.buffer;

fn main() -> Int {
    let v_u8: UInt8 = 255;
    let v_i16: Int16 = -7;
    let v_u16: UInt16 = 7;
    let v_i32: Int32 = -11;
    let v_u32: UInt32 = 11;
    let v_i64: Int64 = -21;
    let v_u64: UInt64 = 21u64;
    let patch_u16: UInt16 = 10;
    let patch_u32: UInt32 = 20;
    let patch_u64: UInt64 = 30u64;
    let peek_pos: Int = 0;

    let b0 = new_buffer(64);
    let _w0 = buf_write_u8(b0, v_u8);
    let b1 = new_buffer(64);
    let _w1 = buf_write_i16_be(b1, v_i16);
    let b2 = new_buffer(64);
    let _w2 = buf_write_u16_le(b2, v_u16);
    let b3 = new_buffer(64);
    let _w3 = buf_write_i32_le(b3, v_i32);
    let b4 = new_buffer(64);
    let _w4 = buf_write_u32_be(b4, v_u32);
    let b5 = new_buffer(64);
    let _w5 = buf_write_i64_be(b5, v_i64);
    let b6 = new_buffer(64);
    let _w6 = buf_write_u64_le(b6, v_u64);

    let p0 = new_buffer(64);
    let _p0 = buf_patch_u16_be(p0, 0, patch_u16);
    let p1 = new_buffer(64);
    let _p1 = buf_patch_u32_le(p1, 1, patch_u32);
    let p2 = new_buffer(64);
    let _p2 = buf_patch_u64_be(p2, 2, patch_u64);

    let r0 = new_buffer(64);
    let _r0 = buf_read_u8(r0);
    let r1 = new_buffer(64);
    let _r1 = buf_read_i16_be(r1);
    let r2 = new_buffer(64);
    let _r2 = buf_read_u32_le(r2);
    let r3 = new_buffer(64);
    let _peek = buf_peek_u8(r3, peek_pos);
    0
}
"#,
        );

        let out = run_frontend(&dir.path().join("src/main.aic")).expect("frontend");
        assert!(
            !has_errors(&out.diagnostics),
            "diags={:#?}",
            out.diagnostics
        );
    });
}

#[test]
fn std_buffer_typed_result_helpers_typecheck() {
    with_local_std_root(|| {
        let dir = tempdir().expect("tempdir");
        write_main(
            dir.path(),
            r#"module app.main;
import std.buffer;

fn u8_or(v: Result[UInt8, BufferError], fallback: UInt8) -> UInt8 {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn u16_or(v: Result[UInt16, BufferError], fallback: UInt16) -> UInt16 {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn i16_or(v: Result[Int16, BufferError], fallback: Int16) -> Int16 {
    match v {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn main() -> Int {
    let buf = new_buffer(32);
    let _w0 = buf_write_u8(buf, 9);
    let _w1 = buf_write_u16_be(buf, 17);
    let _w2 = buf_write_i16_le(buf, -3);
    let _p0 = buf_patch_u16_le(buf, 0, 7);

    buf_reset(buf);
    let _r0: UInt8 = u8_or(buf_read_u8(buf), 0);
    let _r1: UInt16 = u16_or(buf_read_u16_be(buf), 0);
    let _r2: Int16 = i16_or(buf_read_i16_le(buf), 0);
    0
}
"#,
        );

        let out = run_frontend(&dir.path().join("src/main.aic")).expect("frontend");
        assert!(
            !has_errors(&out.diagnostics),
            "diags={:#?}",
            out.diagnostics
        );
    });
}

#[test]
fn fixed_width_operator_mismatch_reports_deterministic_diagnostics() {
    let dir = tempdir().expect("tempdir");
    write_main(
        dir.path(),
        r#"module app.main;

fn main(a: Int8, b: UInt16) -> Int {
    let _bad_add = a + b;
    let _bad_cmp = a < b;
    0
}
"#,
    );

    let out = run_frontend(&dir.path().join("src/main.aic")).expect("frontend");
    assert!(has_errors(&out.diagnostics), "diags={:#?}", out.diagnostics);
    assert!(
        out.diagnostics.iter().any(|d| {
            d.code == "E1230"
                && d.message.contains("matching integer signedness/width")
                && d.message.contains("Int8")
                && d.message.contains("UInt16")
        }),
        "diags={:#?}",
        out.diagnostics
    );
    assert!(
        out.diagnostics.iter().any(|d| {
            d.code == "E1232"
                && d.message.contains("fixed-width integers")
                && d.message.contains("Int8")
                && d.message.contains("UInt16")
        }),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn fixed_width_integer_literal_narrowing_and_range_diagnostics() {
    let dir = tempdir().expect("tempdir");
    write_main(
        dir.path(),
        r#"module app.main;

fn main() -> Int {
    let _ok_u8: UInt8 = 255;
    let _bad_u8: UInt8 = 256;
    let _ok_i8: Int8 = -128;
    let _bad_i8: Int8 = -129;
    0
}
"#,
    );

    let out = run_frontend(&dir.path().join("src/main.aic")).expect("frontend");
    assert!(has_errors(&out.diagnostics), "diags={:#?}", out.diagnostics);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "E1204" && d.message.contains("UInt8")),
        "diags={:#?}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "E1204" && d.message.contains("Int8")),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn int128_uint128_literal_boundaries_and_failures_typecheck() {
    let dir = tempdir().expect("tempdir");
    write_main(
        dir.path(),
        r#"module app.main;

fn main() -> Int {
    let _ok_i128_min: Int128 = -170141183460469231731687303715884105728i128;
    let _ok_u128_max: UInt128 = 340282366920938463463374607431768211455u128;
    let _bad_i128: Int128 = 170141183460469231731687303715884105728i128;
    let _bad_u128_neg: UInt128 = -1u128;
    0
}
"#,
    );

    let out = run_frontend(&dir.path().join("src/main.aic")).expect("frontend");
    assert!(has_errors(&out.diagnostics), "diags={:#?}", out.diagnostics);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "E1204" && d.message.contains("Int128")),
        "diags={:#?}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "E1204" && d.message.contains("UInt128")),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn fixed_width_integer_pattern_range_diagnostics() {
    let dir = tempdir().expect("tempdir");
    write_main(
        dir.path(),
        r#"module app.main;

fn main(x: UInt8, y: Int8) -> Int {
    let a = match x {
        255u8 => 1,
        256 => 2,
        _ => 0,
    };
    let b = match y {
        127 => 1,
        128 => 2,
        _ => 0,
    };
    a + b
}
"#,
    );

    let out = run_frontend(&dir.path().join("src/main.aic")).expect("frontend");
    assert!(has_errors(&out.diagnostics), "diags={:#?}", out.diagnostics);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "E1234" && d.message.contains("UInt8")),
        "diags={:#?}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "E1234" && d.message.contains("Int8")),
        "diags={:#?}",
        out.diagnostics
    );
}

#[test]
fn fixed_width_assignments_reject_lossy_or_sign_changing_conversions() {
    let dir = tempdir().expect("tempdir");
    write_main(
        dir.path(),
        r#"module app.main;

fn main(a: Int16, b: UInt16) -> Int {
    let _widen_ok: Int32 = a;
    let _bad_narrow: Int8 = a;
    let _bad_sign: UInt16 = a;
    let _bad_unsigned_to_signed: Int8 = b;
    0
}
"#,
    );

    let out = run_frontend(&dir.path().join("src/main.aic")).expect("frontend");
    assert!(has_errors(&out.diagnostics), "diags={:#?}", out.diagnostics);
    let conversion_diags = out.diagnostics.iter().filter(|d| d.code == "E1204").count();
    assert!(
        conversion_diags >= 3,
        "expected conversion diagnostics, got {conversion_diags}: {:#?}",
        out.diagnostics
    );
}

#[test]
fn fixed_width_int128_uint128_assignments_enforce_lossless_rules() {
    let dir = tempdir().expect("tempdir");
    write_main(
        dir.path(),
        r#"module app.main;

fn main(a: Int64, b: UInt64, c: Int128, d: UInt128) -> Int {
    let _ok_signed_widen: Int128 = a;
    let _ok_unsigned_widen: UInt128 = b;
    let _bad_signed_to_unsigned: UInt128 = c;
    let _bad_unsigned_to_signed: Int128 = d;
    0
}
"#,
    );

    let out = run_frontend(&dir.path().join("src/main.aic")).expect("frontend");
    assert!(has_errors(&out.diagnostics), "diags={:#?}", out.diagnostics);
    let conversion_diags = out.diagnostics.iter().filter(|d| d.code == "E1204").count();
    assert!(
        conversion_diags >= 2,
        "expected conversion diagnostics, got {conversion_diags}: {:#?}",
        out.diagnostics
    );
}

#[test]
fn frontend_size_primitives_parse_in_signatures_and_type_positions() {
    let src = r#"module app.main;

type Counter = UInt;

fn next(index: USize, delta: UInt, signed: ISize) -> UInt {
    let _index: USize = index;
    let _delta: UInt = delta;
    let _signed: ISize = signed;
    delta
}
"#;
    let (program, diagnostics) = parse(src, "test.aic");
    assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
    let program = program.expect("program");
    let f = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Function(f) if f.name == "next" => Some(f),
            _ => None,
        })
        .expect("next function");

    let expected = ["USize", "USize", "ISize"];
    for (param, expected_name) in f.params.iter().zip(expected.iter()) {
        assert!(matches!(
            &param.ty.kind,
            TypeKind::Named { name, args } if name == expected_name && args.is_empty()
        ));
    }
    assert!(matches!(
        &f.ret_type.kind,
        TypeKind::Named { name, args } if name == "USize" && args.is_empty()
    ));

    let mut let_type_names = Vec::new();
    for stmt in &f.body.stmts {
        if let Stmt::Let { ty: Some(ty), .. } = stmt {
            if let TypeKind::Named { name, args } = &ty.kind {
                assert!(args.is_empty(), "expected non-generic type, got {name}[..]");
                let_type_names.push(name.as_str());
            }
        }
    }
    assert_eq!(let_type_names, vec!["USize", "USize", "ISize"]);
}

#[test]
fn frontend_size_primitives_round_trip_is_deterministic() {
    let src = r#"module app.main;

fn bump(index: USize, delta: UInt, signed: ISize) -> UInt {
    let step: UInt = delta;
    if signed < 0 {
        step
    } else {
        index + step
    }
}
"#;
    let (program, diagnostics) = parse(src, "roundtrip.aic");
    assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
    let ir = build(&program.expect("program"));
    let formatted_once = format_program(&ir);

    let (reparsed, reparsed_diags) = parse(&formatted_once, "roundtrip_formatted.aic");
    assert!(
        reparsed_diags.is_empty(),
        "reparse diagnostics={reparsed_diags:#?}\nformatted={formatted_once}"
    );
    let reformatted = format_program(&build(&reparsed.expect("program")));

    assert_eq!(formatted_once, reformatted);
    assert!(
        formatted_once.contains("ISize"),
        "formatted={formatted_once}"
    );
    assert!(
        formatted_once.contains("USize"),
        "formatted={formatted_once}"
    );
    assert!(
        !formatted_once.contains("UInt"),
        "formatted={formatted_once}"
    );
}
