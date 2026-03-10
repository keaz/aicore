# Concurrency Runtime (IO-T5)

See also the complete IO runtime guide: `docs/io-runtime/README.md`.

This document defines `std.concurrent` behavior, runtime ABI, and operational guarantees.

## Overview

`std.concurrent` provides bounded, explicit-effect concurrency primitives:

- Generic task lifecycle: `spawn`, `join`, `join_value`, `spawn_named`
- Scoped concurrency: `scoped`, `scope_spawn`, `scope_join_all`, `scope_cancel`
- Legacy task compatibility: `spawn_task`, `join_task`, `timeout_task`, `cancel_task`
- Structured task orchestration: `spawn_group`, `timeout_task`, `select_first`
- Generic channels: `Sender[T]` / `Receiver[T]` with buffered creation, blocking/non-blocking send/recv
- Typed channel selection: `select2` and `select_any`
- Legacy compatibility: `IntChannel` and `*_int` channel APIs remain available during migration
- Generic synchronization: `Mutex[T]`, `MutexGuard[T]`, `RwLock[T]`
- Shared ownership: `Arc[T]` with atomic reference counting
- Lock-free primitives: `AtomicInt`, `AtomicBool` with sequentially-consistent operations
- Per-thread state: `ThreadLocal[T]` with lazy initialization per thread
- Legacy synchronization compatibility: `IntMutex` with `lock_int`, `unlock_int`, `close_mutex`
- Compile-time thread-safety checks: marker traits `Send[T]` / `Sync[T]` with `Send` enforcement on cross-thread payload APIs

All APIs are `effects { concurrency }`.

Related runtime note:
- `std.net` async submit/wait operations (`async_*`) also require `effects { concurrency }` and are backed by the async reactor loop documented in `docs/async-event-loop.md`.
- `await` submit-bridge lowering (`await Result[Async*Op, NetError|TlsError]`) polls those reactor operations cooperatively via runtime async poll helpers.

## Runtime Support Matrix

| Capability | Status | Evidence anchor |
|---|---|---|
| Generic `std.concurrent` task/channel/sync APIs | Supported | Runtime ABI symbols documented below and covered by `tests/execution_tests.rs` + `tests/e8_concurrency_stress_tests.rs` |
| Fixed-width wrapper surface (`*_u32`, typed handle/count aliases) | Supported | Type/API contracts in this doc and execution/unit coverage for wrapper helpers |
| `std.net` async operations requiring `effects { concurrency }` | Supported | Async reactor runtime in `docs/async-event-loop.md` with submit/wait/poll/cancel coverage |
| `await` submit bridge for async net/tls handles | Supported | Cooperative poll helper usage (`aic_rt_async_poll_int`, `aic_rt_async_poll_string`) and async bridge execution coverage |
| Linux/macOS concurrency runtime paths | Supported | Full task/channel/mutex/rwlock/arc/atomic behavior implemented |
| Windows concurrency runtime paths | Supported (client-runtime scope) | Shared runtime backend in `src/codegen/runtime/part03.c` + Windows CI smoke coverage for `exec_concurrency_worker_pool_is_deterministic` alongside async/net coordination tests |

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

trait Send[T];
trait Sync[T];

type ConcurrencyCapacityU32 = UInt32;
type ConcurrencyIndexU32 = UInt32;
type ConcurrencyHandleU32 = UInt32;
type ConcurrencyPayloadIdU32 = UInt32;
type ConcurrencyCountU32 = UInt32;

enum GuardKind { MutexGuardKind, RwLockWriteGuardKind }

