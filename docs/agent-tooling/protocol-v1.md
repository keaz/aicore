# Agent Compiler Protocol v1.0

This document defines machine-facing contracts for parse/ast/check/build/fix workflows and their compatibility guarantees.

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

Positive fixtures:

- `examples/agent/protocol_parse.json`
- `examples/agent/protocol_ast.md`
- `examples/agent/protocol_check.json`
- `examples/agent/protocol_build.json`
- `examples/agent/protocol_fix.json`

Negative/error fixtures:

- `examples/agent/protocol_parse_error.json`
- `examples/agent/protocol_build_error.json`
- `examples/agent/protocol_fix_conflict.json`

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
- cache invalidation rules based on content hashes
- warm/cold parity verification via `output_sha256`
- troubleshooting common daemon failures

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
