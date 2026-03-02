# Network, Time, Random, And Retry APIs

This document provides the detailed contract for `std.net`, `std.time`, `std.rand`, and `std.retry`.

## `std.net` (`effects { net }`)

### Types

```aic
enum NetError {
    NotFound,
    PermissionDenied,
    Refused,
    Timeout,
    AddressInUse,
    InvalidInput,
    Io,
    ConnectionClosed,
    Cancelled,
}

struct UdpPacket {
    from: String,
    payload: Bytes,
}

struct TcpStream {
    handle: Int,
}

struct AsyncIntOp {
    handle: Int,
}

struct AsyncStringOp {
    handle: Int,
}

struct AsyncIntSelection {
    index: Int,
    value: Int,
}

struct AsyncStringSelection {
    index: Int,
    payload: Bytes,
}

struct AsyncRuntimePressure {
    active_ops: Int,
    queue_depth: Int,
    op_limit: Int,
    queue_limit: Int,
}
```

### API

```aic
fn tcp_listen(addr: String) -> Result[Int, NetError] effects { net }
fn tcp_local_addr(handle: Int) -> Result[String, NetError] effects { net }
fn tcp_accept(listener: Int, timeout_ms: Int) -> Result[Int, NetError] effects { net }
fn tcp_connect(addr: String, timeout_ms: Int) -> Result[Int, NetError] effects { net }
fn tcp_send(handle: Int, payload: Bytes) -> Result[Int, NetError] effects { net }
fn tcp_send_timeout(handle: Int, payload: Bytes, timeout_ms: Int) -> Result[Int, NetError] effects { net }
fn tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net }
fn tcp_close(handle: Int) -> Result[Bool, NetError] effects { net }
fn tcp_set_nodelay(handle: Int, enabled: Bool) -> Result[Bool, NetError] effects { net }
fn tcp_get_nodelay(handle: Int) -> Result[Bool, NetError] effects { net }
fn tcp_set_keepalive(handle: Int, enabled: Bool) -> Result[Bool, NetError] effects { net }
fn tcp_get_keepalive(handle: Int) -> Result[Bool, NetError] effects { net }
fn tcp_set_keepalive_idle_secs(handle: Int, idle_secs: Int) -> Result[Bool, NetError] effects { net }
fn tcp_get_keepalive_idle_secs(handle: Int) -> Result[Int, NetError] effects { net }
fn tcp_set_keepalive_interval_secs(handle: Int, interval_secs: Int) -> Result[Bool, NetError] effects { net }
fn tcp_get_keepalive_interval_secs(handle: Int) -> Result[Int, NetError] effects { net }
fn tcp_set_keepalive_count(handle: Int, probe_count: Int) -> Result[Bool, NetError] effects { net }
fn tcp_get_keepalive_count(handle: Int) -> Result[Int, NetError] effects { net }
fn tcp_peer_addr(handle: Int) -> Result[String, NetError] effects { net }
fn tcp_shutdown(handle: Int) -> Result[Bool, NetError] effects { net }
fn tcp_shutdown_read(handle: Int) -> Result[Bool, NetError] effects { net }
fn tcp_shutdown_write(handle: Int) -> Result[Bool, NetError] effects { net }
fn tcp_set_send_buffer_size(handle: Int, size_bytes: Int) -> Result[Bool, NetError] effects { net }
fn tcp_get_send_buffer_size(handle: Int) -> Result[Int, NetError] effects { net }
fn tcp_set_recv_buffer_size(handle: Int, size_bytes: Int) -> Result[Bool, NetError] effects { net }
fn tcp_get_recv_buffer_size(handle: Int) -> Result[Int, NetError] effects { net }
fn tcp_stream(handle: Int) -> TcpStream
fn tcp_stream_send(stream: TcpStream, payload: Bytes) -> Result[Int, NetError] effects { net }
fn tcp_stream_send_timeout(stream: TcpStream, payload: Bytes, timeout_ms: Int) -> Result[Int, NetError] effects { net }
fn tcp_stream_recv(stream: TcpStream, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net }
fn tcp_stream_recv_exact_deadline(stream: TcpStream, expected_bytes: Int, deadline_ms: Int) -> Result[Bytes, NetError] effects { net, time }
fn tcp_stream_recv_exact(stream: TcpStream, expected_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net, time }
fn tcp_stream_recv_framed_deadline(stream: TcpStream, max_frame_bytes: Int, deadline_ms: Int) -> Result[Bytes, NetError] effects { net, time }
fn tcp_stream_recv_framed(stream: TcpStream, max_frame_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net, time }
fn tcp_stream_close(stream: TcpStream) -> Result[Bool, NetError] effects { net }
fn tcp_stream_set_nodelay(stream: TcpStream, enabled: Bool) -> Result[Bool, NetError] effects { net }
fn tcp_stream_get_nodelay(stream: TcpStream) -> Result[Bool, NetError] effects { net }
fn tcp_stream_set_keepalive(stream: TcpStream, enabled: Bool) -> Result[Bool, NetError] effects { net }
fn tcp_stream_get_keepalive(stream: TcpStream) -> Result[Bool, NetError] effects { net }
fn tcp_stream_set_keepalive_idle_secs(stream: TcpStream, idle_secs: Int) -> Result[Bool, NetError] effects { net }
fn tcp_stream_get_keepalive_idle_secs(stream: TcpStream) -> Result[Int, NetError] effects { net }
fn tcp_stream_set_keepalive_interval_secs(stream: TcpStream, interval_secs: Int) -> Result[Bool, NetError] effects { net }
fn tcp_stream_get_keepalive_interval_secs(stream: TcpStream) -> Result[Int, NetError] effects { net }
fn tcp_stream_set_keepalive_count(stream: TcpStream, probe_count: Int) -> Result[Bool, NetError] effects { net }
fn tcp_stream_get_keepalive_count(stream: TcpStream) -> Result[Int, NetError] effects { net }
fn tcp_stream_peer_addr(stream: TcpStream) -> Result[String, NetError] effects { net }
fn tcp_stream_shutdown(stream: TcpStream) -> Result[Bool, NetError] effects { net }
fn tcp_stream_shutdown_read(stream: TcpStream) -> Result[Bool, NetError] effects { net }
fn tcp_stream_shutdown_write(stream: TcpStream) -> Result[Bool, NetError] effects { net }
fn tcp_stream_set_send_buffer_size(stream: TcpStream, size_bytes: Int) -> Result[Bool, NetError] effects { net }
fn tcp_stream_get_send_buffer_size(stream: TcpStream) -> Result[Int, NetError] effects { net }
fn tcp_stream_set_recv_buffer_size(stream: TcpStream, size_bytes: Int) -> Result[Bool, NetError] effects { net }
fn tcp_stream_get_recv_buffer_size(stream: TcpStream) -> Result[Int, NetError] effects { net }
fn async_accept_submit(listener: Int, timeout_ms: Int) -> Result[AsyncIntOp, NetError] effects { net, concurrency }
fn async_tcp_send_submit(handle: Int, payload: Bytes) -> Result[AsyncIntOp, NetError] effects { net, concurrency }
fn async_tcp_recv_submit(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[AsyncStringOp, NetError] effects { net, concurrency }
fn async_wait_int(op: AsyncIntOp, timeout_ms: Int) -> Result[Int, NetError] effects { net, concurrency }
fn async_wait_string(op: AsyncStringOp, timeout_ms: Int) -> Result[Bytes, NetError] effects { net, concurrency }
fn async_cancel_int(op: AsyncIntOp) -> Result[Bool, NetError] effects { net, concurrency }
fn async_cancel_string(op: AsyncStringOp) -> Result[Bool, NetError] effects { net, concurrency }
fn async_poll_int(op: AsyncIntOp) -> Result[Option[Int], NetError] effects { net, concurrency }
fn async_poll_string(op: AsyncStringOp) -> Result[Option[Bytes], NetError] effects { net, concurrency }
fn async_wait_any_int(op1: AsyncIntOp, op2: AsyncIntOp, timeout_ms: Int) -> Result[AsyncIntSelection, NetError] effects { net, concurrency, time }
fn async_wait_any_string(op1: AsyncStringOp, op2: AsyncStringOp, timeout_ms: Int) -> Result[AsyncStringSelection, NetError] effects { net, concurrency, time }
fn async_runtime_pressure() -> Result[AsyncRuntimePressure, NetError] effects { net, concurrency }
fn async_shutdown() -> Result[Bool, NetError] effects { net, concurrency }
fn async_accept(listener: Int, timeout_ms: Int) -> Result[Int, NetError] effects { net, concurrency }
fn async_tcp_send(handle: Int, payload: Bytes, timeout_ms: Int) -> Result[Int, NetError] effects { net, concurrency }
fn async_tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net, concurrency }

fn udp_bind(addr: String) -> Result[Int, NetError] effects { net }
fn udp_local_addr(handle: Int) -> Result[String, NetError] effects { net }
fn udp_send_to(handle: Int, addr: String, payload: Bytes) -> Result[Int, NetError] effects { net }
fn udp_recv_from(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[UdpPacket, NetError] effects { net }
fn udp_close(handle: Int) -> Result[Bool, NetError] effects { net }

fn dns_lookup(host: String) -> Result[String, NetError] effects { net }
fn dns_lookup_all(host: String) -> Result[Vec[String], NetError] effects { net }
fn dns_reverse(addr: String) -> Result[String, NetError] effects { net }
```

