# CLI Contract (E7)

AICore CLI command/flag behavior is versioned by the contract version emitted by:

```bash
aic contract --json
```

Current contract version: `1.0`

## Exit codes

- `0`: success
- `1`: diagnostic/runtime failure (compile/type/effect/contracts/codegen or harness assertion failure)
- `2`: command-line usage error (argument parsing/conflicting flags)
- `3`: internal/tooling failure (unexpected IO/process/runtime error)

## Stable commands

- `aic init`
- `aic check`
- `aic diag`
- `aic explain`
- `aic fmt`
- `aic ir`
- `aic ir-migrate`
- `aic lock`
- `aic pkg` (`publish`, `install`, `search`)
- `aic build`
- `aic doc`
- `aic std-compat`
- `aic lsp`
- `aic test`
- `aic contract`
- `aic release`
- `aic run`

Stable `run` flags include:

- `--offline`
- `--sandbox` (`none|ci|strict`)

## Diagnostics output modes

`aic check` and `aic diag` expose stable output modes:

- text (default)
- `--json` (conforms to `docs/diagnostics.schema.json`)
- `--sarif` (SARIF 2.1.0 structure)

`--json` and `--sarif` are mutually exclusive.

## Breaking-change policy

Any command/flag/output shape changes require a contract version bump and migration notes in docs.
