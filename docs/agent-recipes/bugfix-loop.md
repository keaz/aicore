# Bugfix Loop Recipe

## Goal

Diagnose and resolve parser/type diagnostics with deterministic autofix planning.

## Protocol Example

- Check envelope: `examples/agent/protocol_check.json`
- Fix envelope: `examples/agent/protocol_fix.json`

## Workflow

1. Reproduce failure with structured diagnostics.
2. Generate deterministic autofix plan.
3. Re-check after applying edits (manual or agent-driven).

## Fallback Behavior

- If no fix suggestions are available: run `aic explain <code>` and patch manually.
- If fix conflicts are reported: apply non-conflicting edits first, then rerun dry-run.
- If diagnostics remain after patching: switch to refactor loop to reduce change scope.

## Docs Test

<!-- docs-test:start -->
! aic check examples/agent/fixable_imports.aic --json
aic diag apply-fixes examples/agent/fixable_imports.aic --dry-run --json
aic explain E1033 --json
<!-- docs-test:end -->
