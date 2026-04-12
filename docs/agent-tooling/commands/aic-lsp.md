# `aic lsp` Agent Guide

Related docs:
- [Agent-First aic Command Playbook](../aic-command-playbook.md)
- [`docs/cli-contract.md`](../../cli-contract.md)
- [`examples/agent/lsp_workflow.json`](../../../examples/agent/lsp_workflow.json)
- [`src/lsp.rs`](../../../src/lsp.rs)

## What it does

`aic lsp` starts the AICore language server over stdio using JSON-RPC 2.0.

Implementation source: [`src/lsp.rs`](../../../src/lsp.rs).

## When to use

Use `aic lsp` for long-lived interactive workflows:

- live diagnostics while editing
- hover/definition/completion driven refactors
- code actions and formatting inside editor loops

## When not to use

- CI and deterministic one-shot validation (prefer `aic check --json`, `aic fmt --check`).
- Batch automation that does not need interactive editor state.

## Startup

```bash
aic lsp
```

Clients should send `initialize`, then `initialized`, then normal text-document requests.

Minimal client launch config (stdio):

```json
{
  "command": "aic",
  "args": ["lsp"],
  "transport": "stdio"
}
```

Minimal initialize request shape:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "rootUri": "file:///workspace"
  }
}
```

## Implemented capability surface

From the server `initialize` response:

- `textDocumentSync` (`openClose`, incremental `change`, `save`)
- `hoverProvider`
- `definitionProvider`
- `documentSymbolProvider`
- `workspaceSymbolProvider`
- `documentFormattingProvider`
- `completionProvider` (trigger characters: `.`, `:`, `#`, and `[`)
- `renameProvider`
- `codeActionProvider`
- `semanticTokensProvider` (full)
- `inlayHintProvider`
- `callHierarchyProvider`
- `foldingRangeProvider`
- `selectionRangeProvider`

Attribute-specific support:

- Semantic decorator tokens for `#[...]` attributes, including multi-line attribute forms.
- Hover help for known framework/test attributes and generic preserved attributes.
- Snippet completions for framework route/extractor/validation attributes and attribute-test markers.
- Selection and folding ranges include attribute spans where they are present in source.

## Inlay hint settings

Supports configuration updates via `workspace/didChangeConfiguration`:

- `settings.aic.inlayHints.typeAnnotations` (bool)
- `settings.aic.inlayHints.effectAnnotations` (bool)
- `settings.aic.inlayHints.contractAnnotations` (bool)

## Troubleshooting

- No diagnostics:
  - Ensure client sends `textDocument/didOpen` and `textDocument/didChange` with full text updates.
- Wrong module resolution:
  - Ensure `rootUri` points at workspace root containing `aic.toml` and imports.
- Non-deterministic local editor state:
  - Re-check with `aic check <entry> --json` as canonical confirmation.
- Formatting/code-action mismatch:
  - Verify the client supports `textDocument/formatting` and `textDocument/codeAction`.
  - Re-run `aic fmt <entry> --check` and `aic check <entry> --json` to confirm canonical backend behavior.

## Deterministic agent loop handoff

Use `aic lsp` for interactive editing state, but treat one-shot CLI commands as the canonical gate before applying automated changes or reporting success:

```bash
aic lsp

# deterministic confirmation before commit/apply
aic fmt src/main.aic --check
aic check src/main.aic --json
```