struct Task[T] { handle: Int }
struct Scope { handle: Int }
struct Sender[T] { handle: Int }
struct Receiver[T] { handle: Int }
enum SelectResult[A, B] { First(A), Second(B), Timeout, Closed }
struct IntTaskSelection { task_index: Int, value: Int }
struct IntTaskSelectionU32 { task_index: ConcurrencyIndexU32, value: Int }
struct IntChannel { handle: Int }
struct IntChannelSelection { channel_index: Int, value: Int }
struct IntChannelSelectionU32 { channel_index: ConcurrencyIndexU32, value: Int }
struct IntMutex { handle: Int }
struct Arc[T] { handle: Int }
struct AtomicInt { handle: Int }
struct AtomicBool { handle: Int }
struct ThreadLocal[T] { handle: Int }
struct Mutex[T] { handle: Int }
struct MutexGuard[T] { handle: Int, guard_kind: GuardKind, value: T }
struct RwLock[T] { handle: Int }
struct IntRwLock { handle: Int }
```

## API

Typechecker compatibility rule: `std.concurrent.spawn`, `spawn_named`, `scope_spawn`, `send`, and `try_send`
enforce that payload type `T` is `Send` at compile time, while keeping stable public signatures.
Fixed-width wrappers are additive: legacy `Int` signatures remain available for runtime/backward compatibility.

```aic
fn spawn[T](f: Fn() -> T) -> Task[T] effects { concurrency }
fn spawn_named[T](name: String, f: Fn() -> T) -> Task[T] effects { concurrency }
fn join[T](task: Task[T]) -> Result[T, ConcurrencyError] effects { concurrency }
fn join_value[T](task: Task[T]) -> Result[T, ConcurrencyError] effects { concurrency }
fn task_handle_u32[T](task: Task[T]) -> Result[ConcurrencyHandleU32, ConcurrencyError]
fn scope_handle_u32(scope: Scope) -> Result[ConcurrencyHandleU32, ConcurrencyError]
fn sender_handle_u32[T](tx: Sender[T]) -> Result[ConcurrencyHandleU32, ConcurrencyError]
fn receiver_handle_u32[T](rx: Receiver[T]) -> Result[ConcurrencyHandleU32, ConcurrencyError]

fn scoped[T](f: Fn(Scope) -> T) -> T effects { concurrency }
fn scope_spawn[T](scope: Scope, f: Fn() -> T) -> Task[T] effects { concurrency }
fn scope_join_all(scope: Scope) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn scope_cancel(scope: Scope) -> Result[Bool, ConcurrencyError] effects { concurrency }

fn spawn_task(value: Int, delay_ms: Int) -> Result[Task[Int], ConcurrencyError] effects { concurrency }
fn join_task(task: Task[Int]) -> Result[Int, ConcurrencyError] effects { concurrency }
fn timeout_task(task: Task[Int], timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency }
fn cancel_task(task: Task[Int]) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn spawn_group(values: Vec[Int], delay_ms: Int) -> Result[Vec[Int], ConcurrencyError] effects { concurrency }
fn select_first(tasks: Vec[Task[Int]], timeout_ms: Int) -> Result[IntTaskSelection, ConcurrencyError] effects { concurrency }
fn select_first_u32(tasks: Vec[Task[Int]], timeout_ms: Int) -> Result[IntTaskSelectionU32, ConcurrencyError] effects { concurrency }

