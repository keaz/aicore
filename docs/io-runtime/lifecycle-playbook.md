# IO Runtime Lifecycle And Service Playbook

This document provides agent-focused lifecycle rules, capability boundaries, and long-running service constraints.

## Resource Lifecycle Rules

### Filesystem

- Use `temp_file`/`temp_dir` for scratch paths.
- Prefer `move` over copy+delete for finalization.
- Treat `delete` as best-effort cleanup with explicit error matching.

### Processes

- `spawn` must be paired with `wait` (or `kill` then `wait`) to avoid leaked process state.
- Prefer `run`/`pipe` for short-lived commands with captured output.

### Networking

- Close all handles (`tcp_close`/`udp_close`) in reverse acquisition order.
- Use explicit receive/send timeout values to prevent unbounded blocking.

### Concurrency

- Join or cancel+join every spawned task.
- Close channels/mutexes during shutdown paths.
- Treat handle tables as bounded runtime resources, not unbounded queues.
- Use `send_int` for backpressure-aware writes (blocks until capacity or timeout).
- Use `try_send_int` / `try_recv_int` when the caller must stay non-blocking.
- `select_recv_int` currently selects across two `IntChannel` handles; use it instead of ad-hoc busy loops.

## Capability Boundaries For Agents

- Keep effectful IO in orchestration functions.
- Keep domain logic pure where possible.
- Use wrapper functions that convert runtime enums to app-specific policy decisions.

## Failure-Handling Templates

### Template: process fallback

```aic
match run("cmd") {
    Ok(out) => out,
    Err(_) => ProcOutput { status: 1, stdout: "", stderr: "" },
}
```

### Template: network timeout policy

```aic
match tcp_recv(conn, 4096, 1000) {
    Ok(payload) => payload,
    Err(err) => match err {
        Timeout => "",
        _ => "",
    },
}
```

### Template: cooperative task cancellation

```aic
let cancelled = cancel_task(task);
let _joined = join_task(task);
```

## Performance Constraints (Current Runtime)

- Handle tables are bounded in runtime implementation:
  - process handles: 64
  - network handles: 128
  - task handles: 128
  - channel handles: 128
  - mutex handles: 128
- `IntChannel` capacity is bounded and validated.
- Runtime uses blocking host primitives; avoid creating unbounded numbers of threads or open descriptors.
- Channel select uses short timed polling under the hood; prefer bounded `timeout_ms` and avoid unbounded service loops without cancellation.

## Safety Constraints

- No null in source-level API surface.
- All fallible operations are typed (`Result[_, ErrorEnum]`), forcing explicit failure handling.
- Avoid panic-based control flow in service loops; reserve panic for irrecoverable paths.

## Docs-To-Implementation Dry Run

Use these examples as canonical templates:

- CLI app baseline: `examples/io/cli_file_pipeline.aic`
- TCP loopback service: `examples/io/tcp_echo.aic`
- Worker orchestration: `examples/io/worker_pool.aic`

Each is included in CI examples-check/examples-run gates.
