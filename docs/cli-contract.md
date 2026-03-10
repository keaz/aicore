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

Published parse/check/build/fix/testgen/session/patch/validate/suggest/context/query/symbols schemas:

- `docs/agent-tooling/schemas/parse-response.schema.json`
- `docs/agent-tooling/schemas/check-response.schema.json`
- `docs/agent-tooling/schemas/build-response.schema.json`
- `docs/agent-tooling/schemas/fix-response.schema.json`
- `docs/agent-tooling/schemas/testgen-response.schema.json`
- `docs/agent-tooling/schemas/session-response.schema.json`
- `docs/agent-tooling/schemas/patch-response.schema.json`
- `docs/agent-tooling/schemas/validate-call-response.schema.json`
- `docs/agent-tooling/schemas/validate-type-response.schema.json`
- `docs/agent-tooling/schemas/suggest-response.schema.json`
- `docs/agent-tooling/schemas/context-response.schema.json`
- `docs/agent-tooling/schemas/query-response.schema.json`
- `docs/agent-tooling/schemas/symbols-response.schema.json`

Published patch authoring schema:

- `docs/agent-tooling/schemas/patch-request.schema.json`

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
- `aic validate-call`
- `aic validate-type`
- `aic suggest`
- `aic context`
- `aic query`
- `aic symbols`
- `aic scaffold`
- `aic synthesize`
- `aic testgen`
- `aic checkpoint`
- `aic session`
- `aic patch`
- `aic metrics`
- `aic ir-migrate`
- `aic migrate`
- `aic lock`
- `aic pkg` (`publish`, `install`, `search`)
- `aic build`
- `aic doc`
- `aic std-compat`
- `aic diff`
- `aic lsp`
- `aic debug` (`dap`)
- `aic daemon`
- `aic test`
- `aic contract`
- `aic release`
- `aic run`

Stable `patch` flags include:

- `--preview` (compute deterministic edit plan without filesystem writes)
- `--apply` (write patched files only when no conflicts are present)
- `--project <path>` (symbol/index resolution root)
- `--json` (machine-readable patch response)

Stable `context` flags include:

- `--for <target...>` (target selector; supports `function <name>` or `<module>.<name>`)
- `--depth <N>` (transitive dependency/caller traversal depth)
- `--limit <N>` (truncate ranked `dependencies[]`, `callers[]`, and `related_tests[]` after deterministic ordering)
- `--project <path>` (project root used for symbol index + call graph extraction)
- `--json` (machine-readable context response)

Stable `query` flags include:

- `--project <path>` (project root used for deterministic workspace symbol indexing)
- `--kind <kind>` (`function|struct|enum|variant|trait|impl|module`)
- `--name <pattern>` (exact or wildcard symbol-name filter)
- `--module <pattern>` (exact or wildcard module-name filter)
- `--effects <effect[,effect...]>` (comma-delimited effect filter; all listed effects must be present)
- `--has-contract` (matches symbols with any published contract clause)
- `--has-invariant` (struct-contract filter; rejected with non-struct `--kind`)
- `--generic-over <type_param>` (match symbols declaring the named generic parameter)
- `--has-requires` / `--has-ensures` (function-contract filters; rejected with non-function `--kind`)
- `--limit <N>` (deterministic pagination guard; current maximum is `500`)
- `--json` (machine-readable query response envelope)

Stable `symbols` flags include:

- `--project <path>` (project root used for deterministic workspace symbol indexing)
- `--format <format>` (`text|json`)
- `--json` (machine-readable symbols response envelope; equivalent to `--format json`)

Stable `scaffold` flags include:

