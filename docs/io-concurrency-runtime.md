# Concurrency Runtime (IO-T5)

See also the complete IO runtime guide: `docs/io-runtime/README.md`.

This document defines `std.concurrent` behavior, runtime ABI, and operational guarantees.

## Overview

`std.concurrent` provides bounded, explicit-effect concurrency primitives:

- Task lifecycle: `spawn_task`, `join_task`, `cancel_task`
- Structured task orchestration: `spawn_group`, `timeout_task`, `select_first`
- Generic channels: `Sender[T]` / `Receiver[T]` with buffered creation, blocking/non-blocking send/recv
- Typed channel selection: `select2` and `select_any`
- Legacy compatibility: `IntChannel` and `*_int` channel APIs remain available during migration
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
struct Sender[T] { handle: Int }
struct Receiver[T] { handle: Int }
enum SelectResult[A, B] { First(A), Second(B), Timeout, Closed }
struct IntTaskSelection { task_index: Int, value: Int }
struct IntChannel { handle: Int }
struct IntChannelSelection { channel_index: Int, value: Int }
struct IntMutex { handle: Int }
```

## API

```aic
fn spawn_task(value: Int, delay_ms: Int) -> Result[Task, ConcurrencyError] effects { concurrency }
fn join_task(task: Task) -> Result[Int, ConcurrencyError] effects { concurrency }
fn timeout_task(task: Task, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency }
fn cancel_task(task: Task) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn spawn_group(values: Vec[Int], delay_ms: Int) -> Result[Vec[Int], ConcurrencyError] effects { concurrency }
fn select_first(tasks: Vec[Task], timeout_ms: Int) -> Result[IntTaskSelection, ConcurrencyError] effects { concurrency }

fn channel[T]() -> (Sender[T], Receiver[T]) effects { concurrency }
fn buffered_channel[T](capacity: Int) -> (Sender[T], Receiver[T]) effects { concurrency }
fn send[T](tx: Sender[T], value: T) -> Result[Bool, ChannelError] effects { concurrency }
fn recv[T](rx: Receiver[T]) -> Result[T, ChannelError] effects { concurrency }
fn try_send[T](tx: Sender[T], value: T) -> Result[Bool, ChannelError] effects { concurrency }
fn try_recv[T](rx: Receiver[T]) -> Result[T, ChannelError] effects { concurrency }
fn recv_timeout[T](rx: Receiver[T], timeout_ms: Int) -> Result[T, ChannelError] effects { concurrency }
fn select2[A, B](rx1: Receiver[A], rx2: Receiver[B], timeout_ms: Int) -> SelectResult[A, B] effects { concurrency }
fn select_any[T](receivers: Vec[Receiver[T]], timeout_ms: Int) -> Result[(Int, T), ChannelError] effects { concurrency, env }
fn close_sender[T](tx: Sender[T]) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn close_receiver[T](rx: Receiver[T]) -> Result[Bool, ConcurrencyError] effects { concurrency }

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

## Migration Strategy (Legacy -> Generic)

Preferred APIs are `Sender[T]` / `Receiver[T]` + generic `send/recv` operations.
Legacy `IntChannel` APIs remain available for compatibility during migration and emit deterministic `E6001` warnings with replacement hints.

Legacy-to-preferred mapping:

- `channel_int(cap)` -> `buffered_channel[Int](cap)`
- `buffered_channel_int(cap)` -> `buffered_channel[Int](cap)`
- `channel_int_buffered(cap)` -> `buffered_channel[Int](cap)`
- `send_int(ch, value, timeout)` -> `send(Sender { handle: ch.handle }, value)` (or migrate producer to typed sender)
- `recv_int(ch, timeout)` -> `recv_timeout(Receiver { handle: ch.handle }, timeout)` (or migrate consumer to typed receiver)
- `try_send_int(ch, value)` -> `try_send(Sender { handle: ch.handle }, value)`
- `try_recv_int(ch)` -> `try_recv(Receiver { handle: ch.handle })`
- `select_recv_int(ch1, ch2, timeout)` -> `select2(Receiver { handle: ch1.handle }, Receiver { handle: ch2.handle }, timeout)`