fn channel[T]() -> (Sender[T], Receiver[T]) effects { concurrency }
fn buffered_channel[T](capacity: Int) -> (Sender[T], Receiver[T]) effects { concurrency }
fn buffered_channel_u32[T](capacity: ConcurrencyCapacityU32) -> (Sender[T], Receiver[T]) effects { concurrency }
fn send[T](tx: Sender[T], value: T) -> Result[Bool, ChannelError] effects { concurrency }
fn recv[T](rx: Receiver[T]) -> Result[T, ChannelError] effects { concurrency }
fn try_send[T](tx: Sender[T], value: T) -> Result[Bool, ChannelError] effects { concurrency }
fn try_recv[T](rx: Receiver[T]) -> Result[T, ChannelError] effects { concurrency }
fn recv_timeout[T](rx: Receiver[T], timeout_ms: Int) -> Result[T, ChannelError] effects { concurrency }
fn bytes_channel() -> (Sender[Bytes], Receiver[Bytes]) effects { concurrency }
fn buffered_bytes_channel(capacity: Int) -> (Sender[Bytes], Receiver[Bytes]) effects { concurrency }
fn buffered_bytes_channel_u32(capacity: ConcurrencyCapacityU32) -> (Sender[Bytes], Receiver[Bytes]) effects { concurrency }
fn send_bytes(tx: Sender[Bytes], value: Bytes) -> Result[Bool, ChannelError] effects { concurrency }
fn try_send_bytes(tx: Sender[Bytes], value: Bytes) -> Result[Bool, ChannelError] effects { concurrency }
fn recv_bytes(rx: Receiver[Bytes]) -> Result[Bytes, ChannelError] effects { concurrency }
fn try_recv_bytes(rx: Receiver[Bytes]) -> Result[Bytes, ChannelError] effects { concurrency }
fn recv_bytes_timeout(rx: Receiver[Bytes], timeout_ms: Int) -> Result[Bytes, ChannelError] effects { concurrency }
fn select2[A, B](rx1: Receiver[A], rx2: Receiver[B], timeout_ms: Int) -> SelectResult[A, B] effects { concurrency }
fn select_any[T](receivers: Vec[Receiver[T]], timeout_ms: Int) -> Result[(Int, T), ChannelError] effects { concurrency, env }
fn select_any_u32[T](receivers: Vec[Receiver[T]], timeout_ms: Int) -> Result[(ConcurrencyIndexU32, T), ChannelError] effects { concurrency, env }
fn close_sender[T](tx: Sender[T]) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn close_receiver[T](rx: Receiver[T]) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn store_payload_for_channel_u32[T](value: T) -> Result[ConcurrencyPayloadIdU32, ChannelError] effects { concurrency }
fn take_payload_string_u32(payload_id: ConcurrencyPayloadIdU32) -> Result[String, ChannelError] effects { concurrency }
fn take_payload_for_channel_u32[T](payload_id: ConcurrencyPayloadIdU32, hint: Receiver[T]) -> Result[T, ChannelError] effects { concurrency }
fn store_payload_for_channel[T](value: T) -> Result[Int, ChannelError] effects { concurrency }

fn channel_int(capacity: Int) -> Result[IntChannel, ConcurrencyError] effects { concurrency }
fn buffered_channel_int(capacity: Int) -> Result[IntChannel, ConcurrencyError] effects { concurrency }
fn channel_int_buffered(capacity: Int) -> Result[IntChannel, ConcurrencyError] effects { concurrency }
fn channel_int_u32(capacity: ConcurrencyCapacityU32) -> Result[IntChannel, ConcurrencyError] effects { concurrency }
fn buffered_channel_int_u32(capacity: ConcurrencyCapacityU32) -> Result[IntChannel, ConcurrencyError] effects { concurrency }
fn channel_int_buffered_u32(capacity: ConcurrencyCapacityU32) -> Result[IntChannel, ConcurrencyError] effects { concurrency }
fn send_int(ch: IntChannel, value: Int, timeout_ms: Int) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn recv_int(ch: IntChannel, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency }
fn try_send_int(ch: IntChannel, value: Int) -> Result[Bool, ChannelError] effects { concurrency }
fn try_recv_int(ch: IntChannel) -> Result[Int, ChannelError] effects { concurrency }
fn select_recv_int(ch1: IntChannel, ch2: IntChannel, timeout_ms: Int) -> Result[IntChannelSelection, ChannelError] effects { concurrency }
fn select_recv_int_u32(ch1: IntChannel, ch2: IntChannel, timeout_ms: Int) -> Result[IntChannelSelectionU32, ChannelError] effects { concurrency }
fn close_channel(ch: IntChannel) -> Result[Bool, ConcurrencyError] effects { concurrency }

fn mutex_int(initial: Int) -> Result[IntMutex, ConcurrencyError] effects { concurrency }
fn lock_int(mutex: IntMutex, timeout_ms: Int) -> Result[Int, ConcurrencyError] effects { concurrency }
fn unlock_int(mutex: IntMutex, value: Int) -> Result[Bool, ConcurrencyError] effects { concurrency }
fn close_mutex(mutex: IntMutex) -> Result[Bool, ConcurrencyError] effects { concurrency }

