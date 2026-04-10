# IO Runtime Guide (IO-T6)

This guide is the agent-facing entrypoint for the complete IO runtime surface in AICore.

Use this when building CLI tools, network services, scheduled jobs, and concurrent workers.

## Capability Matrix

| Module | Effects | Primary capabilities | Typical failure model |
|---|---|---|---|
| `std.fs` | `fs` | text/byte read-write, file handles, directories, symlinks, readonly flags, metadata, temp paths | `FsError` |
| `std.env` | `env` (+`fs` for cwd/home/temp_dir) | env get/set/remove, args snapshot, host metadata, working directory | `EnvError` |
| `std.path` | none | path join/base/dir/ext/absolute checks | pure return values |
| `std.proc` | `proc`, `env` | spawn/wait/kill/is_running/current_pid, run/run_with/run_timeout, pipe/pipe_chain | `ProcError` + exit status |
| `std.net` | `net` | TCP/UDP sockets, DNS lookup/reverse | `NetError` |
| `std.time` | `time` | wall/monotonic clocks, sleep/deadline helpers | deterministic primitives |
| `std.signal` | `proc` | register SIGINT/SIGTERM/SIGHUP handlers, block for shutdown signal | `SignalError` |
| `std.rand` | `rand` | deterministic seeding, random int/range/bool | deterministic when seeded |
| `std.retry` | `time`, `rand` | retry/backoff policy and timeout wrappers | `RetryResult[T]` and timeout `Result[T, String]` |
| `std.concurrent` | `concurrency` | tasks, structured group/select/timeout helpers, generic `Sender[T]/Receiver[T]` channels, legacy int channel compatibility, int mutex | `ConcurrencyError`, `ChannelError` |
| `std.pool` | `concurrency` | generic resource pooling with health checks, idle/lifetime recycling, lifecycle stats | `PoolError` |
| `std.http_server` | `net` | synchronous request/response server APIs, text/json response helpers | `ServerError` |
| `std.router` | none | deterministic route registration and matching | `RouterError` |
| `std.config` | `fs`, `env` | file/env config composition for startup loading | `ConfigError` |

## Effect Boundaries

- Every side-effecting API requires explicit `effects { ... }` declarations.
- Pure functions cannot call IO runtime modules.
- Transitive effects are enforced: if `A -> B -> C`, callers of `A` must declare effects needed by `C`.

## External Client Libraries

- Core runtime modules stay protocol-agnostic and expose generic transport/binary primitives.
- Protocol implementations (PostgreSQL/Kafka/Redis/etc.) should be built as external libraries on top of these primitives.

## Resource Lifecycle (RAII Subset)

- Compiler-managed resource locals (`FileHandle`, `Map[K, V]`, `Set[T]`, `TcpReader`, `IntChannel`, `IntMutex`) are automatically cleaned up on scope exit in deterministic reverse lexical order.
- Cleanup also runs on early-exit control flow (`return`, `break`, `continue`, and `?` error propagation).
- Direct local move-outs (`let b = a`, direct `return a`, direct tail `a`) preserve transferred ownership by suppressing cleanup on the moved-from local.
- Concrete `Drop` trait implementations (`trait Drop[T] { fn drop(self: T) -> (); }`) are dispatched at scope exits before builtin cleanup fallback, with the same reverse-lexical ordering and move-out suppression rules.

## Platform Caveats

- Linux/macOS: full runtime support for fs/env/path/proc/net/time/rand/retry/concurrency.
- Linux/macOS: `std.signal` supports SIGINT/SIGTERM/SIGHUP registration + blocking waits.
- Windows: `std.proc`, `std.net`, and `std.concurrent` use the shared runtime backend and are validated by Windows CI smoke coverage for proc lifecycle, TCP loopback, async wait failure paths, deterministic worker-pool behavior, and Windows-target `net` lowering smoke.
- Windows `std.net` supported substrate: TCP loopback (`tcp_listen`/`tcp_local_addr`/`tcp_connect`/`tcp_accept`/`tcp_send`/`tcp_recv`/`tcp_close`) plus async accept/recv wait/cancel/shutdown client-transport flows.
- Windows `std.net` partial substrate: UDP, DNS, socket tuning, peer-address, and shutdown-tuning helpers are implemented through the shared runtime backend but are not yet covered by Windows CI smoke; library authors should keep typed `NetError` branches around those paths until broader validation lands.
- Windows `std.net` typed fallback contract: unsupported socket-option paths return `NetError::Io`; invalid-handle/type misuse remains `NetError::InvalidInput`; peer-close still surfaces `NetError::ConnectionClosed`.
- Windows: `std.proc` operations can still surface `ProcError::Io` and `ProcError::UnknownProcess`; branch on typed errors instead of assuming success.
- Windows: `std.tls` remains backend-dependent; when the TLS backend is available, async pressure snapshots report occupied-slot depth and configured slot limit rather than a separate reactor queue.
- Windows and other non-Linux/macOS targets: `std.signal` returns `SignalError::UnsupportedPlatform`.
- `std.http_server` and `std.router` are synchronous control-plane APIs and are exercised through the current REST examples rather than network mocks.

### Windows `std.net` Service-Library Contract

