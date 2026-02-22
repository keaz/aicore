# IO Runtime Error Model And Remediation

This document maps compile-time diagnostics and runtime errors to practical remediation actions for agents.

## Compile-Time Diagnostics (Structured)

Use `aic check --json` for machine-readable diagnostics.

| Code | Meaning | Typical cause | Suggested remediation |
|---|---|---|---|
| `E2001` | undeclared effect usage | called effectful API from function missing `effects { ... }` | add required effects on the enclosing function |
| `E2003` | unknown effect declaration | typo in effect name | use known taxonomy: `io, fs, net, time, rand, env, proc, concurrency` |
| `E2004` | duplicate effect declaration | same effect listed twice | remove duplicates; canonical order is auto-normalized |
| `E2005` | transitive effect path missing | indirect calls reach effectful leaf | add effect on top-level caller or split pure/effectful paths |
| `E2006` | resource protocol violation | operation on closed/consumed concurrent handle | reorder lifecycle calls or allocate a fresh handle before reuse |
| `E2100` | missing imported module | module not available/imported | add valid `import` and ensure module exists |
| `E2102` | symbol requires explicit import | symbol reachable only through module import | add explicit `import module.path;` |

## Runtime Error Enums

### `FsError`

- `NotFound`: path missing
- `PermissionDenied`: ACL/ownership violation
- `AlreadyExists`: destination collision
- `InvalidInput`: invalid path/arguments
- `Io`: other host IO failures

Remediation:

- validate paths with `exists` and `metadata`
- create unique temp names for writes/copies
- retry only on transient `Io`; fail fast on `InvalidInput`

### `EnvError`

- `NotFound`, `PermissionDenied`, `InvalidInput`, `Io`

Remediation:

- branch on missing optional variables vs required ones
- avoid mutating process env in pure library code; keep env access at app boundary

### `ProcError`

- `NotFound`, `PermissionDenied`, `InvalidInput`, `Io`, `UnknownProcess`

Remediation:

- validate command strings before spawn/run
- always `wait` spawned handles
- treat `UnknownProcess` as idempotent cleanup case for kill/wait flows

### `NetError`

- `NotFound`, `PermissionDenied`, `Refused`, `Timeout`, `AddressInUse`, `InvalidInput`, `Io`

Remediation:

- retry with backoff for `Timeout`/`Refused`
- rotate address/port for `AddressInUse`
- fail fast for `InvalidInput`

### `ConcurrencyError`

- `NotFound`, `Timeout`, `Cancelled`, `InvalidInput`, `Panic`, `Closed`, `Io`

Remediation:

- treat `Cancelled` and `Closed` as expected control flow outcomes
- reserve retries for `Timeout`/transient `Io`
- treat `Panic` as task failure boundary and isolate/restart worker units

## Retry/Backoff Pattern

Use deterministic jittered retries with `std.time` + `std.rand`.

```aic
import std.time;
import std.rand;

fn next_delay(base: Int, attempt: Int) -> Int effects { rand } {
    let jitter = random_range(0, 5);
    base + attempt * 5 + jitter
}

fn wait_retry(base: Int, attempt: Int) -> () effects { time, rand } {
    sleep_ms(next_delay(base, attempt));
    ()
}
```

## Diagnostics + Runtime Correlation

- Compile-time diagnostics catch API misuse and missing effects before execution.
- Runtime enums capture host/runtime outcomes after effects are allowed.
- Agent policy should prioritize fixing diagnostics first, then applying runtime fallback branches.

## Negative Example (CI)

- `examples/io/effect_misuse_fs.aic` intentionally triggers `E2001` to validate effect enforcement.
- `examples/verify/file_protocol_invalid.aic` intentionally triggers `E2006` to validate protocol enforcement.
- `examples/e5/panic_line_map.aic` validates runtime panic surfacing and panic-location diagnostics.