fn arc_new[T](value: T) -> Arc[T] effects { concurrency }
fn arc_clone[T](a: Arc[T]) -> Arc[T] effects { concurrency }
fn arc_handle_u32[T](a: Arc[T]) -> Result[ConcurrencyHandleU32, ConcurrencyError]
fn arc_get[T](a: Arc[T]) -> Result[T, ConcurrencyError] effects { concurrency }
fn arc_strong_count[T](a: Arc[T]) -> Int effects { concurrency }
fn arc_strong_count_u32[T](a: Arc[T]) -> Result[ConcurrencyCountU32, ConcurrencyError] effects { concurrency }
fn atomic_int(initial: Int) -> AtomicInt effects { concurrency }
fn atomic_load(a: AtomicInt) -> Int effects { concurrency }
fn atomic_store(a: AtomicInt, value: Int) -> () effects { concurrency }
fn atomic_add(a: AtomicInt, delta: Int) -> Int effects { concurrency }
fn atomic_sub(a: AtomicInt, delta: Int) -> Int effects { concurrency }
fn atomic_cas(a: AtomicInt, expected: Int, desired: Int) -> Bool effects { concurrency }
fn atomic_bool(initial: Bool) -> AtomicBool effects { concurrency }
fn atomic_load_bool(a: AtomicBool) -> Bool effects { concurrency }
fn atomic_store_bool(a: AtomicBool, value: Bool) -> () effects { concurrency }
fn atomic_swap_bool(a: AtomicBool, desired: Bool) -> Bool effects { concurrency }
fn thread_local[T](init: Fn() -> T) -> ThreadLocal[T] effects { concurrency }
fn tl_get[T](tl: ThreadLocal[T]) -> T effects { concurrency }
fn tl_set[T](tl: ThreadLocal[T], value: T) -> () effects { concurrency }

fn new_mutex[T](value: T) -> Mutex[T] effects { concurrency }
fn lock[T](m: Mutex[T]) -> Result[MutexGuard[T], ConcurrencyError] effects { concurrency }
fn try_lock[T](m: Mutex[T]) -> Result[MutexGuard[T], ConcurrencyError] effects { concurrency }
fn lock_timeout[T](m: Mutex[T], ms: Int) -> Result[MutexGuard[T], ConcurrencyError] effects { concurrency }
fn guard_value[T](g: MutexGuard[T]) -> T
fn guard_set[T](g: MutexGuard[T], value: T) -> MutexGuard[T]
fn unlock_guard[T](g: MutexGuard[T]) -> () effects { concurrency }

fn new_rwlock[T](value: T) -> RwLock[T] effects { concurrency }
fn read_lock[T](rw: RwLock[T]) -> Result[T, ConcurrencyError] effects { concurrency }
fn write_lock[T](rw: RwLock[T]) -> Result[MutexGuard[T], ConcurrencyError] effects { concurrency }
fn close_rwlock[T](rw: RwLock[T]) -> Result[Bool, ConcurrencyError] effects { concurrency }
```

## Arc Usage Pattern

Idiomatic shared mutable state uses `Arc[Mutex[T]]`:

```aic
import std.concurrent;
import std.map;

let base: Map[String, Int] = map.new_map();
let seeded = map.insert(base, "count", 0);
let shared: Arc[Mutex[Map[String, Int]]] = arc_new(new_mutex(seeded));

let worker_shared: Arc[Mutex[Map[String, Int]]] = arc_clone(shared);
let _task: Task[Int] = spawn_named("worker", || -> Int {
    match arc_get(worker_shared) {
        Ok(mutex) => match lock(mutex) {
            Ok(g) => {
                let current = match map.get(g.value, "count") { Some(v) => v, None => 0 };
                let next = map.insert(g.value, "count", current + 1);
                unlock_guard(guard_set(g, next));
                1
            },
            Err(_) => 0,
        },
        Err(_) => 0,
    }
});
```

## Atomic Usage Pattern

Use atomics for lock-free counters/flags where shared mutable state does not require compound invariants:

```aic
import std.concurrent;
import std.vec;