### Runtime Behavior

- TCP/UDP handles are bounded runtime resources; always close handles.
- Runtime handle ceilings are process-start configurable:
  - `AIC_RT_LIMIT_FS_FILES`, `AIC_RT_LIMIT_PROC_HANDLES`
  - `AIC_RT_LIMIT_NET_HANDLES`, `AIC_RT_LIMIT_NET_ASYNC_OPS`, `AIC_RT_LIMIT_NET_ASYNC_QUEUE`
  - `AIC_RT_LIMIT_TLS_HANDLES`, `AIC_RT_LIMIT_TLS_ASYNC_OPS`
  - `AIC_RT_LIMIT_CONC_TASKS`, `AIC_RT_LIMIT_CONC_CHANNELS`, `AIC_RT_LIMIT_CONC_MUTEXES`
  - values outside the accepted range fall back to deterministic defaults.
- `timeout_ms` is explicit in accept/connect/recv APIs for liveness control.
- `tcp_send_timeout` and `tcp_stream_send_timeout` enforce timeout-bounded write loops.
- `tcp_recv` reports `ConnectionClosed` on peer EOF/close; `Timeout` remains distinct.
- `async_cancel_*` keeps peer-close separate by surfacing `Cancelled` from cancelled waits.
- `dns_lookup_all` returns de-duplicated numeric host addresses in deterministic lexicographic order.
- Protocol clients can pair `dns_lookup_all` with retry/timeout budgets to attempt each address deterministically.
- DNS reverse may legitimately return `NotFound` for unmapped addresses.
- Exact stream reads are deadline-based: `tcp_stream_recv_exact*` keeps reading until `expected_bytes` is met.
- Framed stream reads are length-prefixed: `tcp_stream_recv_framed*` consumes a 4-byte big-endian frame length and enforces `max_frame_bytes`.
- Socket tuning is explicit and typed:
  - `tcp_set/get_nodelay` toggles Nagle behavior.
  - `tcp_set/get_keepalive` toggles keepalive probes.
  - `tcp_set/get_keepalive_idle_secs`, `tcp_set/get_keepalive_interval_secs`, and `tcp_set/get_keepalive_count` tune keepalive probe behavior where supported.
  - `tcp_set/get_send_buffer_size` and `tcp_set/get_recv_buffer_size` tune kernel buffers (read-back may differ from requested size).
  - `tcp_peer_addr` / `tcp_stream_peer_addr` expose remote endpoint identity for telemetry and policy checks.
  - `tcp_shutdown*` / `tcp_stream_shutdown*` expose half-close/full-close controls for protocol flow control.