- `struct <name> --field <NAME:TYPE>... [--with-invariant <expr>]`
- `struct <name> { name: Type, ... }` (inline trailing field list form)
- `enum <name> --variant <NAME[:TYPE]>...`
- `enum <name> { Variant, Payload(Type), ... }` (inline trailing variant list form)
- `fn <name> --param <NAME:TYPE>... --return <TYPE> [--effect <EFFECT>] [--capability <CAP>] [--requires <expr>] [--ensures <expr>]`
- `match <expr> --arm <PATTERN=>BODY>... [--exhaustive]`
- `test --for <target> [--run-pass] [--compile-fail]`
- `--json` (machine-readable scaffold payload with `kind`, `name`, and `content`)

Stable `synthesize` flags include:

- `--from <kind>` (`spec` in the current implementation)
- `--project <path>` (project root used to discover `spec fn` files and dependent types)
- `--json` (machine-readable synthesis response)

Stable `testgen` flags include:

- `--strategy <kind>` (`boundary|invariant-violation|exhaustive-match|effect-coverage`)
- `--for <target...>` (target selector; supports `function <name>`, `struct <name>`, `enum <name>`)
- `--project <path>` (project root used for symbol discovery and source loading)
- `--emit-dir <path>` (materialize generated fixtures under the provided root using each artifact `path_hint`)
- `--seed <N>` (deterministic value-synthesis seed)
- `--json` (machine-readable test-generation response)

Stable `checkpoint` flags include:

- `create --project <path>` (capture deterministic workspace snapshot rooted at the resolved project path)
- `list --project <path>` (enumerate stored checkpoints in deterministic id order)
- `restore <checkpoint> --project <path>` (validate snapshot integrity and restore checkpointed files)
- `diff <checkpoint> [--to <checkpoint>] --project <path>` (compare checkpoint to workspace or another checkpoint)
- `--json` (machine-readable checkpoint response envelope)

Stable `session` flags include:

- `create --project <path>` (create a deterministic collaboration session rooted at the resolved project path)
- `create --label <name>` (attach a human-readable label to the session)
- `create --now-ms <N>` (override lease/event clock for deterministic automation/testing)
- `list --project <path>` (enumerate recorded sessions and current lock table)
- `list --now-ms <N>` (mark expired locks deterministically during listing)
- `lock acquire <session> --for <target...>` (claim exclusive symbol ownership for an existing session)
- `lock acquire --lease-ms <N>` (lease duration in milliseconds before another session may reclaim an expired lock)
- `lock acquire --operation-id <id>` (associate the lease with a specific planned operation)
- `lock acquire --project <path>`
- `lock acquire --now-ms <N>`
- `lock release <session> --for <target...> --project <path>`
- `lock release --now-ms <N>`
- `conflicts <plan.json> --project <path>` (machine-readable overlap/ownership analysis for patch-backed session plans)
- `merge <plan.json> --project <path>` (validation-only merge of a patch-backed session plan)
- `merge --offline` (validate merge under offline dependency resolution)
- `merge --now-ms <N>`

Stable `validate-call` flags include:

- `--arg <type>` (repeatable argument type list in call-order)
- `--project <path>` (resolved project root used for symbol tables/import aliases)
- `--offline` (disable dependency/network resolution during fast-path load)

Stable `validate-type` flags include:

- `<type_expr>` (type expression to validate against the current project/import context)
- `--project <path>` (resolved project root used for visible type aliases/structs/enums/traits)
- `--offline` (disable dependency/network resolution during fast-path load)

Stable `suggest` flags include:

- `--partial <text>` (partial symbol text to rank against the workspace symbol index)
- `--project <path>` (resolved project root used for symbol index extraction)
- `--limit <N>` (maximum number of ranked candidates returned)

Stable `run` flags include:

- `--offline`
- `--sandbox` (`none|ci|strict`)
- `--sandbox-config` (JSON policy file path)
- `--check-leaks` (debug-mode leak tracking; exits non-zero on leaks)
- `--asan` (compile/run with AddressSanitizer instrumentation)

Environment toggles:

- `AIC_RUN_ASAN=1` enables the same ASan path as `--asan`.
- `AIC_ASAN=1` enables ASan for direct `aic build`/codegen compile paths.

Stable `pkg` flags include:

