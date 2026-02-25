# Concurrency Runtime (IO-T5)

See also the complete IO runtime guide: `docs/io-runtime/README.md`.

This document defines `std.concurrent` behavior, runtime ABI, and operational guarantees.

## Overview

`std.concurrent` provides bounded, explicit-effect concurrency primitives:

- Task lifecycle: `spawn_task`, `join_task`, `cancel_task`
- Typed channels: `IntChannel` with buffered creation, blocking/non-blocking send/recv, and select
- Synchronization utility: `IntMutex` with `lock_int`, `unlock_int`, `close_mutex`

All APIs are `effects { concurrency }`.

Related runtime note:
- `std.net` async submit/wait operations (`async_*`) also require `effects { concurrency }` and are backed by the async reactor loop documented in `docs/async-event-loop.md`.
- `await` submit-bridge lowering (`await Result[Async*Op, NetError]`) polls those reactor operations cooperatively via runtime async poll helpers.

## Types

```aic
enum ConcurrencyError {
    NotFound,
    Timeout,
    Cancelled,
    InvalidInput,
    Panic,
    Closed,
    Io,
}

enum ChannelError {
    Closed,
    Full,
    Empty,
    Timeout,
}

struct Task { handle: Int }
struct IntChannel { handle: Int }
struct IntChannelSelection { channel_index: Int, value: Int }
struct IntMutex { handle: Int }
```

## API

```aic
fn spawn_task(value: Int, delay_ms: Int) -> Result[Task, ConcurrencyError] effects { concurrency }
fn join_task(task: Task) -> Result[Int, ConcurrencyError] effects { concurrency }
fn cancel_task(task: Task) -> Result[Bool, ConcurrencyError] effects { concurrency }

fn channel_int(capacity: Int) -> Result[IntChannel, ConcurrencyError] effects { concurrency }
fn buffered_channel_int(capacity: Int) -> Result[IntChannel, ConcurrencyError] effects { concurrency }
fn channel_int_buffered(capacity: Int) -> Result[IntChannel, ConcurrencyError] effects { concurrency }
fn send_int(ch: IntChannel, value: Int, timeout_ms: Int) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn recv_int(ch: IntChannel, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency }
fn try_send_int(ch: IntChannel, value: Int) -> Result[Bool, ChannelError] effects { concurrency }
fn try_recv_int(ch: IntChannel) -> Result[Int, ChannelError] effects { concurrency }
fn select_recv_int(ch1: IntChannel, ch2: IntChannel, timeout_ms: Int) -> Result[IntChannelSelection, ChannelError] effects { concurrency }
fn close_channel(ch: IntChannel) -> Result[Bool, ConcurrencyError] effects { concurrency }

fn mutex_int(initial: Int) -> Result[IntMutex, ConcurrencyError] effects { concurrency }
fn lock_int(mutex: IntMutex, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency }
fn unlock_int(mutex: IntMutex, value: Int) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn close_mutex(mutex: IntMutex) -> Result[Bool, ConcurrencyError] effects { concurrency }
```

## Runtime Semantics

- Task scheduler:
  - Runtime uses host threads with bounded handle tables.
  - `spawn_task(value, delay_ms)` produces a task that completes with `value * 2` after `delay_ms`.
- Buffered channel creation:
  - `channel_int`, `buffered_channel_int`, and `channel_int_buffered` create bounded buffered channels.
  - Capacity must be positive and within runtime limits.
- Backpressure when full:
  - `send_int` blocks while channel is full, up to `timeout_ms`.
  - If no space is available before deadline, `send_int` returns `Err(Timeout)`.
- Non-blocking channel operations:
  - `try_send_int` returns `Err(Full)` when buffer is full and `Err(Closed)` when closed.
  - `try_recv_int` returns `Err(Empty)` when buffer is empty but open, and `Err(Closed)` when closed and drained.
- Select over int channels:
  - `select_recv_int(ch1, ch2, timeout_ms)` waits for the first available receive across two channels.
  - Return payload includes selected channel index (`0` or `1`) and received value.
  - Selection alternates polling order between channels to reduce starvation.
- Cancellation:
  - `cancel_task` is cooperative and returns `Ok(true)` when cancellation was requested before completion.
  - `join_task` on a cancelled task returns `Err(Cancelled)`.
- Panic propagation:
  - Negative task input (`value < 0`) is treated as runtime task panic.
  - `join_task` returns `Err(Panic)` for that task.
- Channel shutdown:
  - `close_channel` transitions channel to closed state and wakes blocked producers/consumers.
  - Blocking receives on empty closed channel return `Err(Closed)`.
- Locking and liveness:
  - `lock_int` uses bounded wait via `timeout_ms` and returns `Err(Timeout)` if lock cannot be acquired.

## Error-Code Mapping

Runtime codegen maps host/runtime status codes to `ConcurrencyError`:

- `1 -> NotFound`
- `2 -> Timeout`
- `3 -> Cancelled`
- `4 -> InvalidInput`
- `5 -> Panic`
- `6 -> Closed`
- `7 -> Io`

Runtime codegen maps channel status codes to `ChannelError`:

- `2 -> Timeout`
- `6 -> Closed`
- `8 -> Full`
- `9 -> Empty`

## Runtime ABI

Codegen lowers to these runtime symbols:

- `aic_rt_conc_spawn`
- `aic_rt_conc_join`
- `aic_rt_conc_cancel`
- `aic_rt_conc_channel_int`
- `aic_rt_conc_channel_int_buffered`
- `aic_rt_conc_send_int`
- `aic_rt_conc_try_send_int`
- `aic_rt_conc_recv_int`
- `aic_rt_conc_try_recv_int`
- `aic_rt_conc_select_recv_int`
- `aic_rt_conc_close_channel`
- `aic_rt_conc_mutex_int`
- `aic_rt_conc_mutex_lock`
- `aic_rt_conc_mutex_unlock`
- `aic_rt_conc_mutex_close`
- `aic_rt_async_poll_int`
- `aic_rt_async_poll_string`

## Platform Limitations

- Linux/macOS:
  - Full channel/task/mutex runtime behavior is implemented.
- Windows:
  - Existing concurrency APIs return `ConcurrencyError::Io` for unsupported runtime paths.
  - Channel try/select APIs return deterministic `ChannelError::Closed`.

## Example

- `examples/io/worker_pool.aic`
- `examples/io/async_await_submit_bridge.aic`
