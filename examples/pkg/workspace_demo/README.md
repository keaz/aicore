# Workspace Demo (PKG-T5)

This example demonstrates workspace package graphs with deterministic build order and a shared lockfile.

## Layout

- `aic.workspace.toml` defines workspace members
- `packages/util` is a leaf package
- `packages/app` depends on `packages/util`
- `packages/tool` is independent and builds first by lexical tie-break

## Commands

```bash
aic check examples/pkg/workspace_demo
aic lock examples/pkg/workspace_demo
aic build examples/pkg/workspace_demo
```

## Expected behavior

- Build order is deterministic and topological (`tool_pkg`, `util_pkg`, `app_pkg`)
- Shared lockfile is written at `examples/pkg/workspace_demo/aic.lock`
- Workspace build artifacts are emitted under `target/workspace/<package>/` as libraries (for example `libmain.a`)
- Repeated workspace builds without source changes report `up-to-date`
