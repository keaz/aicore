# CLI Contract (E7)

AICore CLI command/flag behavior is versioned by the contract version emitted by:

```bash
aic contract --json
```

Current contract version: `1.0`

Agent JSON protocol negotiation:

```bash
aic contract --json --accept-version 1.2,1.0
```

Published parse/check/build/fix schemas:

- `docs/agent-tooling/schemas/parse-response.schema.json`
- `docs/agent-tooling/schemas/check-response.schema.json`
- `docs/agent-tooling/schemas/build-response.schema.json`
- `docs/agent-tooling/schemas/fix-response.schema.json`

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
- `aic daemon`
- `aic test`
- `aic contract`
- `aic release`
- `aic run`

Stable `run` flags include:

- `--offline`
- `--sandbox` (`none|ci|strict`)

Stable `pkg` flags include:

- `--registry` (alias or path)
- `--registry-config` (JSON registry settings file)
- `--token` (auth token for private registries)

## Diagnostics output modes

`aic check` and `aic diag` expose stable output modes:

- text (default)
- `--json` (conforms to `docs/diagnostics.schema.json`)
- `--sarif` (SARIF 2.1.0 structure)

`--json` and `--sarif` are mutually exclusive.

Autofix API:

```bash
aic diag apply-fixes <file-or-workspace> --dry-run --json
aic diag apply-fixes <file-or-workspace> --json
```

- Dry-run mode computes deterministic edit plans without writing files.
- Apply mode writes only non-conflicting safe edits.
- Conflicts are reported in `conflicts[]` and produce non-zero exit.

Incremental daemon API:

```bash
aic daemon
```

- Protocol: line-delimited JSON-RPC 2.0 over stdio.
- Methods: `check`, `build`, `stats`, `shutdown`.
- Reference: `docs/agent-tooling/incremental-daemon.md`.

## Breaking-change policy

Any command/flag/output shape changes require a contract version bump and migration notes in docs.