| Capability | Windows status | Evidence / notes |
|---|---|---|
| TCP listen/connect/accept/send/recv/close | Supported | Windows CI smoke: `exec_net_tcp_loopback_echo` |
| Async accept/recv wait/cancel/shutdown transport lifecycle | Supported | Windows CI smoke: `exec_net_async_wait_negative_paths_are_stable` |
| Windows-target `net` lowering/build path | Supported | Windows-target smoke: `build_windows_target_allows_net_effect_usage_without_e6007` |
| UDP bind/send/recv/close | Partial | Shared runtime backend exists; not yet covered by Windows CI smoke |
| DNS lookup/lookup_all/reverse | Partial | Shared runtime backend exists; not yet covered by Windows CI smoke |
| Socket tuning, peer-address, and shutdown-tuning helpers | Partial | Shared runtime backend exists; unsupported socket-option paths return `NetError::Io`, invalid-handle misuse remains `NetError::InvalidInput`; not yet covered by Windows CI smoke |
| Native async REST-server execution paths | Partial | Separate from the low-level transport contract; Windows coverage remains narrower than Linux/macOS |

## Quick-Start Templates

### CLI pipeline skeleton

```aic
import std.io;
import std.fs;
import std.path;
import std.env;

fn main() -> Int effects { io, fs, env } {
    let cwd_path = cwd();
    let _ = cwd_path;
    let out_path = join(".", "out.txt");
    write_text(out_path, "ok");
    print_int(42);
    0
}
```

### HTTP server and router skeleton

```aic
import std.http_server;
import std.router;

fn main() -> Int effects { net } {
    let router0 = match new_router() {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let router1 = match add(router0, "GET", "/health", 1) {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let response = text_response(200, "ok");
    let _ = router1;
    let _ = response;
    0
}
```

### TCP loopback service skeleton

```aic
import std.io;
import std.net;

fn main() -> Int effects { io, net } {
    let listener = tcp_listen("127.0.0.1:0");
    let _ = listener;
    print_int(42);
    0
}
```

## Runnable Examples (CI-covered)

- Filesystem operations: `examples/io/fs_all_ops.aic`
- Byte stream copy and adapter behavior: `examples/io/stream_copy.aic`
- RAII scope-exit and early-return cleanup: `examples/io/raii_file_cleanup.aic`
- `Drop` trait destructor dispatch on scope-exit and `?`: `examples/io/drop_trait_cleanup.aic`
- CLI file pipeline: `examples/io/cli_file_pipeline.aic`
- HTTP request/response construction: `examples/io/http_server_hello.aic`
- Route matching and precedence: `examples/io/http_router.aic`
- Subprocess orchestration: `examples/io/process_pipeline.aic`
- Networking TCP loopback: `examples/io/tcp_echo.aic`
- Async worker-pool loopback: `examples/io/async_net_worker_pool.aic`
- Async submit+await bridge (reactor polling): `examples/io/async_await_submit_bridge.aic`
- Retry/backoff + timeout pattern: `examples/io/retry_with_jitter.aic`
- Graceful shutdown via OS signal: `examples/io/signal_shutdown.aic` (manual signal required)
- Concurrency worker pool: `examples/io/worker_pool.aic`
- Connection pool lifecycle + health/recycle checks: `examples/io/connection_pool.aic`
- Legacy-to-generic channel migration compatibility: `examples/io/channel_migration_compat.aic`
- Generic channel payloads (`String`/`Vec[Int]`/struct): `examples/io/generic_channel_types.aic`
- Generic channel protocol-valid lifecycle: `examples/verify/generic_channel_protocol_ok.aic`
- Generic channel protocol-invalid lifecycle: `examples/verify/generic_channel_protocol_invalid.aic` (expected check failure, `E2006`)
- Structured task group/select/timeout: `examples/io/structured_concurrency.aic`
- Negative effect-enforcement example: `examples/io/effect_misuse_fs.aic` (expected check failure)

Run all IO examples:

```bash
cargo run --quiet --bin aic -- run examples/io/fs_all_ops.aic
cargo run --quiet --bin aic -- run examples/io/cli_file_pipeline.aic
cargo run --quiet --bin aic -- run examples/io/http_server_hello.aic
cargo run --quiet --bin aic -- run examples/io/http_router.aic
cargo run --quiet --bin aic -- run examples/io/process_pipeline.aic
cargo run --quiet --bin aic -- run examples/io/tcp_echo.aic
cargo run --quiet --bin aic -- run examples/io/async_await_submit_bridge.aic
cargo run --quiet --bin aic -- run examples/io/retry_with_jitter.aic
cargo run --quiet --bin aic -- run examples/io/worker_pool.aic
cargo run --quiet --bin aic -- run examples/io/connection_pool.aic
cargo run --quiet --bin aic -- run examples/io/channel_migration_compat.aic
cargo run --quiet --bin aic -- run examples/io/generic_channel_types.aic
cargo run --quiet --bin aic -- run examples/io/structured_concurrency.aic
```

Expected final line for each: `42`.
`examples/io/signal_shutdown.aic` intentionally waits for SIGINT/SIGTERM/SIGHUP and is excluded from the batch commands.

## Deep-Dive Docs

- Network/time/rand/retry API contract: `docs/io-runtime/net-time-rand.md`
- Error model + remediation playbook: `docs/io-runtime/error-model.md`
- Resource lifecycle + long-running service guidance: `docs/io-runtime/lifecycle-playbook.md`
- Connection pool contract + agent-safe usage: `docs/io-runtime/connection-pool.md`
- External protocol integration harness contract: `docs/io-runtime/integration-harness.md`
- FS API contract: `docs/io-filesystem.md`
- Proc/env/path contract: `docs/io-process-env-path.md`
- Concurrency runtime contract: `docs/io-concurrency-runtime.md`