let counter = atomic_int(0);
let mut tasks: Vec[Task[Int]] = vec.new_vec();
let mut i = 0;
while i < 10 {
    tasks = vec.push(tasks, spawn(|| -> Int {
        let mut j = 0;
        while j < 1000 {
            let _old = atomic_add(counter, 1);
            j = j + 1;
        };
        1
    }));
    i = i + 1;
};
// counter == 10000 after joins
```

## Thread-Local Usage Pattern

Use `ThreadLocal[T]` when each thread should own an independent copy of state without lock contention:

```aic
import std.concurrent;

let init_runs = atomic_int(0);
let tl = thread_local(|| -> Int {
    let _old = atomic_add(init_runs, 1);
    5
});

let _task: Task[Int] = spawn(|| -> Int {
    tl_set(tl, 11);
    tl_get(tl) // 11 in this worker thread
});

let main_value = tl_get(tl); // 5 in main thread
```

Key guarantees:
- Values are isolated per thread.
- Initialization is lazy: the `init` closure runs on first `tl_get` per thread.
- `tl_get`/`tl_set` avoid cross-thread locking because each thread owns its local value.

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

- `mutex_int` / `lock_int` / `unlock_int` -> `new_mutex[T]` / `lock[T]` / `unlock_guard[T]`
- `Task` + `spawn_task(value, delay_ms)` -> `Task[T]` + closure-based `spawn(fn() -> T)`
- Named/structured threads:
  - `spawn_named("worker-a", || -> T { ... })` for debuggable thread labels
  - `scoped(|scope| -> T { let _t = scope_spawn(scope, || -> U { ... }); ... })` for join-on-scope-exit guarantees
- During transition, legacy forms remain supported and should be migrated incrementally with deterministic diagnostics.

Sunset policy:

- Legacy `IntChannel` APIs are compatibility wrappers in the 0.2.x line.
- New code should use generic APIs by default.
- Agent migrations should treat `E6001` warnings as actionable and rewrite callsites incrementally.

## Runtime Semantics

- Generic task scheduler:
  - Runtime uses host threads with bounded handle tables and payload slots.
  - Fixed-width wrapper surfaces expose those non-negative domains as `ConcurrencyHandleU32` and `ConcurrencyPayloadIdU32`.
  - `spawn(f)` executes closure `f` on a dedicated worker thread and returns `Task[T]`.
  - `join(task)` / `join_value(task)` block until completion and deserialize captured result payload as `T`.
  - `spawn_named(name, f)` behaves like `spawn`, additionally attaching best-effort OS thread names on supported platforms.
- Scoped threads:
  - `scoped(f)` creates a runtime scope, executes `f(scope)`, and always performs `scope_join_all` + scope close before returning.
  - `scope_spawn(scope, f)` ties child task lifecycle to the owning scope.
  - `scope_cancel(scope)` requests cooperative cancellation for all tasks currently tracked by that scope.
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
  - `send[T]` stores typed payload snapshots in concurrency payload slots via binary-safe value codec intrinsics.
  - Channel runtime transports payload IDs across thread boundaries.
  - `ConcurrencyPayloadIdU32` wrappers provide the fixed-width view; legacy `Int` payload-id paths remain for ABI parity.
  - `recv[T]`/`try_recv[T]`/`recv_timeout[T]` decode payload snapshots back to `T` with runtime size checks.
  - On send failure paths, staged payloads are dropped to avoid payload-slot leaks.
  - `send_bytes`/`try_send_bytes` and `recv_bytes`/`try_recv_bytes`/`recv_bytes_timeout` remain the explicit binary-bytes fast path for protocol payloads.
  - Compatibility guidance: generic channel payloads are now opaque binary snapshots; if tooling relied on JSON payload text in channel internals, migrate to explicit `std.json` encode/decode at the application boundary.
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
  - `select_any_u32(receivers, timeout_ms)` is the phase-1 fixed-width wrapper that returns `ConcurrencyIndexU32` indices.
  - `select_any` probes receivers in rotating order and uses bounded 1ms waits to avoid fixed-index starvation.
- Fixed-width phase-1 wrappers:
  - `*_u32` capacity wrappers migrate non-negative queue/capacity arguments to `ConcurrencyCapacityU32`.
  - `select_first_u32` / `select_recv_int_u32` / `select_any_u32` migrate selection indices to `ConcurrencyIndexU32`.
  - Handle/payload/counter domains now expose additive aliases: `ConcurrencyHandleU32`, `ConcurrencyPayloadIdU32`, and `ConcurrencyCountU32`.
  - `arc_strong_count_u32` provides the fixed-width Arc strong-count surface.
  - Legacy `Int` signatures remain available for compatibility/runtime parity.
- Scalar taxonomy artifact for this wave: `docs/io-fixed-width-taxonomy-wave2.md`.
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
  - `Mutex[T]` stores typed payloads via concurrency payload slots and exposes updates through `MutexGuard[T]`.
  - `MutexGuard[T].guard_kind` uses typed discriminator `GuardKind` (`MutexGuardKind` or `RwLockWriteGuardKind`) instead of raw sentinel integers.
  - `RwLock[T]` supports concurrent read access and exclusive write access.
  - Read paths clone payload handles to keep read operations non-destructive.
- Arc shared ownership:
  - `arc_new` encodes payloads and stores them behind shared handles.
  - `arc_clone` increments Arc refcount with sequentially-consistent atomics (`fetch_add`).
  - `arc_release` is runtime-managed and decrements refcount with sequentially-consistent atomics (`fetch_sub`).
  - Arc payload storage is freed automatically when strong count reaches `0`.
  - `arc_strong_count_u32` is additive and returns `Result[ConcurrencyCountU32, ConcurrencyError]`; `arc_strong_count` remains available for legacy `Int` callers.
  - `Arc[T]` is treated as a thread-safe wrapper and supports `Arc[Mutex[T]]` for shared mutable state.
- Lock-free atomics:
  - `atomic_add`/`atomic_sub` lower to host `fetch_add`/`fetch_sub` with sequential consistency.
  - `atomic_cas` lowers to compare-and-swap (`compare_exchange`) with sequential consistency.
  - `atomic_*_bool` operations map to sequentially-consistent load/store/swap on bool slots.
  - Atomic handle allocation uses lock-free slot activation (`compare_exchange` on active flags).
- Thread-local storage:
  - `thread_local(init)` registers an init closure and value layout for per-thread storage.
  - `tl_get` lazily initializes the current thread value on first access, then returns that thread's copy.
  - `tl_set` updates only the current thread's value and does not affect other threads.

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
- `aic_rt_conc_spawn_fn`
- `aic_rt_conc_spawn_fn_named`
- `aic_rt_conc_join`
- `aic_rt_conc_join_value`
- `aic_rt_conc_join_timeout`
- `aic_rt_conc_cancel`
- `aic_rt_conc_scope_new`
- `aic_rt_conc_scope_spawn_fn`
- `aic_rt_conc_scope_join_all`
- `aic_rt_conc_scope_cancel`
- `aic_rt_conc_scope_close`
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
- `aic_rt_conc_rwlock_int`
- `aic_rt_conc_rwlock_read`
- `aic_rt_conc_rwlock_write_lock`
- `aic_rt_conc_rwlock_write_unlock`
- `aic_rt_conc_rwlock_close`
- `aic_rt_conc_payload_store`
- `aic_rt_conc_payload_take`
- `aic_rt_conc_payload_drop`
- `aic_rt_conc_arc_new`
- `aic_rt_conc_arc_clone`
- `aic_rt_conc_arc_get`
- `aic_rt_conc_arc_strong_count`
- `aic_rt_conc_arc_release`
- `aic_rt_conc_atomic_int_new`
- `aic_rt_conc_atomic_int_load`
- `aic_rt_conc_atomic_int_store`
- `aic_rt_conc_atomic_int_add`
- `aic_rt_conc_atomic_int_sub`
- `aic_rt_conc_atomic_int_cas`
- `aic_rt_conc_atomic_bool_new`
- `aic_rt_conc_atomic_bool_load`
- `aic_rt_conc_atomic_bool_store`
- `aic_rt_conc_atomic_bool_swap`
- `aic_rt_conc_tl_new`
- `aic_rt_conc_tl_get`
- `aic_rt_conc_tl_set`
- `aic_rt_async_poll_int`
- `aic_rt_async_poll_string`

`arc_strong_count_u32` is a std-level fixed-width wrapper over `aic_rt_conc_arc_strong_count` to preserve runtime ABI stability.

## Platform Limitations

- Linux/macOS:
  - Full channel/task/mutex runtime behavior is implemented.
- Windows:
  - Existing concurrency APIs (including generic spawn/join/scoped APIs, Arc APIs, and atomic APIs) return `ConcurrencyError::Io` for unsupported runtime paths.
  - Channel try/select APIs return deterministic `ChannelError::Closed`.

## Pattern Examples

Use these curated examples as the migration and implementation baseline for agents:

- Producer-consumer:
  - `examples/io/worker_pool.aic`
  - `examples/io/generic_channel_types.aic`
- Shared state:
  - `examples/io/mutex_rwlock_shared_state.aic`
  - `examples/io/arc_mutex_shared_ownership.aic`
  - `examples/io/thread_local_isolation.aic`
- Cancellation and select:
  - `examples/io/structured_concurrency.aic`
  - `examples/io/select_multi_channel.aic`
- Migration from legacy Int-only channel APIs:
  - `examples/io/channel_migration_compat.aic`
- Misuse diagnostics (expected to fail under `aic check`):
  - `examples/verify/generic_channel_protocol_invalid.aic` (resource protocol misuse; `E2006`)

CI integration:
- `scripts/ci/examples.sh check` validates positive examples plus negative diagnostics.
- `scripts/ci/examples.sh run` validates executable behavior for runnable examples.
- `tests/e8_concurrency_stress_tests.rs` provides deterministic stress/replay coverage for concurrency runtime behavior.

## Agent Troubleshooting

Common agent-facing failures and fixes:

- `E1258` (`Send` bound failure):
  - Cause: sending/spawning non-`Send` payloads (for example runtime handles) across threads.
  - Fix: move only `Send` values across thread boundaries; wrap shared mutable state as `Arc[Mutex[T]]` or pass serializable data.
- `E2006` (resource protocol violation):
  - Cause: illegal operation order (for example using a closed channel handle).
  - Fix: enforce lifecycle protocol in control flow; prefer wrappers that encode close/join ordering.
- `Err(Timeout)` on blocking channel/lock operations:
  - Cause: insufficient capacity, missing consumer, or lock contention beyond timeout budget.
  - Fix: increase channel capacity, ensure consumer/task liveness, or use non-blocking `try_*` APIs with retry policy.
- `Err(Closed)` during send/recv/select:
  - Cause: peer closed channel or selected handle already terminated.
  - Fix: treat `Closed` as terminal and unwind producer/consumer loops cleanly.
- `Err(NotFound)` for handles:
  - Cause: invalid or stale runtime handle (often from fallback/default handle values after creation failure).
  - Fix: guard creation results and avoid using wrappers when underlying runtime creation failed.

## Example

- `examples/io/worker_pool.aic`
- `examples/io/mutex_rwlock_shared_state.aic`
- `examples/io/arc_mutex_shared_ownership.aic`
- `examples/io/atomic_counter_vs_mutex.aic`
- `examples/io/thread_local_isolation.aic`
- `examples/io/channel_migration_compat.aic`
- `examples/io/generic_channel_types.aic`
- `examples/io/structured_concurrency.aic`
- `examples/io/select_multi_channel.aic`
- `examples/io/async_await_submit_bridge.aic`
- `examples/verify/generic_channel_protocol_ok.aic` (`aic check`)
- `examples/verify/generic_channel_protocol_invalid.aic` (`aic check`, expected `E2006`)
