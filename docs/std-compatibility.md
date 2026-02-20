# Std Compatibility And Deprecation Policy (E6)

This document defines how std APIs evolve without breaking consumers unexpectedly.

## Baseline

- Canonical snapshot: `docs/std-api-baseline.json`
- Check command:

```bash
aic std-compat --check --baseline docs/std-api-baseline.json
```

CI runs this check and fails when baseline symbols are removed or changed.

## Deprecation

Deprecated std APIs are declared in `src/std_policy.rs`.

Current warning:

- `E6001`: deprecated API usage warning with replacement guidance

Rule:

1. Introduce replacement API.
2. Keep old API available and mark deprecated.
3. Emit `E6001` warning with migration help.
4. Update docs/examples.
5. Only remove API in a planned compatibility window with baseline update.

## Breaking Changes

Policy lint failure:

- `E6002`: compatibility check detected removed/changed std API symbols.

To intentionally break compatibility, update:

- std API implementation
- deprecation/migration notes
- `docs/std-api-baseline.json`

and document rationale in the related issue/PR.
