# Agent Compiler Protocol v1.0

This document defines machine-facing contracts for parse/ast/check/build/fix/testgen/session/patch workflows, context-window and semantic symbol query workflows, and fast API-conformance workflows and their compatibility guarantees.

## Version negotiation

Query contract metadata and negotiate protocol versions:

```bash
aic contract --json --accept-version 1.2,1.0
```

Negotiation rules:

- server advertises `protocol.supported_versions`
- compatible selection requires matching major version
- server version must be less than or equal to requested version
- no compatible match => `protocol.compatible=false` and exit status `1`

## Published schemas

- Parse response: `docs/agent-tooling/schemas/parse-response.schema.json`
- AST response: `docs/agent-tooling/schemas/ast-response.schema.json`
- Check response: `docs/agent-tooling/schemas/check-response.schema.json`
- Build response: `docs/agent-tooling/schemas/build-response.schema.json`
- Fix response: `docs/agent-tooling/schemas/fix-response.schema.json`
- Testgen response: `docs/agent-tooling/schemas/testgen-response.schema.json`
- Session response: `docs/agent-tooling/schemas/session-response.schema.json`
- Patch response: `docs/agent-tooling/schemas/patch-response.schema.json`
- Validate-call response: `docs/agent-tooling/schemas/validate-call-response.schema.json`
- Validate-type response: `docs/agent-tooling/schemas/validate-type-response.schema.json`
- Suggest response: `docs/agent-tooling/schemas/suggest-response.schema.json`
- Context response: `docs/agent-tooling/schemas/context-response.schema.json`
- Query response: `docs/agent-tooling/schemas/query-response.schema.json`
- Symbols response: `docs/agent-tooling/schemas/symbols-response.schema.json`
- Shared raw diagnostics array: `docs/diagnostics.schema.json`
- Patch request schema: `docs/agent-tooling/schemas/patch-request.schema.json`

Positive fixtures:

- `examples/agent/protocol_parse.json`
- `examples/agent/protocol_ast.md`
- `examples/agent/protocol_check.json`
- `examples/agent/protocol_build.json`
- `examples/agent/protocol_fix.json`
- `examples/agent/protocol_testgen.json`
- `examples/agent/protocol_session.json`
- `examples/agent/protocol_patch.json`
- `examples/agent/protocol_validate_call.json`
- `examples/agent/protocol_validate_type.json`
- `examples/agent/protocol_suggest.json`
- `examples/agent/protocol_context.json`
- `examples/agent/protocol_query.json`
- `examples/agent/protocol_query_partial.json`
- `examples/agent/protocol_symbols.json`
- `examples/agent/protocol_symbols_partial.json`

Negative/error fixtures:

- `examples/agent/protocol_parse_error.json`
- `examples/agent/protocol_build_error.json`
- `examples/agent/protocol_fix_conflict.json`

## Diagnostic reasoning metadata

- Any protocol surface that embeds `diagnostics[]` may include optional `diagnostics[*].reasoning`.
- Absence is the explicit fallback: no strategy pack is available yet for that diagnostic code/message variant.
- When present, `reasoning.schema_version` versions the nested reasoning object independently from the outer protocol version.
- `aic check --json` and `aic diag --json` must emit reasoning for the currently supported high-frequency families: `E1033`, `E1100`, `E1214`, `E1218`, `E1250`, `E2001`, `E2102`.
- Within `reasoning`, `hypotheses[]` are sorted deterministically by descending `confidence`, then stable identity fields.
- For multi-file programs, `diagnostics[*].spans[*].file` must identify the originating source file for that span (not the entry file fallback) whenever the span comes from real source.

## Symbol index partial-result metadata

- `query` and `symbols` JSON responses include:
  - `files_scanned`
  - `files_indexed`
  - `files_skipped`
  - `skipped_files[]` (`file`, `error_count`, `code_count`, `codes[]`, `summary`)
- Parse/read failures are never silently dropped from machine-facing responses.
- `--strict-index` is supported for both commands; when any file is skipped, responses set `ok=false` with a stable `error.code = "symbol_index_partial"`.

## Autofix contract (AG-T2)

Reference command:

```bash
aic diag apply-fixes examples/agent/fixable_imports.aic --dry-run --json
```

Behavior:

- only parser-safe diagnostic codes are auto-applied (`E1033`, `E1034`, `E1041`, `E1062`)
- edits are ordered deterministically by file/start/end/message
- overlapping edits become `conflicts[]` and are never applied in write mode
- dry-run computes deterministic plans without filesystem writes

## LSP workflow examples (AG-T3)

LSP request/response samples are documented in:

- `examples/agent/lsp_workflow.json`

Covered methods:

- completion (`textDocument/completion`)
- goto-definition (`textDocument/definition`)
- rename (`textDocument/rename`)
- code action (`textDocument/codeAction`)
- semantic tokens (`textDocument/semanticTokens/full`)

Includes unknown-method error response examples for client fallback handling.

## Incremental daemon workflow (AG-T4)

See `docs/agent-tooling/incremental-daemon.md` for:

