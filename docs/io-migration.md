# IO Migration Guide
This guide covers migration to the current IO runtime surface in `std.io`, `std.fs`, `std.env`, `std.path`, `std.proc`, `std.net`, `std.time`, and `std.rand`.

## Baseline Step

Capture the current standard-library snapshot before and after migration work:

```bash
cargo run --quiet --bin aic -- std-compat > docs/std-api-baseline.json
cargo run --quiet --bin aic -- std-compat --check --baseline docs/std-api-baseline.json
```

## Intrinsic verification gate

Before merging runtime-bound std changes, validate intrinsic declarations:

```bash
cargo run --quiet --bin aic -- verify-intrinsics std --json
```

This check fails for missing lowering mappings, signature drift, or unsupported intrinsic ABI metadata.

Guard against reintroducing source-level intrinsic stubs in AGX1 policy modules:

```bash
make intrinsic-placeholder-guard
```


See `docs/intrinsics-runtime-bindings.md` for declaration rules, side-effect boundaries, and troubleshooting examples.

## API Migration Highlights

### 1. `std.io`: richer interactive and stream APIs

Current public APIs include:

- `read_line`, `read_int`, `read_char`, `prompt`
- `eprint_str`, `eprint_int`
- `println_str`, `println_int`, `print_bool`, `println_bool`
- `flush_stdout`, `flush_stderr`

Migration action:

- replace ad hoc input wrappers with direct `Result[..., IoError]` matching.

### 2. `std.fs`: file-handle and bytes APIs

Current additions include:

- bytes APIs: `read_bytes`, `write_bytes`, `append_bytes`
- handle APIs: `open_read`, `open_write`, `open_append`, `file_read_line`, `file_write_str`, `file_close`
- directory/symlink/permissions APIs: `mkdir`, `mkdir_all`, `rmdir`, `list_dir`, `create_symlink`, `read_symlink`, `set_readonly`

Migration action:

- use handle APIs for multi-step file workflows.
- use `list_dir` for concrete entries; use `walk_dir` for count-oriented workflows.

### 3. `std.env`: process metadata and argv/env snapshots

Current additions include:

- `args`, `arg_count`, `arg_at`
- `all_vars`
- `home_dir`, `temp_dir`, `os_name`, `arch`
- `exit`

Migration action:

- move process-boundary config parsing into explicit env helpers.

### 4. `std.proc`: controlled execution APIs

Current additions include:

- `run_with(command, RunOptions)`
- `run_timeout(command, timeout_ms)`
- `pipe_chain(stages)`
- `is_running(handle)`, `current_pid()`

Migration action:

- migrate one-off shell wrappers to `run`/`pipe`.
- migrate workflow runners to `run_with` and typed `ProcOutput` handling.

### 5. `std.time`: parse/format DateTime

Current additions include:

- `parse_rfc3339`, `parse_iso8601`
- `format_rfc3339`, `format_iso8601`

Deprecation:

- `std.time.now` is compatibility API; prefer `std.time.now_ms`.
- deprecated usage emits `E6001` warning.

### 6. `std.net` and platform caveat

Current `std.net` signatures are stable, but runtime support differs by platform.

Migration action:

- when targeting Windows, treat TCP loopback plus async accept/recv wait/cancel/shutdown as the smoke-backed `std.net` subset.
- keep typed fallback branches around UDP/DNS/socket-tuning paths until your library validates them on Windows, and always branch `NetError::Io` separately from `InvalidInput` / `ConnectionClosed`.

## Diagnostic Migration Map

| Code | Migration meaning | Typical fix |
|---|---|---|
| `E2001` | direct undeclared effect use | declare missing effects on function |
| `E2005` | transitive undeclared effect path | declare effect at call-chain root |
| `E6001` | deprecated std API usage | move to replacement API (`std.time.now_ms`) |
| `E6002` | std API compatibility check failure | update baseline only for intentional additive API changes |

## Example Migration Targets

Use these maintained examples as migration anchors:

- `examples/io/interactive_greeter.aic`
- `examples/io/file_processor.aic`
- `examples/io/log_tee.aic`
- `examples/io/env_config.aic`
- `examples/io/subprocess_pipeline.aic`

## Migration Checklist

1. Align function signatures with `docs/io-api-reference.md`.
2. Add missing effects and re-run typecheck.
3. Add typed `Result` handling for all new fallible APIs.
4. Update examples to compile (`aic check`) and run where deterministic.
5. Re-run docs/examples checks.
