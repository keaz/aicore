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

## Native Dependency Bridge (PKG-T3)

AICore supports MVP C-ABI interop through explicit `extern` declarations and manifest-native linkage settings.

### Source declaration rules

```aic
extern "C" fn zlibCompileFlags() -> Int;

fn zlib_flags() -> Int {
    unsafe { zlibCompileFlags() }
}
```

Rules enforced by diagnostics:

- only `extern "C"` is supported (`E2120`, backend guard `E5024`)
- extern signatures must be plain declarations (no async/generics/effects/contracts) (`E2121`)
- currently supported raw C ABI types are `Int`, `Bool`, and `()` (`E2123`)
- calls to `extern` (and `unsafe fn`) require an explicit unsafe boundary (`unsafe { ... }` or `unsafe fn`) (`E2122`)

### Manifest linkage

Add a `[native]` table in `aic.toml`:

```toml
[native]
libs = ["z"]
search_paths = ["native/lib"]
objects = ["native/startup.o"]
```

Link semantics:

- `libs`: emitted as `-l<name>` during native link.
- `search_paths`: emitted as `-L<path>`; relative paths are resolved from the project root.
- `objects`: extra object files passed directly to the linker; relative paths are resolved from the project root.

Example source file: `examples/pkg/ffi_zlib.aic`

## Registry Provenance And Trust Policy (PKG-T4)

Package index entries now carry checksum plus optional signature metadata.

Publish signing behavior:

- if `AIC_PKG_SIGNING_KEY` is set during `aic pkg publish`, release metadata includes:
  - `signature` (HMAC-SHA256 over package/version/checksum payload)
  - `signature_alg` (`hmac-sha256`)
  - `signature_key_id` (from `AIC_PKG_SIGNING_KEY_ID`, default `default`)
- if no signing key is set, release is published unsigned.

Install verification behavior:

- checksums are always verified against index metadata.
- signatures are verified when present.
- trust policy can require signatures and enforce allow/deny rules.

Trust policy configuration lives inside each registry entry in `aic.registry.json`:

```json
{
  "default": "local",
  "registries": {
    "local": {
      "path": "/path/to/registry",
      "trust": {
        "default": "deny",
        "allow": ["corp/*"],
        "deny": ["corp/legacy-*"],
        "require_signed": true,
        "require_signed_for": ["corp/*"],
        "trusted_keys": {
          "corp": "AIC_TRUSTED_CORP_KEY"
        }
      }
    }
  }
}
```

Trust diagnostics:

- `E2119`: trust policy denied package (deny rule/default deny/signature required).
- `E2124`: signature verification or trusted-key configuration failure.

Install auditability:

- `aic pkg install ... --json` includes an `audit` array with per-package policy decisions and signature/checksum verification status.

Example project:

- `examples/pkg/policy_enforced_project/`

## Monorepo Workspace Support (PKG-T5)

AICore supports deterministic multi-package workspaces via a root manifest:

```toml
[workspace]
members = ["packages/app", "packages/util", "packages/tool"]
```

Workspace behavior:

- Build/check walks workspace members in deterministic topological order.
- Package cycles are rejected with a clear cycle diagnostic (`E2126`).
- A single shared lockfile is generated at the workspace root: `aic.lock`.
- Member package dependency contexts resolve through the shared lockfile metadata.

Workspace commands:

```bash
aic check examples/pkg/workspace_demo
aic lock examples/pkg/workspace_demo
aic build examples/pkg/workspace_demo
```

Build output:

- Workspace builds write artifacts to `target/workspace/<package-name>/` (default workspace build emits library artifacts).
- Repeated builds use deterministic fingerprints and print `up-to-date` for unchanged members.

Example workspace:

- `examples/pkg/workspace_demo/`

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
