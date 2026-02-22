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

## CI Coverage

- Execution tests:
  - multi-connection async flow
  - queue saturation and shutdown behavior
- Runnable example:
  - `examples/io/async_net_event_loop.aic`