- daemon methods (`check`, `build`, `stats`, `shutdown`)
- session methods (`session.create`, `session.list`, `session.lock.acquire`, `session.lock.release`, `session.conflicts`, `session.merge`)
- stable daemon JSON-RPC error taxonomy via `error.data.kind`
- cache invalidation rules based on content hashes
- warm/cold parity verification via `output_sha256`
- troubleshooting common daemon failures

## Collaboration session workflow (AG-T8)

Reference commands:

```bash
aic session create --project examples/e7/session_protocol --label alpha --json
aic session lock acquire sess-0002 --for function handle_result --operation-id op-valid-modify --project examples/e7/session_protocol --json
aic session conflicts examples/e7/session_protocol/plans/valid_plan.json --project examples/e7/session_protocol --json
aic session merge examples/e7/session_protocol/plans/valid_plan.json --project examples/e7/session_protocol --json
```

Behavior:

- `create` persists deterministic session ids under `.aic-sessions/state.json`
- `lock acquire` enforces exclusive symbol ownership with reclaimable expiry leases and stale crashed-owner state-lock recovery
- `conflicts` reports overlap and ownership problems as structured `conflicts[]`, not transport errors
- `merge` applies a plan inside an isolated temp workspace, rejects individually invalid patch documents as structured `conflicts[]`, and rejects type/effect-invalid combined state as structured `diagnostics[]`
- state-lock timeout errors include lock metadata and remediation guidance for deterministic operator recovery

## Structured patch workflow (AG-T7)

Reference commands:

```bash
aic patch --preview examples/e7/patch_protocol/patches/valid_patch.json --project examples/e7/patch_protocol --json
aic patch --apply examples/e7/patch_protocol/patches/valid_patch.json --project examples/e7/patch_protocol --json
```

Behavior:

- patch documents follow `docs/agent-tooling/schemas/patch-request.schema.json`
- supported operation kinds are `add_function`, `modify_match_arm`, and `add_field`
- `preview` computes deterministic `applied_edits[]` and `previews[]` without filesystem writes
- `apply` is transactional across touched files; later write failures trigger rollback of earlier writes
- overlapping semantic targets are rejected as `conflicts[].kind = "overlap"` before any write
- parse-invalid or type/effect-invalid candidate states are rejected with stable `conflicts[]` entries that include `operation_index`, `message`, and optional `file`

## API conformance fast-path workflow

Reference commands:

```bash
aic validate-call math.add --arg Int --arg Int --project examples/e7/api_conformance
aic validate-type 'Result[User, AppError]' --project examples/e7/api_conformance
aic suggest --partial add --project examples/e7/api_conformance --limit 5
```

Behavior:

- `validate-call` checks callable existence, arity, and argument compatibility via parser/resolver/typechecker fast paths only
- `validate-type` checks type-expression syntax plus resolver-visible named types without codegen
- `suggest --partial` ranks workspace symbol candidates deterministically by match bucket, edit distance, name-length delta, kind priority, module, name, file, and span
- performance budget is front-end only: no codegen, execution, artifact writes, or session/daemon mutation
- default candidate cap is `8`; callers may lower or raise it with `--limit`

## Context-window workflow

Reference command:

```bash
aic context --project examples/e7/context_query --for function process_user --depth 2 --limit 3 --json
```

Behavior:

- `context` emits a deterministic focused context envelope with `target`, top-level `signature`, ranked `dependencies[]`, `callers[]`, `contracts`, and `related_tests[]`
- `--depth` expands transitive call and caller closure predictably; larger values may reveal additional indirect dependencies/callers
- `--limit` truncates ranked `dependencies[]`, `callers[]`, and `related_tests[]` after deterministic ordering
- invalid or ambiguous targets return stable non-zero diagnostics instead of partial JSON output

## Semantic query workflow

Reference commands:

```bash
aic query --project examples/e7/symbol_query --kind function --name 'validate*' --module demo.search --effects io --has-contract --generic-over T --limit 10 --json
aic symbols --project examples/e7/symbol_query --json
```

Behavior:

- `query` applies deterministic filtering over the workspace symbol index by `kind`, `name`, `module`, `effects`, contract presence, and generic parameter name
- symbol records always include `name`, `kind`, `module`, `signature`, `effects`, `contracts`, and `location`
- unsupported filter combinations return a stable `ok=false` envelope with `error.code=unsupported_filter_combination` and exit status `2`
- `--limit` is guarded for deterministic pagination and rejects values above `500`

## End-to-end loops (AG-T5)

Executable cookbook workflows:

- `docs/agent-recipes/feature-loop.md`
- `docs/agent-recipes/bugfix-loop.md`
- `docs/agent-recipes/refactor-loop.md`
- `docs/agent-recipes/diagnostics-loop.md`

These recipes are validated as docs-as-tests.

## Compatibility guarantees

- schema IDs are versioned (`*-1.0.schema.json`)
- backward compatibility is guaranteed within major version `1`
- adding optional fields is minor-compatible
- removing required fields or changing required field semantics/types is major-breaking
- diagnostics remain stable through code IDs and deterministic ordering
