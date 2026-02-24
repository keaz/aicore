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
- `aic impact`
- `aic suggest-effects`
- `aic suggest-contracts`
- `aic metrics`
- `aic ir-migrate`
- `aic migrate`
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
- `--sandbox-config` (JSON policy file path)

Stable `pkg` flags include:

- `--registry` (alias or path)
- `--registry-config` (JSON registry settings file)
- `--token` (auth token for private registries)

Stable `build` flags include:

- `--artifact` (`exe|obj|lib`)
- `--debug-info`
- `--verify-hash <sha256>`
- `--manifest <path>` (defaults to `build.json` for single-target builds)

Workspace note:

- `aic build <workspace-root>` keeps existing workspace artifact behavior.
- `--verify-hash` and `--manifest` are rejected for workspace-mode builds; invoke `aic build` on a specific member entry path for hermetic manifest output.

## `aic impact` JSON output

Usage:

```bash
aic impact <function> [input]
```

Output keys:

- `function`
- `direct_callers`
- `transitive_callers`
- `affected_tests`
- `affected_contracts`
- `blast_radius` (`small|medium|large`)

`affected_tests` can be empty; when callers are present, this indicates an untested impact zone.

## `aic suggest-effects` JSON output

Usage:

```bash
aic suggest-effects <input>
```

Per-suggestion fields (deterministic ordering by function name):

- `function`
- `current_effects`
- `required_effects`
- `missing_effects`
- `reason` (effect-to-call-chain mapping, for example `"io": "top -> middle -> leaf"`)

Exit behavior:

- returns `0` when no diagnostics errors exist for the input
- returns `1` when diagnostics include errors (including missing effect declarations)

## `aic suggest-contracts` output modes

Usage:

```bash
aic suggest-contracts <input>
aic suggest-contracts <input> --json
```

JSON payload:

- `suggestions[]` (deterministic ordering by function name)
- `function`
- `suggested_requires[]`
  - `expr`
  - `confidence`
  - `reason`
- `suggested_ensures[]`
  - `expr`
  - `confidence`
  - `reason`

Contract inference scope:

- precondition suggestions come from guard/comparison/assertion-style usage patterns
- postcondition suggestions come from deterministic return expression patterns where feasible
- confidence scores are bounded in `[0.0, 1.0]`

Text mode:

- default mode (without `--json`) is human-readable and grouped by function

## `aic metrics` JSON output

Usage:

```bash
aic metrics <file>
aic metrics <file> --check --max-cyclomatic 15
```

Per-function fields (deterministic ordering by function name):

- `name`
- `cyclomatic_complexity`
- `cognitive_complexity`
- `lines`
- `params`
- `effects`
- `max_nesting_depth`
- `rating`

Check mode:

- `--check` enables threshold gating.
- Thresholds are loaded from nearest `aic.toml` `[metrics]` section.
- `--max-cyclomatic` overrides configured `max_cyclomatic`.
- Exit code is non-zero when any threshold violation is present.

## Diagnostics output modes

`aic check` and `aic diag` expose stable output modes:

- text (default)
- `--json` (conforms to `docs/diagnostics.schema.json`)
- `--sarif` (SARIF 2.1.0 structure)
- `--warn-unused` (opt-in warnings for unused imports, unreachable/unused functions, and unused variables)
- `aic check --show-holes` emits typed-hole inference JSON:
  - `{"holes":[{"line":<line>,"inferred":"<type>","context":"..."}]}`

`--json`, `--sarif`, and `--show-holes` are mutually exclusive for `aic check`.

Autofix API:

```bash
aic diag apply-fixes <file-or-workspace> --dry-run --json
aic diag apply-fixes <file-or-workspace> --json
aic diag apply-fixes <file-or-workspace> --warn-unused --dry-run --json
aic diag apply-fixes <file-or-workspace> --warn-unused --json
```

- Dry-run mode computes deterministic edit plans without writing files.
- Apply mode writes only non-conflicting safe edits.
- Conflicts are reported in `conflicts[]` and produce non-zero exit.
- `--warn-unused` extends fix planning with unused-import and unused-variable safe edits.
- Missing declared effects diagnostics (`E2001`, `E2005`) include deterministic suggested fixes that add/update function `effects { ... }` declarations.

Incremental daemon API:

```bash
aic daemon
```

- Protocol: line-delimited JSON-RPC 2.0 over stdio.
- Methods: `check`, `build`, `stats`, `shutdown`.
- Reference: `docs/agent-tooling/incremental-daemon.md`.

Agent cookbook references:

- `docs/agent-recipes/feature-loop.md`
- `docs/agent-recipes/bugfix-loop.md`
- `docs/agent-recipes/refactor-loop.md`
- `docs/agent-recipes/diagnostics-loop.md`

Agent tooling references:

- `docs/agent-tooling/README.md`
- `docs/agent-tooling/protocol-v1.md`
- `examples/agent/lsp_workflow.json`

## Breaking-change policy

Any command/flag/output shape changes require a contract version bump and migration notes in docs.
