# Concurrency Runtime (IO-T5)

See also the complete IO runtime guide: `docs/io-runtime/README.md`.

This document defines `std.concurrent` behavior, runtime ABI, and operational guarantees.

## Overview

`std.concurrent` provides bounded, explicit-effect concurrency primitives:

- Task lifecycle: `spawn_task`, `join_task`, `cancel_task`
- Typed channels: `IntChannel` with `send_int`, `recv_int`, `close_channel`
- Synchronization utility: `IntMutex` with `lock_int`, `unlock_int`, `close_mutex`

All APIs are `effects { concurrency }`.

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

struct Task { handle: Int }
struct IntChannel { handle: Int }
struct IntMutex { handle: Int }
```

## API

```aic
fn spawn_task(value: Int, delay_ms: Int) -> Result[Task, ConcurrencyError] effects { concurrency }
fn join_task(task: Task) -> Result[Int, ConcurrencyError] effects { concurrency }
fn cancel_task(task: Task) -> Result[Bool, ConcurrencyError] effects { concurrency }

fn channel_int(capacity: Int) -> Result[IntChannel, ConcurrencyError] effects { concurrency }
fn send_int(ch: IntChannel, value: Int, timeout_ms: Int) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn recv_int(ch: IntChannel, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency }
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

## Runtime ABI

Codegen lowers to these runtime symbols:

- `aic_rt_conc_spawn`
- `aic_rt_conc_join`
- `aic_rt_conc_cancel`
- `aic_rt_conc_channel_int`
- `aic_rt_conc_send_int`
- `aic_rt_conc_recv_int`
- `aic_rt_conc_close_channel`
- `aic_rt_conc_mutex_int`
- `aic_rt_conc_mutex_lock`
- `aic_rt_conc_mutex_unlock`
- `aic_rt_conc_mutex_close`

## Example

- `examples/io/worker_pool.aic`
