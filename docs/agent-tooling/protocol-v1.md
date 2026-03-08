# Agent Compiler Protocol v1.0

This document defines machine-facing contracts for parse/ast/check/build/fix/testgen/session and fast API-conformance workflows and their compatibility guarantees.

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
- Validate-call response: `docs/agent-tooling/schemas/validate-call-response.schema.json`
- Validate-type response: `docs/agent-tooling/schemas/validate-type-response.schema.json`
- Suggest response: `docs/agent-tooling/schemas/suggest-response.schema.json`
- Shared raw diagnostics array: `docs/diagnostics.schema.json`

Positive fixtures:

- `examples/agent/protocol_parse.json`
- `examples/agent/protocol_ast.md`
- `examples/agent/protocol_check.json`
- `examples/agent/protocol_build.json`
- `examples/agent/protocol_fix.json`
- `examples/agent/protocol_testgen.json`
- `examples/agent/protocol_session.json`
- `examples/agent/protocol_validate_call.json`
- `examples/agent/protocol_validate_type.json`
- `examples/agent/protocol_suggest.json`

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
- `lock acquire` enforces exclusive symbol ownership with reclaimable expiry leases
- `conflicts` reports overlap and ownership problems as structured `conflicts[]`, not transport errors
- `merge` applies a plan inside an isolated temp workspace and rejects type/effect-invalid combined state with structured diagnostics

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
