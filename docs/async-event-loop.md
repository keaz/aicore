# Async Event Loop Runtime (REST-T8)

This document defines the runtime model used by async submit/wait APIs in `std.net`, `std.tls`, and `std.fs`.

## Scope

- Single-process runtime with a configurable async worker pool (`1..=32` worker threads, default `1`).
- Bounded operation queue with deterministic backpressure.
- Reactor-based non-blocking socket progress for async TCP accept/send/recv.
- Async submit/wait API surface for TCP accept/send/recv.
- TLS async submit/wait API surface for TLS send/recv.
- Task-backed async filesystem submit/wait API surface for text/bytes file operations.

## Async + REST Runtime Support Matrix

| Capability | Status | Evidence anchor |
|---|---|---|
| `std.net` async submit/wait/cancel/poll/wait-many/shutdown | Supported | Runtime async reactor in `src/codegen/runtime/part04.c` + execution tests `exec_net_async_event_loop_multi_connection`, `exec_net_async_wait_many_paths_are_stable`, `exec_net_async_queue_backpressure_and_shutdown` |
| `std.fs` async submit/wait/cancel/poll/wait-many/shutdown | Supported | Task-backed runtime bridge in `std/fs.aic` + `src/codegen/runtime/part03.c` + execution tests `exec_fs_async_submit_wait_roundtrip`, `exec_fs_async_runtime_backpressure_is_deterministic`, `exec_fs_async_wait_timeout_retry_is_stable` |
| `await` submit bridge for net/tls async handles | Supported | Runtime poll helpers (`aic_rt_async_poll_int`, `aic_rt_async_poll_string`) + execution test `exec_async_await_submit_bridge_drives_reactor_without_task_spawn` |
| `await` submit bridge for fs async handles | Supported | Task-join helper (`aic_rt_conc_join_value`) + execution test `exec_async_await_fs_submit_bridge_roundtrip` |
| `std.tls` async submit/wait/cancel/poll/wait-many/shutdown | Supported | OpenSSL-backed builds report slot-backed TLS async pressure, and execution tests cover timeout/cancel/poll/wait-many/shutdown/backpressure paths against a local TLS harness; builds without TLS backend support return typed `TlsError::ProtocolError` |
| Async HTTP-server API surface | Supported | request/response I/O now uses dedicated async runtime intrinsics in `src/codegen/runtime/part05.c`, and `async_serve` composes through native async accept/read/write helpers |
| Linux/macOS runtime backend | Supported | Reactor-backed async paths are execution-tested on non-Windows targets |
| Windows async runtime backend | Supported (client-runtime scope) | Shared reactor backend in `src/codegen/runtime/part04.c` + Windows CI smoke coverage for TCP loopback plus async accept/recv wait/cancel/shutdown lifecycle (`exec_net_async_wait_negative_paths_are_stable`, `exec_net_tcp_loopback_echo`) and Windows-target build smoke in `tests/e7_build_hermetic_tests.rs` |

Windows coverage in this document is intentionally low-level: the supported substrate is the transport/runtime contract that service libraries build on, while native async REST-server validation remains a separate, narrower coverage problem.

## API Surface

`std/net.aic` now exposes:

- `async_accept_submit(listener, timeout_ms) -> Result[AsyncIntOp, NetError]`
- `async_tcp_send_submit(handle, payload) -> Result[AsyncIntOp, NetError]`
- `async_tcp_recv_submit(handle, max_bytes, timeout_ms) -> Result[AsyncStringOp, NetError]`
- `async_wait_int(op, timeout_ms) -> Result[Int, NetError]`
- `async_wait_string(op, timeout_ms) -> Result[Bytes, NetError]`
- `async_cancel_int(op) -> Result[Bool, NetError]`
- `async_cancel_string(op) -> Result[Bool, NetError]`
- `async_poll_int(op) -> Result[Option[Int], NetError]`
- `async_poll_string(op) -> Result[Option[Bytes], NetError]`
- `async_wait_many_int(ops, timeout_ms) -> Result[AsyncIntSelection, NetError]`
- `async_wait_many_string(ops, timeout_ms) -> Result[AsyncStringSelection, NetError]`
- `async_wait_any_int(op1, op2, timeout_ms) -> Result[AsyncIntSelection, NetError]`
- `async_wait_any_string(op1, op2, timeout_ms) -> Result[AsyncStringSelection, NetError]`
- `async_runtime_pressure() -> Result[AsyncRuntimePressure, NetError]`
- `async_shutdown() -> Result[Bool, NetError]`
- Convenience wrappers: `async_accept`, `async_tcp_send`, `async_tcp_recv`

