# Filesystem API (IO-T1)

See also the complete IO runtime guide: `docs/io-runtime/README.md`.

This document is the implementation and usage contract for `std.fs`.

## Overview

- All filesystem APIs are explicit-effect APIs: `effects { fs }`.
- `null` is never used in source-level APIs.
- Fallible APIs return `Result[_, FsError]`.
- Error categories are stable and deterministic across platforms.

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
- `walk_dir` returns a `Vec[String]` snapshot shape; `vec_len` is currently the primary utility in MVP.
- `temp_file` and `temp_dir` return absolute paths.

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

These are implemented in `src/codegen.rs` runtime shim (`runtime_c_source()`).

## Example

Reference example:

- `examples/io/fs_backup.aic`

It exercises:

- write/copy/read/delete
- metadata and walk
- temp file creation
- deterministic `FsError` handling via pattern matching