- `--registry` (alias or path)
- `--registry-config` (JSON registry settings file)
- `--token` (auth token for private registries)

Stable `build` flags include:

- `--artifact` (`exe|obj|lib`)
- `--debug-info`
- `--release` (defaults optimization level to `O2`)
- `--opt-level <LEVEL>` (`0|1|2|3` or `O0|O1|O2|O3`)
- `-O<LEVEL>` shorthand (for example `-O2`)
- `--verify-hash <sha256>`
- `--manifest <path>` (defaults to `build.json` for single-target builds)

Stable `debug` flags include:

- `dap --adapter <path>` (override debug adapter backend; defaults to `lldb-dap`/`lldb-vscode` lookup)

Stable `test` flags include:

- `--mode` (`all|run-pass|compile-fail|golden`)
- `--filter <pattern>`
- `--seed <N>`
- `--replay <id-or-artifact>`
- `--json`
- `--update-golden`
- `--check-golden`

Replay contract:

- Failed `aic test --json` runs include a `replay` object:
  - `replay_id`
  - `artifact_path`
  - `seed`
  - `time_ms`
  - `mock_no_real_io`
  - `mock_io_capture`
  - `trace_id` (optional)
  - `generated_at_ms`
- Replay artifacts are written to `.aic-replay/<replay_id>.json`.
- `aic test --replay <id-or-artifact>` re-runs with captured deterministic context.

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
- `current_capabilities`
- `required_capabilities`
- `missing_capabilities`
- `reason` (effect-to-call-chain mapping, for example `"io": "top -> middle -> leaf"`)
- `capability_reason` (capability-to-call-chain mapping, for example `"io": "top -> middle -> leaf"`)

Exit behavior:

- returns `0` when no diagnostics errors exist for the input
- returns `1` when diagnostics include errors (including missing effect/capability declarations)

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

## `aic validate-call` JSON output

Usage:

```bash
aic validate-call math.add --arg Int --arg Int --project examples/e7/api_conformance
```

JSON payload:

- `schema_version`
- `command` (`validate-call`)
- `ok`
- `fast_path`
- `project_root`
- `target`
- `arg_types[]`
- optional `resolved`
  - `qualified_name`
  - `name`
  - optional `module`
  - `signature`
  - `location` (`file`, `line`, `column`, `span_start`, `span_end`)
  - `arity`
  - `is_async`
  - `is_unsafe`
  - `is_extern`
  - optional `extern_abi`
  - optional `effects[]`, `capabilities[]`, `generics[]`, `generic_bindings`, `requires`, `ensures`
- optional `suggestions[]`
  - `qualified_name`
  - `kind`
  - `match_kind`
  - `distance`
  - `score`
- `diagnostics[]`

Behavior:

- validates callable existence against the resolver/typechecker fast-path without codegen
- normalizes single-segment module aliases (for example `math.add`) through the current entry-module import table
- performance budget is front-end only: parse, resolve, and signature/type compatibility checks; no codegen, execution, artifact writes, or daemon state mutation
- returns `1` on unknown callables, arity mismatch, or argument type mismatch

## `aic validate-type` JSON output

Usage:

```bash
aic validate-type 'Result[User, AppError]' --project examples/e7/api_conformance
```

JSON payload:

- `schema_version`
- `command` (`validate-type`)
- `ok`
- `fast_path`
- `project_root`
- `type_expr`
- optional `canonical`
- optional `kind` (`unit|named|dyn_trait|hole`)
- optional `named_types[]`
- `diagnostics[]`

Behavior:

- validates type-expression syntax and resolver visibility against the current project/import context
- combines parser diagnostics for malformed expressions with fast-path type availability/arity diagnostics
- performance budget is front-end only: parse, resolve, and type-shape validation; no codegen, execution, artifact writes, or daemon state mutation
- returns `1` when any error diagnostics are present

## `aic suggest --partial` JSON output

Usage:

```bash
aic suggest --partial add --project examples/e7/api_conformance --limit 5
```