`std/tls.aic` now exposes:

- `tls_async_send_submit(stream, data, timeout_ms) -> Result[AsyncIntOp, TlsError]`
- `tls_async_recv_submit(stream, max_bytes, timeout_ms) -> Result[AsyncStringOp, TlsError]`
- `tls_async_wait_int(op, timeout_ms) -> Result[Int, TlsError]`
- `tls_async_wait_string(op, timeout_ms) -> Result[Bytes, TlsError]`
- `tls_async_cancel_int(op) -> Result[Bool, TlsError]`
- `tls_async_cancel_string(op) -> Result[Bool, TlsError]`
- `tls_async_poll_int(op) -> Result[Option[Int], TlsError]`
- `tls_async_poll_string(op) -> Result[Option[Bytes], TlsError]`
- `tls_async_wait_many_int(ops, timeout_ms) -> Result[TlsAsyncIntSelection, TlsError]`
- `tls_async_wait_many_string(ops, timeout_ms) -> Result[TlsAsyncStringSelection, TlsError]`
- `tls_async_wait_any_int(op1, op2, timeout_ms) -> Result[TlsAsyncIntSelection, TlsError]`
- `tls_async_wait_any_string(op1, op2, timeout_ms) -> Result[TlsAsyncStringSelection, TlsError]`
- `tls_async_runtime_pressure() -> Result[AsyncRuntimePressure, TlsError]`
- `tls_async_shutdown() -> Result[Bool, TlsError]`
- Convenience wrappers: `tls_async_send`, `tls_async_recv`

`std/fs.aic` now exposes:

- `async_read_text_submit(path) -> Result[AsyncFsTextOp, FsError]`
- `async_read_bytes_submit(path) -> Result[AsyncFsBytesOp, FsError]`
- `async_write_text_submit(path, content) -> Result[AsyncFsBoolOp, FsError]`
- `async_write_bytes_submit(path, content) -> Result[AsyncFsBoolOp, FsError]`
- `async_append_text_submit(path, content) -> Result[AsyncFsBoolOp, FsError]`
- `async_append_bytes_submit(path, content) -> Result[AsyncFsBoolOp, FsError]`
- `async_wait_text(op, timeout_ms) -> Result[String, FsError]`
- `async_wait_bytes(op, timeout_ms) -> Result[Bytes, FsError]`
- `async_wait_bool(op, timeout_ms) -> Result[Bool, FsError]`
- `async_poll_text(op) -> Result[Option[String], FsError]`
- `async_poll_bytes(op) -> Result[Option[Bytes], FsError]`
- `async_poll_bool(op) -> Result[Option[Bool], FsError]`
- `async_cancel_text(op) -> Result[Bool, FsError]`
- `async_cancel_bytes(op) -> Result[Bool, FsError]`
- `async_cancel_bool(op) -> Result[Bool, FsError]`
- `async_wait_many_text(ops, timeout_ms) -> Result[FsAsyncTextSelection, FsError]`
- `async_wait_many_bytes(ops, timeout_ms) -> Result[FsAsyncBytesSelection, FsError]`
- `async_wait_many_bool(ops, timeout_ms) -> Result[FsAsyncBoolSelection, FsError]`
- `async_wait_any_text(op1, op2, timeout_ms) -> Result[FsAsyncTextSelection, FsError]`
- `async_wait_any_bytes(op1, op2, timeout_ms) -> Result[FsAsyncBytesSelection, FsError]`
- `async_wait_any_bool(op1, op2, timeout_ms) -> Result[FsAsyncBoolSelection, FsError]`
- `async_runtime_pressure() -> Result[FsAsyncRuntimePressure, FsError]`
- `async_shutdown() -> Result[Bool, FsError]`
- Convenience wrappers: `async_read_text`, `async_read_bytes`, `async_write_text`, `async_write_bytes`, `async_append_text`, `async_append_bytes`

