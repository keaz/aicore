# Package Workflow (E6)

AICore package workflow is manifest-driven with deterministic lock/checksum behavior.

## Files

- Manifest: `aic.toml`
- Lockfile: `aic.lock`
- Offline cache root: `.aic-cache/`

## Manifest

Minimal shape:

```toml
[package]
name = "my_app"
main = "src/main.aic"

[dependencies]
util = { path = "deps/util" }
```

Supported dependency value forms:

- inline table: `{ path = "..." }`
- quoted shorthand: `dep = "path/to/dep"`

## Lockfile

Generated with:

```bash
aic lock <project-dir>
```

`aic.lock` stores deterministic dependency entries:

- name
- resolved path
- package checksum (`sha256:...`)

## Registry CLI (PKG-T1)

Package lifecycle commands:

```bash
aic pkg publish <project-dir> [--registry <path>]
aic pkg search <query> [--registry <path>]
aic pkg install <name@requirement>... [--path <project-dir>] [--registry <path>]
```

Version requirement forms:

- wildcard: `*`
- exact: `1.2.3` or `=1.2.3`
- caret: `^1.2.0`
- tilde: `~1.2.3`
- comparator sets: `>=1.0.0,<2.0.0`

Install writes dependencies to `aic.toml` (`[dependencies]`) and regenerates `aic.lock`.
Resolver behavior is deterministic: matching versions are sorted by semantic version and the highest compatible version is selected.

Example consumer source: `examples/pkg/consume_http_client.aic`

## Build/Check Integration

When `aic.lock` exists, frontend package loading verifies dependency checksums before typechecking/codegen.

Drift diagnostic:

- `E2106`: lockfile drift between `aic.toml` and `aic.lock`

Checksum diagnostic:

- `E2107`: dependency source/checksum mismatch

## Offline Mode

Use `--offline` with `aic check`, `aic build`, `aic run`, `aic ir`, or `aic diag`.

Offline behavior:

- resolves dependencies from `.aic-cache/`
- validates cached checksum against lock entry

Offline diagnostics:

- `E2108`: missing lockfile or cache entry
- `E2109`: corrupted cache entry checksum

## Recommended Flow

1. Update dependencies in `aic.toml`.
2. Run `aic lock` online.
3. Commit both `aic.toml` and `aic.lock`.
4. Use `--offline` for reproducible no-network builds.
