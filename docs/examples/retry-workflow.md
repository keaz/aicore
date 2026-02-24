# Retry Workflow (IO)

This workflow demonstrates using `std.retry` for retry/backoff and deadline handling in IO flows.

## APIs Used

- `default_retry_config() -> RetryConfig`
- `retry[T](config, operation) -> RetryResult[T] effects { time, rand }`
- `with_timeout[T](timeout_ms, operation) -> Result[T, String] effects { time }`

## Recommended Pattern

1. Start from `default_retry_config()`.
2. Override `max_attempts`, backoff settings, and jitter based on workload.
3. Wrap transient operations with `retry(...)`.
4. Wrap bounded operations with `with_timeout(...)`.
5. Inspect `RetryResult.attempts` and `RetryResult.elapsed_ms` for observability.

## Runnable Example

- `examples/io/retry_with_jitter.aic`

Run:

```bash
cargo run --quiet --bin aic -- run examples/io/retry_with_jitter.aic
```

Expected final line: `42`.
