# Process, Environment, and Path APIs (IO-T2)

See also the complete IO runtime guide: `docs/io-runtime/README.md`.

This document describes `std.proc`, `std.env`, and `std.path`.
It covers the richer process/environment API set now used by the IO runtime docs and examples.

## Overview

- `std.proc` enables subprocess orchestration and shell-style pipelines.
- `std.env` provides environment-variable snapshots, working-directory APIs, and host metadata.
- `std.path` provides deterministic path utility helpers.
- Effects are explicit and enforced by typechecking.

## Effect Contracts

- Process APIs: `effects { proc }` or `effects { proc, env }`.
- Environment APIs: `effects { env }`; cwd APIs require `effects { env, fs }`.
- Path utilities are pure (no effect annotation).
- Windows caveat: process APIs are available, but backend/runtime failures can still surface as `ProcError::Io`, and missing/invalid handles can surface as `ProcError::UnknownProcess`.

## `std.proc`

```aic
enum ProcError {
    NotFound,
    PermissionDenied,
    InvalidInput,
    Io,
    UnknownProcess,
}

type ProcHandle = UInt32
type ProcExitStatus = Int32

struct ProcResult {
    status: ProcExitStatus,
    stdout: String,
    stderr: String,
}

struct RunOptions {
    stdin: String,
    cwd: String,
    env: Vec[String],
    timeout_ms: Int,
}

fn spawn(command: String) -> Result[ProcHandle, ProcError] effects { proc, env }
fn wait(handle: ProcHandle) -> Result[ProcExitStatus, ProcError] effects { proc }
fn kill(handle: ProcHandle) -> Result[Bool, ProcError] effects { proc }
fn run(command: String) -> Result[ProcResult, ProcError] effects { proc, env }
fn pipe(left: String, right: String) -> Result[ProcResult, ProcError] effects { proc, env }
fn run_with(command: String, options: RunOptions) -> Result[ProcResult, ProcError] effects { proc, env }
fn current_pid() -> Result[ProcHandle, ProcError] effects { proc }
fn is_running(handle: ProcHandle) -> Result[Bool, ProcError] effects { proc }
fn run_timeout(command: String, timeout_ms: Int) -> Result[ProcResult, ProcError] effects { proc, env }
fn pipe_chain(stages: Vec[String]) -> Result[ProcResult, ProcError] effects { proc, env }
```

Notes:
- `run` and `pipe` always return `Ok(ProcResult)` for successful launch, even when process exit status is non-zero.
- `ProcResult.status` carries bounded `Int32` process exit codes.
- Public wrappers validate runtime `Int` values before exposing `ProcHandle`/`ProcExitStatus`.
- `stderr` is captured independently from `stdout`.
- `run_with` is the explicit cwd/env/stdin control path; `run_timeout` and `pipe_chain` are the convenience wrappers used in current examples.

## `std.env`

```aic
enum EnvError {
    NotFound,
    PermissionDenied,
    InvalidInput,
    Io,
}

struct EnvEntry {
    key: String,
    value: String,
}

fn get(key: String) -> Result[String, EnvError] effects { env }
fn set(key: String, value: String) -> Result[Bool, EnvError] effects { env }
fn remove(key: String) -> Result[Bool, EnvError] effects { env }
fn cwd() -> Result[String, EnvError] effects { env, fs }
fn set_cwd(path: String) -> Result[Bool, EnvError] effects { env, fs }
fn args() -> Vec[String] effects { env }
fn arg_count() -> Int effects { env }
fn arg_at(index: Int) -> Option[String] effects { env }
fn all_vars() -> Vec[EnvEntry] effects { env }
fn home_dir() -> Result[String, EnvError] effects { env, fs }
fn temp_dir() -> Result[String, EnvError] effects { env, fs }
fn os_name() -> String effects { env }
fn arch() -> String effects { env }
fn exit(code: Int32) -> () effects { env }
```

Notes:
- `args`, `arg_count`, and `arg_at` expose the process argument snapshot.
- `all_vars` returns a deterministic snapshot of environment key/value pairs.
- `home_dir`, `temp_dir`, `os_name`, and `arch` are the stable metadata helpers used by config/bootstrap code.

## `std.path`

```aic
fn join(left: String, right: String) -> String
fn basename(path: String) -> String
fn dirname(path: String) -> String
fn extension(path: String) -> String
fn is_abs(path: String) -> Bool
```

## Runtime ABI

LLVM/runtime entrypoints used by codegen:

- `aic_rt_proc_spawn`
- `aic_rt_proc_wait`
- `aic_rt_proc_kill`
- `aic_rt_proc_run`
- `aic_rt_proc_pipe`
- `aic_rt_env_get`
- `aic_rt_env_set`
- `aic_rt_env_remove`
- `aic_rt_env_cwd`
- `aic_rt_env_set_cwd`
- `aic_rt_path_join`
- `aic_rt_path_basename`
- `aic_rt_path_dirname`
- `aic_rt_path_extension`
- `aic_rt_path_is_abs`

## Example

- `examples/io/process_pipeline.aic`
