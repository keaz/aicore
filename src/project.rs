use std::fs;
use std::path::Path;

pub fn init_project(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path)?;
    fs::create_dir_all(path.join("src"))?;
    fs::create_dir_all(path.join("std"))?;
    fs::create_dir_all(path.join("examples"))?;
    fs::create_dir_all(path.join("docs"))?;
    fs::create_dir_all(path.join("tests"))?;

    fs::write(
        path.join("aic.toml"),
        "[package]\nname = \"sample\"\nmain = \"src/main.aic\"\n",
    )?;

    fs::write(
        path.join("src/main.aic"),
        r#"module sample.main;

import std.io;

fn maybe_even(x: Int) -> Option[Int] {
    if x % 2 == 0 {
    Some(x)
} else {
    None()
}
}

fn main() -> Int effects { io } {
    let v = maybe_even(10);
    let out = match v {
    Some(n) => n,
    None => 0,
};
    print_int(out);
    0
}
"#,
    )?;

    fs::write(
        path.join("std/option.aic"),
        r#"module std.option;

enum Option[T] {
    None,
    Some(T),
}
"#,
    )?;

    fs::write(
        path.join("std/result.aic"),
        r#"module std.result;

enum Result[T, E] {
    Ok(T),
    Err(E),
}
"#,
    )?;

    fs::write(
        path.join("std/io.aic"),
        r#"module std.io;

fn print_int(x: Int) -> () effects { io } {
    ()
}

fn print_str(x: String) -> () effects { io } {
    ()
}

fn panic(message: String) -> () effects { io } {
    ()
}
"#,
    )?;

    fs::write(
        path.join("std/string.aic"),
        r#"module std.string;

fn len(s: String) -> Int {
    0
}

fn is_empty(s: String) -> Bool {
    len(s) == 0
}
"#,
    )?;

    fs::write(
        path.join("std/vec.aic"),
        r#"module std.vec;

struct Vec[T] {
    ptr: Int,
    len: Int,
    cap: Int,
}

fn vec_len[T](v: Vec[T]) -> Int {
    v.len
}

fn vec_cap[T](v: Vec[T]) -> Int {
    v.cap
}
"#,
    )?;

    fs::write(
        path.join("std/fs.aic"),
        r#"module std.fs;

import std.result;
import std.vec;

enum FsError {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    InvalidInput,
    Io,
}

struct FsMetadata {
    is_file: Bool,
    is_dir: Bool,
    size: Int,
}

fn aic_fs_exists_intrinsic(path: String) -> Bool effects { fs } {
    false
}

fn aic_fs_read_text_intrinsic(path: String) -> Result[String, FsError] effects { fs } {
    let out: Result[String, FsError] = Ok("");
    out
}

fn aic_fs_write_text_intrinsic(path: String, content: String) -> Result[Bool, FsError] effects { fs } {
    let out: Result[Bool, FsError] = Ok(true);
    out
}

fn aic_fs_append_text_intrinsic(path: String, content: String) -> Result[Bool, FsError] effects { fs } {
    let out: Result[Bool, FsError] = Ok(true);
    out
}

fn aic_fs_copy_intrinsic(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs } {
    let out: Result[Bool, FsError] = Ok(true);
    out
}

fn aic_fs_move_intrinsic(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs } {
    let out: Result[Bool, FsError] = Ok(true);
    out
}

fn aic_fs_delete_intrinsic(path: String) -> Result[Bool, FsError] effects { fs } {
    let out: Result[Bool, FsError] = Ok(true);
    out
}

fn aic_fs_metadata_intrinsic(path: String) -> Result[FsMetadata, FsError] effects { fs } {
    let out: Result[FsMetadata, FsError] = Ok(FsMetadata {
        is_file: false,
        is_dir: false,
        size: 0,
    });
    out
}

fn aic_fs_walk_dir_intrinsic(path: String) -> Result[Vec[String], FsError] effects { fs } {
    let out: Result[Vec[String], FsError] = Ok(Vec {
        ptr: 0,
        len: 0,
        cap: 0,
    });
    out
}

fn aic_fs_temp_file_intrinsic(prefix: String) -> Result[String, FsError] effects { fs } {
    let out: Result[String, FsError] = Ok("");
    out
}

fn aic_fs_temp_dir_intrinsic(prefix: String) -> Result[String, FsError] effects { fs } {
    let out: Result[String, FsError] = Ok("");
    out
}

fn exists(path: String) -> Bool effects { fs } {
    aic_fs_exists_intrinsic(path)
}

fn read_text(path: String) -> Result[String, FsError] effects { fs } {
    aic_fs_read_text_intrinsic(path)
}

fn write_text(path: String, content: String) -> Result[Bool, FsError] effects { fs } {
    aic_fs_write_text_intrinsic(path, content)
}

fn append_text(path: String, content: String) -> Result[Bool, FsError] effects { fs } {
    aic_fs_append_text_intrinsic(path, content)
}

fn copy(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs } {
    aic_fs_copy_intrinsic(from_path, to_path)
}

fn move(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs } {
    aic_fs_move_intrinsic(from_path, to_path)
}

fn delete(path: String) -> Result[Bool, FsError] effects { fs } {
    aic_fs_delete_intrinsic(path)
}

fn metadata(path: String) -> Result[FsMetadata, FsError] effects { fs } {
    aic_fs_metadata_intrinsic(path)
}

fn walk_dir(path: String) -> Result[Vec[String], FsError] effects { fs } {
    aic_fs_walk_dir_intrinsic(path)
}

fn temp_file(prefix: String) -> Result[String, FsError] effects { fs } {
    aic_fs_temp_file_intrinsic(prefix)
}

fn temp_dir(prefix: String) -> Result[String, FsError] effects { fs } {
    aic_fs_temp_dir_intrinsic(prefix)
}
"#,
    )?;

    fs::write(
        path.join("std/net.aic"),
        r#"module std.net;

fn tcp_connect(addr: String) -> Int effects { net } {
    0
}

fn tcp_send(handle: Int, payload: String) -> () effects { net } {
    ()
}
"#,
    )?;

    fs::write(
        path.join("std/time.aic"),
        r#"module std.time;

fn now_ms() -> Int effects { time } {
    0
}

fn now() -> Int effects { time } {
    now_ms()
}

fn sleep_ms(ms: Int) -> () effects { time } {
    ()
}
"#,
    )?;

    fs::write(
        path.join("std/rand.aic"),
        r#"module std.rand;

fn random_int() -> Int effects { rand } {
    4
}

fn random_bool() -> Bool effects { rand } {
    true
}
"#,
    )?;

    Ok(())
}