- Async lifecycle control is explicit and typed:
  - `async_cancel_*` returns whether cancellation was applied.
  - `async_poll_*` maps pending state to `Option::None`.
  - `async_wait_any_*` provides deterministic two-op select helpers.
  - `async_runtime_pressure` reports active/queued snapshots and configured limits for adaptive submit gating.
- Recommended protocol-client defaults:
  - Request/response clients (PostgreSQL, Redis, RPC) usually start with `tcp_set_nodelay(..., true)`.
  - Long-lived pooled connections usually start with `tcp_set_keepalive(..., true)`.
  - Tune keepalive probes with `tcp_set_keepalive_idle_secs`, `tcp_set_keepalive_interval_secs`, and `tcp_set_keepalive_count` when idle-failure detection latency matters.
  - Start buffer sizing with moderate values (for example `8192`-`65536`) and tune by measured throughput/latency.
  - Size `AIC_RT_LIMIT_NET_ASYNC_OPS` for peak in-flight async requests and `AIC_RT_LIMIT_NET_ASYNC_QUEUE` for expected submit bursts.
- Sustained-load lifecycle verification is CI-gated:
  - `exec_runtime_net_async_lifecycle_sustained_churn_is_leak_free`
  - `exec_runtime_tls_async_lifecycle_sustained_churn_is_leak_free`
  - both tests run repeated submit/wait/cancel cycles under low runtime limits to catch handle/op leak regressions.
