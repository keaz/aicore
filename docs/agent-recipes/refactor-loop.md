# Refactor Loop Recipe

## Goal

Perform behavior-preserving refactors while keeping deterministic formatting and build outputs.

## Protocol Example

- Check envelope: `examples/agent/protocol_check.json`
- Build envelope: `examples/agent/protocol_build.json`

## Workflow

1. Enforce formatting stability.
2. Re-check type/effect/contracts invariants.
3. Emit a non-executable artifact for ABI-safe review.

## Fallback Behavior

- If format check fails: run formatter and restart the loop.
- If check fails: isolate changed files and re-run only affected commands.
- If artifact build fails: reduce scope and split refactor into smaller commits.

## Docs Test

<!-- docs-test:start -->
aic fmt examples/agent/lsp_workspace/src/main.aic --check
aic check examples/agent/lsp_workspace/src/main.aic
aic build examples/agent/lsp_workspace/src/main.aic --artifact lib -o target/agent-recipes/librefactor-loop.a
<!-- docs-test:end -->
