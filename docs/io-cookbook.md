# IO Cookbook

This cookbook provides deterministic implementation patterns for the current IO runtime.
Use these patterns when writing examples, agent workflows, or production-oriented AIC programs.

## 1. Interactive Input With Typed Fallbacks

Use `std.io` read APIs and branch on `IoError` instead of panicking.

```aic
import std.io;

fn read_name() -> String effects { io } {
    match prompt("Name: ") {
        Ok(value) => value,
        Err(EndOfInput) => "guest",
        Err(InvalidInput) => "guest",
        Err(Io) => "guest",
    }
}
```

Reference: `examples/io/interactive_greeter.aic`.

## 2. File Processing With Handle APIs

Prefer handle APIs for multi-line workflows.

```aic
import std.fs;

fn unwrap_handle(v: Result[FileHandle, FsError]) -> FileHandle {
    match v {
        Ok(handle) => handle,
        Err(_) => FileHandle { handle: 0 },
    }
}

fn process() -> Int effects { fs } {
    let reader = unwrap_handle(open_read("in.txt"));
    let first = file_read_line(reader);
    let _ = first;
    let _ = file_close(reader);
    0
}
```

References:

- `examples/io/file_processor.aic`
- `examples/io/line_reader.aic`

## 3. Tee-Style Logging (stdout + stderr + file)

Use stdout for user-facing status, stderr for operator diagnostics, and `std.fs` for persistent logs.

```aic
import std.io;
import std.fs;

fn log_once(path: String) -> Int effects { io, fs } {
    let _ = append_text(path, "INFO startup\n");
    println_str("INFO startup");
    eprint_str("WARN startup");
    flush_stdout();
    flush_stderr();
    0
}
```

Reference: `examples/io/log_tee.aic`.

## 4. Environment-Driven Config

Use `std.env.get` for optional values and keep defaults local.

```aic
import std.env;

fn config_token() -> String effects { env } {
    match get("AIC_TOKEN") {
        Ok(value) => value,
        Err(NotFound) => "default-token",
        Err(_) => "default-token",
    }
}
```

Reference: `examples/io/env_config.aic`.

## 5. Subprocess Pipelines

Use `run`/`pipe` for short commands, and `run_with` when cwd/env/stdin must be controlled.

```aic
import std.proc;
import std.vec;

fn pipeline_demo() -> Result[ProcOutput, ProcError] effects { proc, env } {
    let mut stages: Vec[String] = vec.vec_of("printf 'hello'");
    stages = vec.push(stages, "cat");
    pipe_chain(stages)
}
```

Reference: `examples/io/subprocess_pipeline.aic`.

## 6. Time + Retry Skeleton

Use monotonic deadlines for timeout logic; avoid wall-clock arithmetic for retry waits.

```aic
import std.time;

fn retry_wait(ms: Int) -> () effects { time } {
    let deadline = deadline_after_ms(ms);
    sleep_until(deadline);
    ()
}
```

Reference: `examples/io/retry_with_jitter.aic`.

## 7. Platform Caveat Patterns

When cross-platform behavior differs, branch on typed errors.

```aic
import std.net;

fn connect_or_skip(addr: String) -> Int effects { net } {
    match tcp_connect(addr, 500) {
        Ok(_) => 1,
        Err(Io) => 0,
        Err(_) => 0,
    }
}
```

Current runtime caveats to account for:

- Windows `std.net`: currently returns `NetError::Io`.
- Windows `std.proc`: `spawn/run_with/run_timeout/pipe_chain` can return `ProcError::Io`; `wait/kill/is_running` return `ProcError::UnknownProcess`.
