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
aic pkg publish <project-dir> [--registry <alias-or-path>] [--registry-config aic.registry.json] [--token ...]
aic pkg search <query> [--registry <alias-or-path>] [--registry-config aic.registry.json] [--token ...]
aic pkg install <name@requirement>... [--path <project-dir>] [--registry <alias-or-path>] [--registry-config aic.registry.json] [--token ...]
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

## Private Registries, Scopes, and Mirrors (PKG-T2)

`aic pkg` can load registry settings from `aic.registry.json` in the project root (or `--registry-config` / `AIC_PKG_REGISTRY_CONFIG`).

Example config:

```json
{
  "default": "public",
  "registries": {
    "public": { "path": "/ci/cache/aic/public" },
    "private": {
      "path": "/ci/cache/aic/private",
      "private": true,
      "token_env": "AIC_PRIVATE_TOKEN",
      "token_file": "/ci/secrets/aic-private.token",
      "mirrors": ["/ci/cache/aic/private-mirror"]
    }
  },
  "scopes": {
    "corp/": "private"
  }
}
```

Rules:

- `default` selects the fallback registry alias.
- `scopes` routes package prefixes to a specific registry alias (longest-prefix match).
- `mirrors` are tried in deterministic order when the primary registry is missing data.
- Private registries require credentials from `--token` or `token_env`; mismatch/missing credentials return deterministic auth diagnostics.

Example private-registry runbook: `examples/pkg/private_registry_readme.md`

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
- package install remains deterministic when configured mirrors are available (no network dependency)

Offline diagnostics:

- `E2108`: missing lockfile or cache entry
- `E2109`: corrupted cache entry checksum

## Recommended Flow

1. Update dependencies in `aic.toml`.
2. Run `aic lock` online.
3. Commit both `aic.toml` and `aic.lock`.
4. Use `--offline` for reproducible no-network builds.