Planned legacy-to-generic transitions (sequenced with MT-T2/MT-T4):

- `IntMutex` / `lock_int` / `unlock_int` -> `Mutex[T]` / guard-based lock APIs
- `Task` + `spawn_task(value, delay_ms)` -> `Task[T]` + closure-based `spawn(fn() -> T)`
- During transition, legacy forms remain supported and should be migrated incrementally with deterministic diagnostics.

Sunset policy:

- Legacy `IntChannel` APIs are compatibility wrappers in the 0.2.x line.
- New code should use generic APIs by default.
- Agent migrations should treat `E6001` warnings as actionable and rewrite callsites incrementally.

## Runtime Semantics

- Task scheduler:
  - Runtime uses host threads with bounded handle tables.
  - `spawn_task(value, delay_ms)` produces a task that completes with `value * 2` after `delay_ms`.
- Structured fork-join:
  - `spawn_group(values, delay_ms)` spawns one task per input value, executes them in parallel, and joins in input order.
  - Success path returns ordered outputs where each element follows task semantics (`value * 2`).
  - If any child fails (`Cancelled`, `Panic`, etc.), remaining children are cooperatively cancelled and joined before returning the error.
- Deadline wrapper:
  - `timeout_task(task, timeout_ms)` waits up to the deadline for completion.
  - On deadline expiry it returns `Err(Timeout)`, and runtime performs `cancel + join` cleanup to avoid leaked/zombie worker threads.
- First-completion race:
  - `select_first(tasks, timeout_ms)` returns the first completed task as `{ task_index, value }`.
  - Remaining tasks in the input vector are cooperatively cancelled and joined before the function returns.
  - Empty task vectors or negative timeout values return `Err(InvalidInput)`.
- Buffered channel creation:
  - `channel_int`, `buffered_channel_int`, and `channel_int_buffered` create bounded buffered channels.
  - Capacity must be positive and within runtime limits.
- Generic channel payload transport:
  - `send[T]` serializes payloads to deterministic JSON text and stores them in concurrency payload slots.
  - Channel runtime transports payload IDs (`Int`) across thread boundaries.
  - `recv[T]`/`try_recv[T]`/`recv_timeout[T]` load payload text and decode back to `T`.
  - On send failure paths, staged payloads are dropped to avoid payload-slot leaks.
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
- Typed select helpers:
  - `select2(rx1, rx2, timeout_ms)` returns `SelectResult[A, B]`:
    - `First(a)`: first receiver won.
    - `Second(b)`: second receiver won.
    - `Timeout`: timeout.
    - `Closed`: channel closed/invalid payload path.
  - `select_any(receivers, timeout_ms)` supports fan-in for `N` same-typed receivers and returns `(receiver_index, value)` (`effects { concurrency, env }`).
  - `select_any` probes receivers in rotating order and uses bounded 1ms waits to avoid fixed-index starvation.
- Cancellation:
  - `cancel_task` is cooperative and returns `Ok(true)` when cancellation was requested before completion.
  - `join_task` on a cancelled task returns `Err(Cancelled)`.
  - Structured helpers (`spawn_group`, `select_first`, `timeout_task`) use runtime cancellation scopes/tokens for child-task propagation and enforce join-on-exit cleanup.
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
- `aic_rt_conc_join_timeout`
- `aic_rt_conc_cancel`
- `aic_rt_conc_spawn_group`
- `aic_rt_conc_select_first`
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
- `aic_rt_conc_payload_store`
- `aic_rt_conc_payload_take`
- `aic_rt_conc_payload_drop`
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
- `examples/io/channel_migration_compat.aic`
- `examples/io/generic_channel_types.aic`
- `examples/io/structured_concurrency.aic`
- `examples/io/select_multi_channel.aic`
- `examples/io/async_await_submit_bridge.aic`
- `examples/verify/generic_channel_protocol_ok.aic` (`aic check`)
- `examples/verify/generic_channel_protocol_invalid.aic` (`aic check`, expected `E2006`)