- Unsupported socket-option paths return `NetError::Io` deterministically (including current Windows runtime behavior).
- Invalid-handle/type socket-control calls remain typed (`NetError::InvalidInput`), and shutdown on already-closed streams may surface `NetError::ConnectionClosed` depending on platform socket state.

### Example

- `examples/io/tcp_echo.aic`
- `examples/io/tcp_socket_tuning.aic`
- `examples/io/async_lifecycle_controls.aic`

## `std.time` (`effects { time }`)

### API

```aic
fn now_ms() -> Int effects { time }
fn monotonic_ms() -> Int effects { time }
fn sleep_ms(ms: Int) -> () effects { time }
fn deadline_after_ms(timeout_ms: Int) -> Int effects { time }
fn remaining_ms(deadline_ms: Int) -> Int effects { time }
fn timeout_expired(deadline_ms: Int) -> Bool effects { time }
fn sleep_until(deadline_ms: Int) -> () effects { time }
```

### Runtime Behavior

- `now_ms` is wall clock time.
- `monotonic_ms` is monotonic runtime clock for deadlines/timeouts.
- Use `deadline_after_ms` + `remaining_ms` to avoid negative sleeps.
- Deterministic test-mode overrides:
  - `AIC_TEST_TIME_MS=<ms>` forces `now_ms()` to the provided millisecond value.
  - `AIC_TEST_MODE=1` forces `now_ms()` to `2026-01-01T00:00:00Z` (`1767225600000`) when `AIC_TEST_TIME_MS` is unset.

### Example

- `examples/io/retry_with_jitter.aic`

## `std.rand` (`effects { rand }`)

### API

```aic
fn seed(seed_value: Int) -> () effects { rand }
fn random_int() -> Int effects { rand }
fn random_bool() -> Bool effects { rand }
fn random_range(min_inclusive: Int, max_exclusive: Int) -> Int effects { rand }
```

### Runtime Behavior

- Seeded runs are deterministic for reproducible workflows.
- `random_range(a, a)` returns `a`.
- Use explicit seeds in tests and CI examples.
- Deterministic test-mode overrides:
  - `AIC_TEST_SEED=<seed>` forces process-level RNG seed on first random call.
  - `AIC_TEST_MODE=1` forces deterministic default seed when `AIC_TEST_SEED` is unset.
  - `aic test` sets deterministic test-mode environment by default (`AIC_TEST_MODE=1`, default seed/time when unset).

### Example

- `examples/io/retry_with_jitter.aic`

## `std.retry` (`effects { time, rand }` for `retry`, `effects { time }` for `with_timeout`)

### Types

```aic
struct RetryConfig {
    max_attempts: Int,
    initial_backoff_ms: Int,
    backoff_multiplier: Int,
    max_backoff_ms: Int,
    jitter_enabled: Bool,
    jitter_ms: Int,
}

struct RetryResult[T] {
    result: Result[T, String],
    attempts: Int,
    elapsed_ms: Int,
}
```

### API

```aic
fn default_retry_config() -> RetryConfig
fn retry[T](config: RetryConfig, operation: Fn() -> Result[T, String]) -> RetryResult[T] effects { time, rand }
fn with_timeout[T](timeout_ms: Int, operation: Fn() -> T) -> Result[T, String] effects { time }
```

### Runtime Behavior

- Backoff progression is exponential and capped by `max_backoff_ms`.
- When `jitter_enabled` is true, retry delay includes `random_range(0, jitter_ms + 1)` and is still capped.
- `RetryResult` exposes final `result`, number of `attempts`, and aggregate `elapsed_ms`.
- `with_timeout` is deadline-based and checks timeout before and after operation execution.

### Example

- `examples/io/retry_with_jitter.aic`
