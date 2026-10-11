# Connection Pool Runtime (`std.pool`)

`std.pool` provides a generic connection/resource pool for concurrency-heavy workloads.

Use it for reusable resources like DB client handles, TCP sessions, or expensive authenticated channels.

## API Surface

```aic
enum PoolError {
    MaxSizeReached,
    Timeout,
    ConnectionFailed,
    Closed,
    HealthCheckFailed,
}

struct PoolConfig {
    min_size: Int,
    max_size: Int,
    acquire_timeout_ms: Int,
    idle_timeout_ms: Int,
    max_lifetime_ms: Int,
    health_check_ms: Int,
}

struct Pool[T] {
    handle: Int,
}

struct PooledConn[T] {
    handle: Int,
    value: T,
}

struct PoolStats {
    total: Int,
    idle: Int,
    in_use: Int,
    created: Int,
    destroyed: Int,
}

fn new_pool[T](
    config: PoolConfig,
    create: Fn() -> Result[T, PoolError],
    health_check: Fn(T) -> Bool,
    destroy: Fn(T) -> (),
) -> Result[Pool[T], PoolError] effects { concurrency }

fn acquire[T](pool: Pool[T]) -> Result[PooledConn[T], PoolError] effects { concurrency }
fn release[T](conn: PooledConn[T]) -> () effects { concurrency }
fn discard[T](conn: PooledConn[T]) -> () effects { concurrency }
fn pool_stats[T](pool: Pool[T]) -> PoolStats effects { concurrency }
fn close_pool[T](pool: Pool[T]) -> () effects { concurrency }
```

## Agent-Safe Usage Pattern

Prefer explicit callback typing and explicit pool-result typing:

```aic
import std.concurrent;
import std.pool;

struct Conn {
    id: Int,
    healthy: Bool,
}

fn main() -> Int effects { concurrency } capabilities { concurrency } {
    let create_cb: Fn() -> Result[Conn, PoolError] =
        || -> Result[Conn, PoolError] { Ok(Conn { id: 1, healthy: true }) };
    let health_cb: Fn(Conn) -> Bool = |c: Conn| -> Bool { c.healthy };
    let destroy_cb: Fn(Conn) -> () = |c: Conn| -> () { () };

    let pool_result: Result[Pool[Conn], PoolError] = new_pool(
        PoolConfig {
            min_size: 1,
            max_size: 5,
            acquire_timeout_ms: 100,
            idle_timeout_ms: 30,
            max_lifetime_ms: 0,
            health_check_ms: 10,
        },
        create_cb,
        health_cb,
        destroy_cb,
    );

    let pool: Pool[Conn] = match pool_result {
        Ok(p) => p,
        Err(_) => Pool { handle: 0 },
    };

    let acquired: Result[PooledConn[Conn], PoolError] = acquire(pool);
    match acquired {
        Ok(conn) => release(conn),
        Err(_) => (),
    };

    close_pool(pool);
    0
}
```

## Behavioral Guarantees

- Thread-safe manager state updates are guarded by `Mutex`.
- `min_size` is lazily maintained.
- `max_size` is enforced during acquire.
- Idle/lifetime pruning removes expired connections.
- Periodic idle health-check pruning is supported.
- `discard(...)` destroys broken resources instead of returning them to idle.
- `pool_stats(...)` returns current totals for observability.
- `acquire(...)` keeps one request in flight. It waits for that request's reply in
  1 ms steps until `acquire_timeout_ms` steps are used, and sends a new request only
  after the pool answers `MaxSizeReached`. A slow manager therefore never hands out
  extra leases that no caller holds.
- When `acquire(...)` gives up with `Timeout` while its request is still pending, the
  connection the manager later grants for that request goes back to the idle list. A
  timed-out acquire never leaves a connection counted as `in_use`.

## Timing Model

- The manager ages connections once per loop iteration. An iteration waits up to
  1 ms for a request, so `idle_timeout_ms`, `max_lifetime_ms` and `health_check_ms`
  are lower bounds in wall-clock time. On a loaded machine, iterations take longer
  and expiry happens later than the configured value, never earlier.
- `pool_stats(...)` waits up to 20 ms for the manager. If the manager does not
  answer in that time, it returns all-zero stats. Treat `created == 0` on a pool that
  has created connections as "no answer" and ask again.
- Tests and examples that depend on expiry must wait for the observable effect (for
  example, `pool_stats(...).destroyed` increasing), not for a fixed sleep.
  `examples/io/connection_pool.aic` shows this pattern.

## Known Limitations

- The runtime does not reuse channel slots. A process can create at most 128
  channels (`AIC_RT_LIMIT_CONC_CHANNELS` can only lower this), and closing a channel
  does not free its slot. `new_pool(...)` uses one channel, and every `acquire(...)`
  round and every `pool_stats(...)` call opens one reply channel. A process that
  makes more than about 125 of these calls in total gets channel-creation failures,
  which `acquire(...)` reports as `Closed`. Keep long-running pool workloads and
  stats polling loops well under this budget until slot reclamation lands in the
  runtime (follow-up: free channel slots once both ends are closed, with
  stale-handle protection).

## Validation

- Runnable integration example: `examples/io/connection_pool.aic` (expected final line `42`).
- Execution tests:
  - `exec_pool_ten_workers_share_five_connections_without_leaks`
  - `exec_pool_idle_connections_are_recycled_after_timeout`
  - `exec_pool_discarded_connection_is_replaced`
  - `exec_pool_slow_acquire_reply_does_not_leak_lease`
  - `exec_pool_abandoned_acquire_returns_lease_to_pool`
  - `exec_runtime_pool_churn_sustained_cycles_are_leak_free`
