# Async Event Loop Runtime (REST-T8)

This document defines the runtime model used by async networking APIs in `std.net`.

## Scope

- Single-process, single event-loop worker thread.
- Bounded operation queue with deterministic backpressure.
- Async submit/wait API surface for TCP accept/send/recv.

## API Surface

`std/net.aic` now exposes:

- `async_accept_submit(listener, timeout_ms) -> Result[AsyncIntOp, NetError]`
- `async_tcp_send_submit(handle, payload) -> Result[AsyncIntOp, NetError]`
- `async_tcp_recv_submit(handle, max_bytes, timeout_ms) -> Result[AsyncStringOp, NetError]`
- `async_wait_int(op, timeout_ms) -> Result[Int, NetError]`
- `async_wait_string(op, timeout_ms) -> Result[String, NetError]`
- `async_shutdown() -> Result[Bool, NetError]`
- Convenience wrappers: `async_accept`, `async_tcp_send`, `async_tcp_recv`

## Wrapper Semantics

- `async_accept`, `async_tcp_send`, and `async_tcp_recv` are thin wrappers over submit + wait:
  - `async_accept(listener, timeout_ms)` = `async_accept_submit(listener, timeout_ms)` then `async_wait_int(op, timeout_ms)`
  - `async_tcp_send(handle, payload, timeout_ms)` = `async_tcp_send_submit(handle, payload)` then `async_wait_int(op, timeout_ms)`
  - `async_tcp_recv(handle, max_bytes, timeout_ms)` = `async_tcp_recv_submit(handle, max_bytes, timeout_ms)` then `async_wait_string(op, timeout_ms)`
- Wrapper methods preserve submit failures exactly: submit `Err` is returned directly, with no remapping.
- Wait handles are single-consumer. Re-waiting the same completed handle returns `NetError::NotFound`.
- Timeout while waiting keeps the operation pending and releases the claim so a later wait can retry.

## Runtime Architecture

- Queue capacity is fixed (`AIC_RT_NET_ASYNC_QUEUE_CAP`) and enforced on submit.
- Submit paths enqueue opaque operation handles.
- A dedicated worker thread drains the queue and executes socket operations.
- Completion data is published through per-operation condition variables.
- `async_shutdown` requests graceful drain and joins the worker thread.

## Backpressure and Determinism

- Queue-full submission returns `NetError::Timeout`.
- All failures map through existing `NetError` code mapping.
- Wait calls are single-consumer per operation handle.

## CI and Perf Gate Mapping

- Regression tests in `/Users/kasunranasinghe/Projects/Rust/aicore/tests/execution_tests.rs` cover:
  - multi-connection async flow (`exec_net_async_event_loop_multi_connection`)
  - queue saturation + shutdown (`exec_net_async_queue_backpressure_and_shutdown`)
  - negative async-wait paths (`exec_net_async_wait_negative_paths_are_stable`) for invalid handles, timeout retry semantics, and single-consumer re-wait behavior
- CI example coverage in `/Users/kasunranasinghe/Projects/Rust/aicore/scripts/ci/examples.sh` includes `/Users/kasunranasinghe/Projects/Rust/aicore/examples/io/async_net_event_loop.aic` in both:
  - `check_pass` (compile/check gate)
  - `run_pass` (runtime gate)
- Perf gate baseline is `/Users/kasunranasinghe/Projects/Rust/aicore/benchmarks/service_baseline/async-net-gate.v1.json`:
  - scenario: `rest_async_echo_100_connections`
  - baseline timings: `thread_per_connection_ms = 420.0`, `event_loop_ms = 180.0`
  - gate: `max_ratio = 0.8` (`event_loop_ms / thread_per_connection_ms` must stay <= `0.8`)