`std/http_server.aic` now exposes:

- `async_accept_submit(listener, timeout_ms) -> Result[AsyncIntOp, NetError]`
- `async_accept_wait(op, timeout_ms) -> Result[Int, NetError]`
- `async_accept(listener, timeout_ms) -> Result[Int, NetError]`
- `async_read_request(conn, max_bytes, timeout_ms) -> Result[Request, ServerError]`
- `async_write_response(conn, response) -> Result[Int, ServerError]`
- `async_serve(listener, max_bytes, accept_timeout_ms, io_timeout_ms, handler) -> Result[Int, ServerError]`

Language-level bridge:

- `await` now also accepts submit results directly:
  - `await Result[AsyncIntOp, NetError] -> Result[Int, NetError]`
  - `await Result[AsyncStringOp, NetError] -> Result[Bytes, NetError]`
  - `await Result[AsyncIntOp, TlsError] -> Result[Int, TlsError]`
  - `await Result[AsyncStringOp, TlsError] -> Result[Bytes, TlsError]`
  - `await Result[AsyncFsBoolOp, FsError] -> Result[Bool, FsError]`
  - `await Result[AsyncFsTextOp, FsError] -> Result[String, FsError]`
  - `await Result[AsyncFsBytesOp, FsError] -> Result[Bytes, FsError]`

## Core Async Lowering Model

- `async fn` still has a distinct surface type: call sites see `Async[T]` and must consume it with `await`.
- Current codegen lowers ordinary async functions to compiler-managed future frames with:
  - an explicit `state` slot
  - persisted parameter/local storage for values that must survive suspension
  - generated poll/drop helpers attached to the `Async[T]` handle
- `await` inside `async fn` lowers to resumable suspension points:
  - ordinary `await Async[T]` polls the child future once with `aic_rt_async_poll_once`
  - pending child work returns `1` from the generated poll helper and resumes from the stored state on the next poll
  - completed child futures are dropped after their result is extracted
- The non-blocking reactor/task integration point for I/O-backed work is the submit bridge:
  - `await Result[Async*Op, NetError|TlsError]`
  - `await Result[AsyncFs*Op, FsError]`
- submit-bridge polling is delegated to one-shot runtime helpers inside the async state machine:
  - `aic_rt_net_async_poll_int_once` / `aic_rt_net_async_poll_string_once`
  - `aic_rt_tls_async_poll_int_once` / `aic_rt_tls_async_poll_string_once`
  - `aic_rt_conc_join_poll` for fs task-backed handles
- Outside an async state machine, `await Async[T]` still uses the runtime drive loop (`aic_rt_async_drive`) to poll a future to completion.

## Wrapper Semantics

- `async_accept`, `async_tcp_send`, and `async_tcp_recv` are thin wrappers over submit + wait:
  - `async_accept(listener, timeout_ms)` = `async_accept_submit(listener, timeout_ms)` then `async_wait_int(op, timeout_ms)`
  - `async_tcp_send(handle, payload, timeout_ms)` = `async_tcp_send_submit(handle, payload)` then `async_wait_int(op, timeout_ms)`
  - `async_tcp_recv(handle, max_bytes, timeout_ms)` = `async_tcp_recv_submit(handle, max_bytes, timeout_ms)` then `async_wait_string(op, timeout_ms)`
