# Process, Environment, and Path APIs (IO-T2)

See also the complete IO runtime guide: `docs/io-runtime/README.md`.

This document describes `std.proc`, `std.env`, and `std.path`.

## Overview

- `std.proc` enables subprocess orchestration and shell-style pipelines.
- `std.env` provides environment-variable and working-directory APIs.
- `std.path` provides deterministic path utility helpers.
- Effects are explicit and enforced by typechecking.

## Effect Contracts

- Process APIs: `effects { proc }` or `effects { proc, env }`.
- Environment APIs: `effects { env }`; cwd APIs require `effects { env, fs }`.
- Path utilities are pure (no effect annotation).

## `std.proc`

```aic
enum ProcError {
    NotFound,
    PermissionDenied,
    InvalidInput,
    Io,
    UnknownProcess,
}

struct ProcOutput {
    status: Int,
    stdout: String,
    stderr: String,
}

fn spawn(command: String) -> Result[Int, ProcError] effects { proc, env }
fn wait(handle: Int) -> Result[Int, ProcError] effects { proc }
fn kill(handle: Int) -> Result[Bool, ProcError] effects { proc }
fn run(command: String) -> Result[ProcOutput, ProcError] effects { proc, env }
fn pipe(left: String, right: String) -> Result[ProcOutput, ProcError] effects { proc, env }
```

Notes:
- `run` and `pipe` always return `Ok(ProcOutput)` for successful launch, even when process exit status is non-zero.
- `ProcOutput.status` carries the process exit code.
- `stderr` is captured independently from `stdout`.

## `std.env`

```aic
enum EnvError {
    NotFound,
    PermissionDenied,
    InvalidInput,
    Io,
}

fn get(key: String) -> Result[String, EnvError] effects { env }
fn set(key: String, value: String) -> Result[Bool, EnvError] effects { env }
fn remove(key: String) -> Result[Bool, EnvError] effects { env }
fn cwd() -> Result[String, EnvError] effects { env, fs }
fn set_cwd(path: String) -> Result[Bool, EnvError] effects { env, fs }
```

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