JSON payload:

- `schema_version`
- `command` (`suggest`)
- `ok`
- `fast_path`
- `project_root`
- `partial`
- `candidate_count`
- `candidates[]`
  - `qualified_name`
  - `name`
  - `kind`
  - optional `module`
  - `signature`
  - `match_kind` (`exact|case_insensitive_exact|prefix|substring|wildcard|fuzzy`)
  - `distance`
  - `score`
  - `location`
- `diagnostics[]`

Ranking behavior:

- candidate source is the workspace symbol index (`aic symbols`/`aic query` source data), not a full compile
- performance budget is resolve/index only; `aic suggest --partial` must not trigger codegen, execution, or file writes
- ordering is deterministic by match bucket, edit distance, name-length delta, kind priority, module, name, file, and span
- fuzzy fallback is bounded; unmatched queries return an empty `candidates[]` array with exit `0`

## `aic context` output modes

Usage:

```bash
aic context --project . --for function process_user --depth 2 --json
```

JSON payload:

- `protocol_version`
- `phase` (`context`)
- `depth`
- optional `limit`
- `signature` (top-level target signature mirror for agent convenience)
- `target` (`name`, `kind`, `signature`, optional `module`)
- `dependencies[]` (`name`, `kind`, `signature`, optional `module`, `relation`, `distance`, optional contracts/effects/capabilities)
- `callers[]` (`name`, `signature`, optional `module`, `distance`)
- `contracts` (`requires`, `ensures`, `invariant`)
- `related_tests[]`

## `aic synthesize` output modes

Usage:

```bash
aic synthesize --from spec validate_user --project . --json
```

Spec discovery and current scope:

- reads restricted `spec fn` declarations from project `.aic` files, typically under `specs/`
- supports body clauses on separate lines:
  - `requires <expr>`
  - `ensures <expr>`
  - `effects { ... }`
  - `capabilities { ... }`
- does not write files in this wave; it emits deterministic artifact previews with `path_hint`

JSON payload:

- `protocol_version`
- `phase` (`synthesize`)
- `source_kind` (`spec`)
- `target`
- `spec_file`
- `artifacts[]`
  - `kind` (`function`, `attribute-test-fixture`)
  - `name`
  - `path_hint`
  - `content`
  - optional `reason`
- `notes[]`

Synthesis behavior:

- emits an executable function skeleton carrying the declared signature/contracts/effects
- mirrors `effects` into `capabilities` when capabilities are omitted in the spec
- emits a self-contained attribute-test fixture with at least one happy-path test and one failing contract test when supported by the spec
- reports non-lowerable clauses in `notes[]` instead of forcing them into runnable artifacts

## `aic testgen` output modes

Usage:

```bash
aic testgen --strategy boundary --for function normalize_age --project . --emit-dir . --seed 17 --json
```

JSON payload:

- `protocol_version`
- `phase` (`testgen`)
- `strategy` (`boundary|invariant-violation|exhaustive-match|effect-coverage`)
- `seed`
- `target` (`name`, `kind`, optional `module`)
- `artifacts[]`
  - `kind` (`attribute-test-fixture`, `run-pass-fixture`, `compile-fail-fixture`)
  - `name`
  - `path_hint`
  - `content`
  - optional `written_path` when `--emit-dir` materializes the artifact
  - optional `reason`
- `notes[]`

Generation behavior:

- `boundary` derives direct integer edge cases from supported `requires` clauses and emits runnable attribute tests
- `invariant-violation` emits one valid `run-pass` fixture and one failing attribute-test fixture for supported struct invariants
- `exhaustive-match` emits one attribute test per enum variant with a generated exhaustive `match`
- `effect-coverage` emits a declared-effect `run-pass` fixture and, for effectful targets, a missing-effect `compile-fail` fixture
- generated values are deterministic for a fixed seed and selector
- unsupported strategy/target pairs fail with actionable diagnostics instead of producing partial output