- `std.http_server.async_accept_*` delegates directly to `std.net` async accept handles and preserves `NetError`.
- `std.http_server.async_read_request` and `std.http_server.async_write_response` use dedicated async runtime intrinsics over the async net layer rather than falling back to the synchronous HTTP server helpers.
- `std.http_server.async_serve(...)` composes async accept + native async read + handler dispatch + native async write, then closes the accepted connection before returning.
- Wrapper methods preserve submit failures exactly: submit `Err` is returned directly, with no remapping.
- Wait handles are single-consumer. Re-waiting the same completed handle returns `NetError::NotFound`.
- Timeout while waiting keeps the operation pending and releases the claim so a later wait can retry.
- TLS async wait follows the same retry model; timeout maps to `TlsError::Timeout`.
- `async_cancel_*` / `tls_async_cancel_*` are idempotent and report whether cancellation was applied via `Bool`.
- `async_poll_*` / `tls_async_poll_*` perform zero-timeout probes and return `Option` without blocking.
- `async_wait_many_*` / `tls_async_wait_many_*` scan operations in deterministic index order and return the winning index plus payload/value.
- `async_wait_any_*` / `tls_async_wait_any_*` remain compatibility wrappers over `wait_many_*` for two-op selection.

## Await Submit Bridge Semantics

- `await async_accept_submit(listener, timeout_ms)` lowers to runtime polling over the submit handle.
- Polling inside async state machines uses reactor-backed one-shot helpers:
  - `aic_rt_net_async_poll_int_once`
  - `aic_rt_net_async_poll_string_once`
  - `aic_rt_tls_async_poll_int_once`
  - `aic_rt_tls_async_poll_string_once`
  - `aic_rt_conc_join_poll`
- Ordinary `await` on `Async[T]` inside async state machines polls once and suspends; outside async state machines it is driven to completion by `aic_rt_async_drive`.
- Drive-loop polling still uses short wait slices and cooperative yield (`sleep_ms(1)`) between retry windows.
- Terminal timeout completion remains `Err(Timeout)` (not remapped to `NotFound`).
- Fs async wait timeout keeps the underlying task pending and releases the claim so a later wait can retry.
- Sync `std.fs` APIs still report the stable filesystem subset (`NotFound`, `PermissionDenied`, `AlreadyExists`, `InvalidInput`, `Io`); `Timeout` and `Cancelled` are reserved for async filesystem/runtime-control paths.

Example:

```aic
let accepted = await async_accept_submit(listener, 2000);
let socket = match accepted {
    Ok(h) => h,
    Err(err) => 0,
};
```

## Runtime Architecture

- Queue capacity is configurable at process start via `AIC_RT_LIMIT_NET_ASYNC_QUEUE` (bounded by compile-time hard maximum) and enforced on submit.
- Worker-pool size is configurable at process start via `AIC_RT_LIMIT_NET_ASYNC_WORKERS` (default `1`, max `32`, clamped to the async op limit).
- Submit paths enqueue opaque operation handles and operation metadata into a shared bounded queue.
- A worker pool activates operations from the shared queue and advances them through per-thread reactors.
- Accepted TCP sockets are registered through the synchronized net handle table, so parallel async accept workers cannot publish duplicate or lost stream handles.
- Reactor backends:
  - Linux: `epoll`
  - macOS/BSD: `kqueue`
  - Fallback: `poll`
- Active sockets are temporarily switched to non-blocking mode while an async op is in flight and restored when the op completes.
- Completion data is published through per-operation condition variables.
- Cancelled operations resolve as typed cancellation errors (`NetError::Cancelled` / `TlsError::Cancelled`).
- Cancelling a net operation while it is still queued removes it from the bounded queue immediately, so follow-up pressure snapshots and submissions do not observe stale queued work.
- `async_shutdown` enters drain mode:
  - new submissions are rejected with deterministic `NetError`,
  - queued + active operations are completed/drained,
  - worker is joined before returning.

