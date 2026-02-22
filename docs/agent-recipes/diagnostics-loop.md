# Diagnostics Loop Recipe

## Goal

Triage failures quickly with machine-readable diagnostics and deterministic remediation steps.

## Protocol Example

- Check envelope: `examples/agent/protocol_check.json`
- Fix envelope: `examples/agent/protocol_fix.json`
- CLI contract envelope: `aic contract --json`

## Workflow

1. Capture structured diagnostics.
2. Explain primary code(s).
3. Validate current CLI contract/protocol compatibility before automated retries.

## Fallback Behavior

- If diagnostics are noisy: sort by severity/code and address the first root cause.
- If unknown diagnostic codes appear: treat as tooling-version mismatch and verify contract.
- If protocol mismatch is reported: negotiate supported versions via `--accept-version`.

## Docs Test

<!-- docs-test:start -->
! aic check examples/e7/diag_errors.aic --json
aic explain E2001 --json
aic contract --json
<!-- docs-test:end -->