## `aic patch` output modes

Usage:

```bash
aic patch --preview patches/valid_patch.json --project . --json
aic patch --apply patches/valid_patch.json --project . --json
```

Patch authoring contract:

- request schema: `docs/agent-tooling/schemas/patch-request.schema.json`
- authoring guide: `docs/agent-tooling/patch-authoring.md`

JSON payload:

- `protocol_version`
- `phase` (`patch`)
- `mode` (`preview|apply`)
- `ok`
- `files_changed[]`
- `applied_edits[]`
  - `file`
  - `start`
  - `end`
  - `replacement`
  - `message`
  - `operation_index`
- `previews[]`
  - `file`
  - `start`
  - `end`
  - `before`
  - `after`
  - `message`
  - `operation_index`
- `conflicts[]`
  - `operation_index`
  - `kind`
  - `message`
  - optional `file`

Patch behavior:

- supported operation kinds are `add_function`, `modify_match_arm`, and `add_field`
- `preview` is deterministic and write-free for a fixed project + patch document
- `apply` writes only after every operation parses cleanly and the patched project passes frontend type/effect validation
- semantically invalid intermediate states are rejected as `conflicts[].kind = "validate_semantics"` with the failing `operation_index`
- overlapping semantic targets are rejected as `conflicts[].kind = "overlap"` before any write is attempted
- apply writes are transactional across touched files and roll back previously written files if a later write fails

## `aic checkpoint` output modes

Usage:

```bash
aic checkpoint create --project . --json
aic checkpoint list --project . --json
aic checkpoint diff ckpt-0001 --to ckpt-0002 --project . --json
aic checkpoint restore ckpt-0001 --project . --json
```

JSON payloads:

- common envelope:
  - `protocol_version`
  - `phase` (`checkpoint`)
  - `command` (`create|list|diff|restore`)
- `create`
  - `checkpoint` (`id`, `file_count`, `total_bytes`, `digest`)
- `list`
  - `checkpoints[]` (`id`, `file_count`, `total_bytes`, `digest`)
- `diff`
  - `from`
  - `to`
  - `summary`
    - `added`
    - `removed`
    - `modified`
    - `unchanged`
    - `semantic_breaking`
    - `semantic_non_breaking`
  - `files[]`
    - `path`
    - `status` (`added|removed|modified|unchanged`)
    - optional `old_sha256`
    - optional `new_sha256`
    - optional `semantic` (`changes[]`, `summary.breaking`, `summary.non_breaking`)
    - optional `semantic_error` when text/hash comparison succeeded but semantic parsing failed
- `restore`
  - `checkpoint`
  - `restored_files`
  - `restored_paths[]`
  - `verified`

Checkpoint behavior:

- snapshots include deterministic project inputs under the resolved project root: `aic.toml`, `aic.lock`, and `*.aic`
- metadata is versioned with `schema_version = 1`; readers reject unknown future versions
- snapshot manifests and per-file SHA256 hashes are validated before diff/restore proceeds
- restore uses staged temp files plus rollback of backed-up originals so validation failures never partially rewrite the workspace
- `diff` defaults to `checkpoint -> current workspace`; `--to` switches to checkpoint-to-checkpoint comparison
- semantic summaries are produced for changed `.aic` files using the same semantic engine as `aic diff --semantic`

## `aic session` output modes

Usage:

```bash
aic session create --project . --label alpha --now-ms 100 --json
aic session list --project . --now-ms 1000 --json
aic session lock acquire sess-0002 --for function handle_result --lease-ms 30000 --operation-id op-valid-modify --project . --json
aic session conflicts plans/valid_plan.json --project examples/e7/session_protocol --json
aic session merge plans/valid_plan.json --project examples/e7/session_protocol --json
```

JSON payloads:

- common envelope:
  - `protocol_version`
  - `phase` (`session`)
  - `command` (`create|list|lock|conflicts|merge`)
