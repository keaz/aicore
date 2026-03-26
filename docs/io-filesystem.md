# Filesystem API (IO-T1)

See also the complete IO runtime guide: `docs/io-runtime/README.md`.

This document is the implementation and usage contract for `std.fs`.
It covers the current surface: byte APIs, file-handle APIs, directory helpers, symlink operations, and readonly toggles.

## Overview

- All filesystem APIs are explicit-effect APIs: `effects { fs }`.
- `null` is never used in source-level APIs.
- Fallible APIs return `Result[_, FsError]`.
- Error categories are stable and deterministic across platforms.
- `read_bytes` / `write_bytes` / `append_bytes` are the binary-oriented boundary APIs.
- `open_read` / `open_write` / `open_append` and `file_read_line` / `file_write_str` / `file_close` are the handle lifecycle APIs.
- `mkdir`, `mkdir_all`, `rmdir`, `list_dir`, `walk_dir`, `temp_file`, and `temp_dir` cover directory and temp-path workflows.
- `create_symlink`, `read_symlink`, and `set_readonly` are explicit and may fail differently by platform.

## Types

```aic
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
```

### Stable error mapping

Runtime maps host errors to `FsError` as follows:

- `NotFound`: missing path/file (`ENOENT`, Windows file/path-not-found)
- `PermissionDenied`: ACL/permission failures (`EACCES`, `EPERM`, Windows access denied)
- `AlreadyExists`: destination/resource already exists (`EEXIST`)
- `InvalidInput`: invalid path/input (`EINVAL`, empty path)
- `Io`: any other IO failure

## API Surface

```aic
fn exists(path: String) -> Bool effects { fs }
fn read_text(path: String) -> Result[String, FsError] effects { fs }
fn write_text(path: String, content: String) -> Result[Bool, FsError] effects { fs }
fn append_text(path: String, content: String) -> Result[Bool, FsError] effects { fs }
fn copy(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs }
fn move(from_path: String, to_path: String) -> Result[Bool, FsError] effects { fs }
fn delete(path: String) -> Result[Bool, FsError] effects { fs }
fn metadata(path: String) -> Result[FsMetadata, FsError] effects { fs }
fn walk_dir(path: String) -> Result[Vec[String], FsError] effects { fs }
fn temp_file(prefix: String) -> Result[String, FsError] effects { fs }
fn temp_dir(prefix: String) -> Result[String, FsError] effects { fs }
```

Notes:

- `Bool` results for write/copy/move/delete are success flags (`true` on success).
- `read_bytes`, `write_bytes`, and `append_bytes` are the binary-oriented path helpers.
- Always pair `open_*` with `file_close` in long-lived flows.
- `walk_dir` returns a `Vec[String]` snapshot shape; `vec_len` is currently the primary utility in MVP.
- `temp_file` and `temp_dir` return absolute paths.
- `list_dir` returns concrete directory entry strings.
- `create_symlink`, `read_symlink`, and `set_readonly` expose platform-sensitive path controls explicitly.

## Effect Enforcement

Pure functions cannot call `std.fs` APIs.

```aic
import std.fs;

fn invalid() -> Int {
    let _ = read_text("foo.txt"); // E2001
    0
}
```

Valid usage:

```aic
import std.fs;

fn load() -> Int effects { fs } {
    match read_text("foo.txt") {
        Ok(_) => 1,
        Err(_) => 0,
    }
}
```

## Runtime ABI

LLVM runtime functions used by codegen:

- `aic_rt_fs_exists`
- `aic_rt_fs_read_text`
- `aic_rt_fs_write_text`
- `aic_rt_fs_append_text`
- `aic_rt_fs_copy`
- `aic_rt_fs_move`
- `aic_rt_fs_delete`
- `aic_rt_fs_metadata`
- `aic_rt_fs_walk_dir`
- `aic_rt_fs_temp_file`
- `aic_rt_fs_temp_dir`

These are implemented in `src/codegen/mod.rs` runtime shim (`runtime_c_source()`).

## Example

Reference example:

- `examples/io/fs_backup.aic`

It exercises:

- write/copy/read/delete
- metadata and walk
- temp file creation
- deterministic `FsError` handling via pattern matching