## Backpressure and Determinism

- Queue-full submission returns `NetError::Timeout`.
- Wait calls are single-consumer per operation handle.
- Timeout while waiting does not destroy the in-flight operation; later wait can retry.
- Fs async submission is bounded by `AIC_RT_LIMIT_FS_ASYNC_OPS`; saturation returns `FsError::Timeout`.
- `async_runtime_pressure` snapshots expose `active_ops`, `queue_depth`, `op_limit`, and `queue_limit`.
- Net async pressure remains process-wide over the worker pool:
  - `active_ops` aggregates in-flight operations across all workers.
  - `queue_depth` / `queue_limit` describe the shared bounded submission queue rather than a per-worker queue.
- `std.fs.async_runtime_pressure` snapshots expose active fs task count and configured fs async op limit; current task-backed backend reports `queue_depth = 0` and `queue_limit = 0`.
- `tls_async_runtime_pressure` snapshots expose active in-flight ops plus occupied-slot pressure.
- TLS uses slot-backed worker capacity rather than the net reactor queue, so `queue_depth` mirrors occupied TLS async slots and `queue_limit` mirrors the configured TLS async slot limit.
- All failures map through existing `NetError` code mapping.

## CI and Perf Gate Mapping

- Regression tests in `tests/execution_tests.rs` cover:
  - multi-connection async flow (`exec_net_async_event_loop_multi_connection`)
  - queue saturation + shutdown (`exec_net_async_queue_backpressure_and_shutdown`)
  - multi-worker concurrent service load (`exec_runtime_net_async_multi_worker_service_load_is_stable`)
  - 1000 concurrent accepts on a single thread (`exec_net_async_accept_1000_connections_single_thread`)
  - async submit+await bridge polling (`exec_async_await_submit_bridge_drives_reactor_without_task_spawn`)
  - negative async-wait paths (`exec_net_async_wait_negative_paths_are_stable`) for invalid handles, timeout retry semantics, and single-consumer re-wait behavior
  - TLS async lifecycle controls (`exec_tls_async_wait_selection_and_poll_paths_are_stable`, `exec_tls_async_cancel_reports_typed_cancelled_error`, `exec_tls_async_pressure_shutdown_and_backpressure_are_deterministic`, `exec_runtime_tls_async_lifecycle_sustained_churn_is_leak_free`)
- CI example coverage in `scripts/ci/examples.sh` includes `examples/io/async_net_event_loop.aic` in both:
  - `check_pass` (compile/check gate)
  - `run_pass` (runtime gate)
- CI also includes `examples/io/async_await_submit_bridge.aic` in both check and run gates.
- CI also includes `examples/io/async_net_worker_pool.aic` in both check and run gates for concurrent multi-connection worker-pool coverage.
- CI also includes `examples/io/fs_async_await_bridge.aic`, `examples/io/fs_async_runtime_controls.aic`, and `examples/io/fs_async_tasks.aic` in both check and run gates for filesystem async coverage.
- CI also includes `examples/io/tls_async_submit_wait.aic` in both check and run gates for TLS async submit/wait contract coverage.
- CI also includes `examples/io/async_lifecycle_controls.aic` in both check and run gates for lifecycle controls coverage.
- CI also includes `examples/io/http_server_async_api.aic` in both check and run gates for async HTTP server coverage.
- Runnable wait-many orchestration example: `examples/io/async_wait_many_orchestration.aic`.
- Runnable pressure-gating example: `examples/io/async_runtime_pressure_gating.aic`.
- Perf gate baseline is `benchmarks/service_baseline/async-net-gate.v1.json`:
  - scenario: `rest_async_echo_1000_connections`
  - encoded load: `connections = 1000`
  - baseline timings: `thread_per_connection_ms = 420.0`, `event_loop_ms = 180.0`
  - gate: `max_ratio = 0.8` (`event_loop_ms / thread_per_connection_ms` must stay <= `0.8`)
