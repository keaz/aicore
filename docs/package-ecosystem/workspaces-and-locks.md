# Workspaces And Lockfiles

## Workspace manifest

```toml
# aic.workspace.toml
[workspace]
members = ["packages/app", "packages/tool", "packages/util"]
```

Reference workspace: `examples/pkg/workspace_demo/`.

## Deterministic workspace commands

```bash
aic check examples/pkg/workspace_demo
aic lock examples/pkg/workspace_demo
aic build examples/pkg/workspace_demo
```

Behavior guarantees:

- deterministic topological package ordering
- cycle rejection with explicit path (`E2126`)
- shared lockfile at workspace root (`aic.lock`)
- member dependency resolution scoped by workspace lock metadata
- repeated builds without changes print `up-to-date`

## Lockfile semantics

`aic.lock` entries include:

- dependency package name
- resolved path
- dependency checksum (`sha256:...`)
- resolved version (when dependency metadata is available)
- source provenance (when dependency metadata is available)
- workspace metadata (when generated from workspace)

Schema compatibility:

- schema version `1` lockfiles remain readable.
- new writes use schema version `2` with traceability metadata fields.

Offline mode:

```bash
aic check examples/pkg/workspace_demo --offline
```

Offline lock/cache diagnostics:

- `E2108`: lockfile or cache entry missing
- `E2109`: cache checksum mismatch