- `create`
  - `session` (`id`, optional `label`, `created_ms`, `active_locks`)
- `list`
  - `sessions[]` (`id`, optional `label`, `created_ms`, `active_locks`)
  - `locks[]`
    - `session_id`
    - optional `operation_id`
    - `acquired_ms`
    - `expires_ms`
    - `expired`
    - `target`
- `lock`
  - `action` (`acquire|release`)
  - `ok`
  - `session_id`
  - `target`
  - optional `lock`
  - optional `denied_by`
  - optional `reclaimed_from`
  - `message`
- `conflicts`
  - `plan`
  - `ok`
  - `operations[]` (`session_id`, `operation_id`, `patch`, `symbols[]`)
  - `conflicts[]`
    - `kind`
    - optional `symbol`
    - `sessions[]`
    - `operation_ids[]`
    - `patches[]`
    - `message`
- `merge`
  - `plan`
  - `ok`
  - `valid`
  - `entry`
  - `merged_files[]`
  - `operations[]`
  - `conflicts[]`
  - `diagnostics[]`

Session behavior:

- session registry state is persisted under `.aic-sessions/state.json` inside the resolved project root
- session ids are deterministic (`sess-0001`, `sess-0002`, ...)
- lock ownership is exclusive per resolved symbol key; active conflicting acquisitions return `ok: false` with `denied_by`
- expired leases are reclaimable deterministically; successful reclaims surface `reclaimed_from`
- `conflicts` consumes patch-backed plan documents and reports unknown sessions, unresolved symbols, overlapping symbol edits, and merge-time lock violations without mutating the workspace
- `merge` is validation-only: it applies plan patches inside an isolated temp workspace, rejects individually invalid patch documents as `conflicts[]`, then runs the frontend on the merged result and rejects type/effect-invalid combined state with structured `diagnostics[]`
- current lock keys are based on deterministic source selectors (`kind`, module/name, project-relative file, span start) rather than persistent IR ids

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

## `aic diff --semantic` JSON output

Usage:

```bash
aic diff --semantic <old-file> <new-file>
aic diff --semantic <old-file> <new-file> --fail-on-breaking
```

Output shape:

- `changes[]`
  - `kind` (for example `function_added`, `function_removed`, `params_changed`, `effects_changed`, `requires_changed`)
  - `module`
  - `function`
  - `breaking` (`true|false`)
  - `old` (optional, prior semantic payload/value)
  - `new` (optional, new semantic payload/value)
  - `detail` (optional, extra classification/delta details)
- `summary`
  - `breaking`
  - `non_breaking`

Semantic comparisons include:

- function signature components: generics, params, return type
- effect-set changes
- contract changes (`requires`, `ensures`)

Check mode behavior:

- `--fail-on-breaking` returns non-zero when `summary.breaking > 0`

## Diagnostics output modes

`aic check` and `aic diag` expose stable output modes:

- text (default)
- `--json` (conforms to `docs/diagnostics.schema.json`)
- `--sarif` (SARIF 2.1.0 structure)
- `--warn-unused` (opt-in warnings for unused imports, unreachable/unused functions, and unused variables)
- `aic check --show-holes` emits typed-hole inference JSON:
  - `{"holes":[{"line":<line>,"inferred":"<type>","context":"..."}]}`

`--json`, `--sarif`, and `--show-holes` are mutually exclusive for `aic check`.

Diagnostic JSON versioning notes:

- `diagnostics[*].reasoning` is optional. Omission means no reasoning strategy pack is published for that diagnostic/code path yet.
- When present, `reasoning.schema_version` is currently `1.0`.
- Additive reasoning fields remain compatible within `schema_version: 1.x`; a breaking reasoning-shape change must bump `reasoning.schema_version` and update the published schemas/examples.
- `aic check --json` and `aic diag --json` are the canonical reasoning-bearing surfaces for supported families (`E1033`, `E1100`, `E1214`, `E1218`, `E1250`, `E2001`, `E2102` in this wave).

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
