# Small String Optimization (SSO) for Runtime Map Storage

## Summary

`Map[String, String]` runtime storage now uses inline storage for short strings:

- strings with byte length `<= 23` are stored inline in map entries
- strings with byte length `>= 24` stay heap-backed
- language/API behavior is unchanged (`std.map` surface stays the same)

This reduces allocation/free churn for string-heavy workloads (headers, config keys/values, JSON field names/values).

## Scope

Current SSO scope is runtime map entry storage (`AicMapEntryStorage`) used by:

- `Map[String, String]`
- `Map[Int, String]`
- `Map[Bool, String]`
- HTTP/query/header map helpers that route through runtime map APIs

## Runtime Controls

- Default: SSO enabled.
- Benchmark/debug toggle: set `AIC_RT_DISABLE_MAP_SSO=1` to force heap-only path.

This toggle is intended for A/B benchmarking and diagnostics, not for normal operation.

## Example

Runnable workload example:

- `examples/core/sso_map_workload.aic`

It exercises repeated `map.insert` updates with both a 23-byte and 24-byte value and validates output.

## Benchmark Recipe

Build once:

```bash
cargo run --quiet --bin aic -- build examples/core/sso_map_workload.aic -O2 -o /tmp/aic_sso_map_workload
```

Run with SSO enabled (default):

```bash
AIC_SSO_BENCH_ITERS=3000000 /usr/bin/time -p /tmp/aic_sso_map_workload
```

Run with SSO disabled:

```bash
AIC_SSO_BENCH_ITERS=3000000 AIC_RT_DISABLE_MAP_SSO=1 /usr/bin/time -p /tmp/aic_sso_map_workload
```

Compare `real` time. On short-string-heavy loops, enabled mode should be faster due to fewer heap operations.
