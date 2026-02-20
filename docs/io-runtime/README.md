# IO Runtime Guide (IO-T6)

This guide is the agent-facing entrypoint for the complete IO runtime surface in AICore.

Use this when building CLI tools, network services, scheduled jobs, and concurrent workers.

## Capability Matrix

| Module | Effects | Primary capabilities | Typical failure model |
|---|---|---|---|
| `std.fs` | `fs` | file read/write/copy/move/delete, metadata, temp paths | `FsError` |
| `std.env` | `env` (+`fs` for cwd) | env get/set/remove, working directory | `EnvError` |
| `std.path` | none | path join/base/dir/ext/absolute checks | pure return values |
| `std.proc` | `proc`, `env` | spawn/wait/kill, run, pipe | `ProcError` + exit status |
| `std.net` | `net` | TCP/UDP sockets, DNS lookup/reverse | `NetError` |
| `std.time` | `time` | wall/monotonic clocks, sleep/deadline helpers | deterministic primitives |
| `std.rand` | `rand` | deterministic seeding, random int/range/bool | deterministic when seeded |
| `std.concurrent` | `concurrency` | tasks, typed channel, int mutex | `ConcurrencyError` |

## Effect Boundaries

- Every side-effecting API requires explicit `effects { ... }` declarations.
- Pure functions cannot call IO runtime modules.
- Transitive effects are enforced: if `A -> B -> C`, callers of `A` must declare effects needed by `C`.

## Platform Caveats

- Linux/macOS: full runtime support for fs/env/path/proc/net/time/rand/concurrency.
- Windows: process/network/concurrency runtime paths currently return stable `Io`/unsupported-style errors via enum mapping; code should branch on typed errors instead of assuming success.

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
- CLI file pipeline: `examples/io/cli_file_pipeline.aic`
- Subprocess orchestration: `examples/io/process_pipeline.aic`
- Networking TCP loopback: `examples/io/tcp_echo.aic`
- Timer/backoff pattern: `examples/io/retry_with_jitter.aic`
- Concurrency worker pool: `examples/io/worker_pool.aic`
- Negative effect-enforcement example: `examples/io/effect_misuse_fs.aic` (expected check failure)

Run all IO examples:

```bash
cargo run --quiet --bin aic -- run examples/io/fs_all_ops.aic
cargo run --quiet --bin aic -- run examples/io/cli_file_pipeline.aic
cargo run --quiet --bin aic -- run examples/io/process_pipeline.aic
cargo run --quiet --bin aic -- run examples/io/tcp_echo.aic
cargo run --quiet --bin aic -- run examples/io/retry_with_jitter.aic
cargo run --quiet --bin aic -- run examples/io/worker_pool.aic
```

Expected final line for each: `42`.

## Deep-Dive Docs

- Network/time/rand API contract: `docs/io-runtime/net-time-rand.md`
- Error model + remediation playbook: `docs/io-runtime/error-model.md`
- Resource lifecycle + long-running service guidance: `docs/io-runtime/lifecycle-playbook.md`
- FS API contract: `docs/io-filesystem.md`
- Proc/env/path contract: `docs/io-process-env-path.md`
- Concurrency runtime contract: `docs/io-concurrency-runtime.md`
