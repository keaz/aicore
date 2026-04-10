# IO Agent Guide

This guide is for autonomous agents implementing AIC programs that touch IO runtime modules.
It is intentionally deterministic and aligned with current runtime/codegen behavior.

## 1. Preflight

Before changing IO code:

1. Confirm target APIs in `docs/io-api-reference.md`.
2. Confirm effect requirements for all edited functions.
3. Confirm platform caveats for `std.proc`, `std.net`, `std.signal`, and `std.tls`.
4. Confirm TLS policy contract in `docs/security-ops/tls-policy.v1.json`.
5. Confirm unified secure error contract in `docs/errors/secure-networking-error-contract.v1.json`.
6. Confirm Postgres TLS/SCRAM deterministic replay contract in `docs/security-ops/postgres-tls-scram-replay.v1.json`.

## 2. Effect-First Authoring

Declare effects before writing body logic.

- `std.io` => `effects { io }`
- `std.fs` => `effects { fs }`
- `std.env` => `effects { env }` (and `fs` for cwd/home/temp_dir)
- `std.proc` => `effects { proc, env }` for `run/spawn/pipe/...`
- `std.net` => `effects { net }`
- `std.http_server` => `effects { net }`
- `std.router` => no effects
- `std.config` => `effects { fs }` / `effects { env }` depending on helper
- `std.tls` => `effects { net }`
- `std.time` => `effects { time }`
- `std.rand` => `effects { rand }`

Common diagnostics:

- `E2001`: direct undeclared effect use.
- `E2005`: transitive undeclared effect via call chain.

## 3. Error-Handling Policy

Treat error enums as control-flow boundaries, not exceptions.

- For config/env: treat `NotFound` as optional, other variants as operational errors.
- For filesystem: do not retry `InvalidInput`; retries are only for explicit transient policy.
- For process/network: evaluate `status` and typed errors independently.
- For time parsing: branch by `TimeError` to separate format defects from data defects.
- For secure networking (`std.buffer`, `std.crypto`, `std.tls`, pooled flows): normalize to `std.secure_errors` and branch on stable `code/category/retryable` fields.

## 4. Platform Matrix (Current Runtime)

- Linux/macOS:
  - full runtime behavior for documented `std.io/fs/env/path/time/rand`.
  - `std.proc`, `std.net`, `std.http_server`, `std.router`, and `std.concurrent` implementations are active.
- Windows:
  - `aic build --target x86_64-windows` allows non-std `net`/`tls` usage; the backend links `ws2_32` and uses the shared proc/net/concurrency runtime.
  - Windows CI smoke covers `std.proc` lifecycle paths, TCP loopback, async wait failure paths, and deterministic worker-pool behavior.
  - The smoke-backed `std.net` subset is TCP loopback plus async accept/recv wait/cancel/shutdown client-transport flows.
  - UDP, DNS, socket-tuning, peer-address, and shutdown-tuning helpers share the runtime backend but are not yet covered by Windows CI smoke; keep typed `NetError` branches in libraries that target those paths on Windows.
  - Unsupported socket-option paths return `NetError::Io`; invalid-handle/type misuse remains `NetError::InvalidInput`.
  - `std.proc` operations can still surface `ProcError::Io` and `ProcError::UnknownProcess`.
  - `std.tls` is supported on builds that compile the OpenSSL-backed TLS runtime; builds without TLS backend support return typed `TlsError::ProtocolError`.
  - On supported TLS builds, async TLS APIs are fully surfaced, and `tls_async_runtime_pressure` reports occupied-slot depth and configured slot limit rather than a reactor queue.
  - `std.signal` remains unsupported and returns `SignalError::UnsupportedPlatform`.
  - `std.http_server` and `std.router` are synchronous control-plane APIs and are exercised through the current REST examples rather than network mocks.
- Postgres TLS/SCRAM replay reference (`examples/io/postgres_tls_scram_reference.aic`):
  - deterministic and CI-safe (no external network dependency).
  - secure-flow error compatibility still maps via `PoolErrorContract` when normalizing into `std.secure_errors`.
- Runtime connection pooling is available via `std.pool` (`new_pool/acquire/release/discard/pool_stats/close_pool`).
- See `docs/io-runtime/connection-pool.md` and `examples/io/connection_pool.aic` for agent-safe callback typing and lifecycle patterns.
- `std.config` is the deterministic file/env composition layer for app bootstrap; use it when request handling needs startup config before serving traffic.

## 5. Resource Lifecycle Rules

- Close every `FileHandle` with `file_close`.
- Close every net handle (`tcp_close`/`udp_close`) on success and error paths.
- Close every `TlsStream` with `tls_close` on success and error paths.
- Use `default_tls_config()` unless a ticket explicitly requires unsafe override mode.
- Unsafe override mode must be explicit via `unsafe_insecure_tls_config(...)` and is auditable via `AIC_TLS_POLICY_UNSAFE` warning output.
- For spawned processes, pair with `wait` or `kill` + `wait` where supported.
- Keep temp files/dirs cleaned up in examples and tests.

## 6. Deterministic Validation

Use these checks in order:

```bash
cargo run --quiet --bin aic -- check <file.aic>
cargo run --quiet --bin aic -- run <file.aic>
cargo run --quiet --bin aic -- explain E2001
cargo run --quiet --bin aic -- std-compat --check --baseline docs/std-api-baseline.json
```

For issue-owned examples in this repository:

```bash
cargo run --quiet --bin aic -- check examples/io/interactive_greeter.aic
cargo run --quiet --bin aic -- check examples/io/file_processor.aic
cargo run --quiet --bin aic -- check examples/io/log_tee.aic
cargo run --quiet --bin aic -- check examples/io/env_config.aic
cargo run --quiet --bin aic -- check examples/io/subprocess_pipeline.aic
cargo run --quiet --bin aic -- check examples/io/tls_connect.aic
cargo run --quiet --bin aic -- check examples/io/tls_policy_defaults.aic
cargo run --quiet --bin aic -- check examples/io/secure_error_contract.aic
cargo run --quiet --bin aic -- check examples/io/postgres_tls_scram_reference.aic
cargo run --quiet --bin aic -- check examples/io/connection_pool.aic
cargo run --quiet --bin aic -- check examples/io/http_router.aic
cargo run --quiet --bin aic -- check examples/io/config_loading.aic
cargo run --quiet --bin aic -- run examples/io/postgres_tls_scram_reference.aic
cargo run --quiet --bin aic -- run examples/io/connection_pool.aic
cargo run --quiet --bin aic -- run examples/io/postgres_tls_scram_reference.aic -- bad-cert
cargo run --quiet --bin aic -- run examples/io/postgres_tls_scram_reference.aic -- auth-failure
cargo run --quiet --bin aic -- run examples/io/postgres_tls_scram_reference.aic -- timeout
cargo run --quiet --bin aic -- run examples/io/postgres_tls_scram_reference.aic -- pool-exhausted
```

## 7. Upgrade Hygiene

When `std/*.aic` changes (for example #122 API work):

1. Regenerate baseline:
   - `cargo run --quiet --bin aic -- std-compat > docs/std-api-baseline.json`
2. Re-run docs/examples checks.
3. Update signatures in `docs/io-api-reference.md` and recipe references as needed.
