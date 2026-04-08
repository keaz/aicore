# Async Event Loop Runtime (REST-T8)

This document defines the runtime model used by async networking APIs in `std.net`.

## Scope

- Single-process, single event-loop worker thread.
- Bounded operation queue with deterministic backpressure.
- Reactor-based non-blocking socket progress for async TCP accept/send/recv.
- Async submit/wait API surface for TCP accept/send/recv.
- TLS async submit/wait API surface for TLS send/recv.

## Async + REST Runtime Support Matrix

| Capability | Status | Evidence anchor |
|---|---|---|
| `std.net` async submit/wait/cancel/poll/wait-many/shutdown | Supported | Runtime async reactor in `src/codegen/runtime/part04.c` + execution tests `exec_net_async_event_loop_multi_connection`, `exec_net_async_wait_many_paths_are_stable`, `exec_net_async_queue_backpressure_and_shutdown` |
| `await` submit bridge for net/tls async handles | Supported | Runtime poll helpers (`aic_rt_async_poll_int`, `aic_rt_async_poll_string`) + execution test `exec_async_await_submit_bridge_drives_reactor_without_task_spawn` |
| `std.tls` async submit/wait/cancel/poll/wait-many/shutdown | Partial | API/runtime paths are implemented; `tls_async_runtime_pressure` currently reports `queue_depth = 0` and `queue_limit = 0`, and TLS backend availability gates some execution paths |
| Async HTTP-server API surface | Supported | `std.http_server` async accept + compatibility read/write/serve wrappers in `std/http_server.aic` + runnable example `examples/io/http_server_async_api.aic` |
| Linux/macOS runtime backend | Supported | Reactor-backed async paths are execution-tested on non-Windows targets |
| Windows async runtime backend | Supported (client-runtime scope) | Shared reactor backend in `src/codegen/runtime/part04.c` + Windows CI smoke coverage for `exec_net_async_wait_negative_paths_are_stable`, `exec_net_tcp_loopback_echo`, and Windows-target build smoke in `tests/e7_build_hermetic_tests.rs` |

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

## Core Async Lowering Model

- `async fn` still has a distinct surface type: call sites see `Async[T]` and must consume it with `await`.
- In current codegen, ordinary async returns are lowered to compiler-managed ready `Async[T]` wrapper values with a readiness bit and payload.
- The non-blocking reactor integration point today is the submit bridge:
  - `await Result[Async*Op, NetError|TlsError]`
  - runtime polling is delegated to `aic_rt_async_poll_int` / `aic_rt_async_poll_string`
- This means the repo supports production async net/tls wait paths through the reactor, while agent-facing docs should not describe the current implementation as a general stackless-coroutine future runtime.

## Wrapper Semantics

- `async_accept`, `async_tcp_send`, and `async_tcp_recv` are thin wrappers over submit + wait:
  - `async_accept(listener, timeout_ms)` = `async_accept_submit(listener, timeout_ms)` then `async_wait_int(op, timeout_ms)`
  - `async_tcp_send(handle, payload, timeout_ms)` = `async_tcp_send_submit(handle, payload)` then `async_wait_int(op, timeout_ms)`
  - `async_tcp_recv(handle, max_bytes, timeout_ms)` = `async_tcp_recv_submit(handle, max_bytes, timeout_ms)` then `async_wait_string(op, timeout_ms)`
- `std.http_server.async_accept_*` delegates directly to `std.net` async accept handles and preserves `NetError`.
- `std.http_server.async_read_request` and `std.http_server.async_write_response` are compatibility wrappers over the existing synchronous HTTP server request/response APIs.
- `std.http_server.async_serve(...)` composes accept + async read + handler dispatch + async write, then closes the accepted connection before returning.
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
- Polling uses reactor-backed helpers:
  - `aic_rt_async_poll_int`
  - `aic_rt_async_poll_string`
- Ordinary `await` on `Async[T]` extracts the wrapped payload from the compiler-managed async value.
- Poll helpers use short wait slices and cooperative yield (`sleep_ms(1)`) between retry windows.
- Terminal timeout completion remains `Err(Timeout)` (not remapped to `NotFound`).

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
- Submit paths enqueue opaque operation handles and operation metadata.
- A dedicated worker thread activates operations and advances them through a reactor.
- Reactor backends:
  - Linux: `epoll`
  - macOS/BSD: `kqueue`
  - Fallback: `poll`
- Active sockets are temporarily switched to non-blocking mode while an async op is in flight and restored when the op completes.
- Completion data is published through per-operation condition variables.
- Cancelled operations resolve as typed cancellation errors (`NetError::Cancelled` / `TlsError::Cancelled`).
- `async_shutdown` enters drain mode:
  - new submissions are rejected with deterministic `NetError`,
  - queued + active operations are completed/drained,
  - worker is joined before returning.

## Backpressure and Determinism

- Queue-full submission returns `NetError::Timeout`.
- Wait calls are single-consumer per operation handle.
- Timeout while waiting does not destroy the in-flight operation; later wait can retry.
- `async_runtime_pressure` snapshots expose `active_ops`, `queue_depth`, `op_limit`, and `queue_limit`.
- `tls_async_runtime_pressure` snapshots expose `active_ops` and `op_limit`; current TLS runtime reports `queue_depth = 0` and `queue_limit = 0`.
- All failures map through existing `NetError` code mapping.

## CI and Perf Gate Mapping

- Regression tests in `tests/execution_tests.rs` cover:
  - multi-connection async flow (`exec_net_async_event_loop_multi_connection`)
  - queue saturation + shutdown (`exec_net_async_queue_backpressure_and_shutdown`)
  - 1000 concurrent accepts on a single thread (`exec_net_async_accept_1000_connections_single_thread`)
  - async submit+await bridge polling (`exec_async_await_submit_bridge_drives_reactor_without_task_spawn`)
  - negative async-wait paths (`exec_net_async_wait_negative_paths_are_stable`) for invalid handles, timeout retry semantics, and single-consumer re-wait behavior
- CI example coverage in `scripts/ci/examples.sh` includes `examples/io/async_net_event_loop.aic` in both:
  - `check_pass` (compile/check gate)
  - `run_pass` (runtime gate)
- CI also includes `examples/io/async_await_submit_bridge.aic` in both check and run gates.
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
