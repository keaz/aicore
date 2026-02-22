# IO Agent Guide

This guide is for autonomous agents implementing AIC programs that touch IO runtime modules.
It is intentionally deterministic and aligned with current runtime/codegen behavior.

## 1. Preflight

Before changing IO code:

1. Confirm target APIs in `/Users/kasunranasinghe/Projects/Rust/aicore/docs/io-api-reference.md`.
2. Confirm effect requirements for all edited functions.
3. Confirm platform caveats for `std.proc` and `std.net`.

## 2. Effect-First Authoring

Declare effects before writing body logic.

- `std.io` => `effects { io }`
- `std.fs` => `effects { fs }`
- `std.env` => `effects { env }` (and `fs` for cwd/home/temp_dir)
- `std.proc` => `effects { proc, env }` for `run/spawn/pipe/...`
- `std.net` => `effects { net }`
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

## 4. Platform Matrix (Current Runtime)

- Linux/macOS:
  - full runtime behavior for documented `std.io/fs/env/path/time/rand`.
  - `std.proc` and `std.net` implementations are active.
- Windows:
  - `std.net` currently maps to `NetError::Io` (unsupported runtime path).
  - `std.proc` partial behavior:
    - `run`, `pipe`, `current_pid` available.
    - `spawn`, `run_with`, `run_timeout`, `pipe_chain` return `ProcError::Io`.
    - `wait`, `kill`, `is_running` return `ProcError::UnknownProcess`.

## 5. Resource Lifecycle Rules

- Close every `FileHandle` with `file_close`.
- Close every net handle (`tcp_close`/`udp_close`) on success and error paths.
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
```

## 7. Upgrade Hygiene

When `std/*.aic` changes (for example #122 API work):

1. Regenerate baseline:
   - `cargo run --quiet --bin aic -- std-compat > docs/std-api-baseline.json`
2. Re-run docs/examples checks.
3. Update signatures in `/Users/kasunranasinghe/Projects/Rust/aicore/docs/io-api-reference.md` and recipe references as needed.
