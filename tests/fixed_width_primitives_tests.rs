use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use aicore::driver::{has_errors, run_frontend};
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
    e: UInt8,
    f: UInt16,
    g: UInt32,
    h: UInt64,
) -> Int {
    let _a: Int8 = a;
    let _b: Int16 = b;
    let _c: Int32 = c;
    let _d: Int64 = d;
    let _e: UInt8 = e;
    let _f: UInt16 = f;
    let _g: UInt32 = g;
    let _h: UInt64 = h;
    let _sum: Int32 = c + 1;
    let _shifted: UInt32 = g >>> 1;
    0
}

fn main() -> Int {
    narrow_types(1, 2, 3, 4, 5, 6, 7, 8)
}
"#,
    );

    let out = run_frontend(&dir.path().join("src/main.aic")).expect("frontend");
    assert!(!has_errors(&out.diagnostics), "diags={:#?}", out.diagnostics);
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
        assert!(!has_errors(&out.diagnostics), "diags={:#?}", out.diagnostics);
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
    let conversion_diags = out
        .diagnostics
        .iter()
        .filter(|d| d.code == "E1204")
        .count();
    assert!(
        conversion_diags >= 3,
        "expected conversion diagnostics, got {conversion_diags}: {:#?}",
        out.diagnostics
    );
}
