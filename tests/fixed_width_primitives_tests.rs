use std::fs;

use aicore::driver::{has_errors, run_frontend};
use tempfile::tempdir;

fn write_main(root: &std::path::Path, source: &str) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(root.join("src/main.aic"), source).expect("write main.aic");
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
    let v_u64: UInt64 = 21;
    let patch_u16: UInt16 = 10;
    let patch_u32: UInt32 = 20;
    let patch_u64: UInt64 = 30;
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
}
