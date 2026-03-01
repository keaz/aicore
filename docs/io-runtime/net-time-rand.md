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
}

struct UdpPacket {
    from: String,
    payload: Bytes,
}

struct TcpStream {
    handle: Int,
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
fn tcp_stream(handle: Int) -> TcpStream
fn tcp_stream_send(stream: TcpStream, payload: Bytes) -> Result[Int, NetError] effects { net }
fn tcp_stream_send_timeout(stream: TcpStream, payload: Bytes, timeout_ms: Int) -> Result[Int, NetError] effects { net }
fn tcp_stream_recv(stream: TcpStream, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net }
fn tcp_stream_recv_exact_deadline(stream: TcpStream, expected_bytes: Int, deadline_ms: Int) -> Result[Bytes, NetError] effects { net, time }
fn tcp_stream_recv_exact(stream: TcpStream, expected_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net, time }
fn tcp_stream_recv_framed_deadline(stream: TcpStream, max_frame_bytes: Int, deadline_ms: Int) -> Result[Bytes, NetError] effects { net, time }
fn tcp_stream_recv_framed(stream: TcpStream, max_frame_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError] effects { net, time }
fn tcp_stream_close(stream: TcpStream) -> Result[Bool, NetError] effects { net }

fn udp_bind(addr: String) -> Result[Int, NetError] effects { net }
fn udp_local_addr(handle: Int) -> Result[String, NetError] effects { net }
fn udp_send_to(handle: Int, addr: String, payload: Bytes) -> Result[Int, NetError] effects { net }
fn udp_recv_from(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[UdpPacket, NetError] effects { net }
fn udp_close(handle: Int) -> Result[Bool, NetError] effects { net }

fn dns_lookup(host: String) -> Result[String, NetError] effects { net }
fn dns_reverse(addr: String) -> Result[String, NetError] effects { net }
```

### Runtime Behavior

- TCP/UDP handles are bounded runtime resources; always close handles.
- `timeout_ms` is explicit in accept/connect/recv APIs for liveness control.
- `tcp_send_timeout` and `tcp_stream_send_timeout` enforce timeout-bounded write loops.
- `tcp_recv` reports `ConnectionClosed` on peer EOF/close; `Timeout` remains distinct.
- DNS reverse may legitimately return `NotFound` for unmapped addresses.
- Exact stream reads are deadline-based: `tcp_stream_recv_exact*` keeps reading until `expected_bytes` is met.
- Framed stream reads are length-prefixed: `tcp_stream_recv_framed*` consumes a 4-byte big-endian frame length and enforces `max_frame_bytes`.

### Example

- `examples/io/tcp_echo.aic`

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
