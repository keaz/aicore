# Network, Time, And Random APIs

This document provides the detailed contract for `std.net`, `std.time`, and `std.rand`.

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
}

struct UdpPacket {
    from: String,
    payload: String,
}
```

### API

```aic
fn tcp_listen(addr: String) -> Result[Int, NetError] effects { net }
fn tcp_local_addr(handle: Int) -> Result[String, NetError] effects { net }
fn tcp_accept(listener: Int, timeout_ms: Int) -> Result[Int, NetError] effects { net }
fn tcp_connect(addr: String, timeout_ms: Int) -> Result[Int, NetError] effects { net }
fn tcp_send(handle: Int, payload: String) -> Result[Int, NetError] effects { net }
fn tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[String, NetError] effects { net }
fn tcp_close(handle: Int) -> Result[Bool, NetError] effects { net }

fn udp_bind(addr: String) -> Result[Int, NetError] effects { net }
fn udp_local_addr(handle: Int) -> Result[String, NetError] effects { net }
fn udp_send_to(handle: Int, addr: String, payload: String) -> Result[Int, NetError] effects { net }
fn udp_recv_from(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[UdpPacket, NetError] effects { net }
fn udp_close(handle: Int) -> Result[Bool, NetError] effects { net }

fn dns_lookup(host: String) -> Result[String, NetError] effects { net }
fn dns_reverse(addr: String) -> Result[String, NetError] effects { net }
```

### Runtime Behavior

- TCP/UDP handles are bounded runtime resources; always close handles.
- `timeout_ms` is explicit in accept/connect/recv APIs for liveness control.
- DNS reverse may legitimately return `NotFound` for unmapped addresses.

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
