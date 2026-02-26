# `aic suggest-effects` workflow

Use `aic suggest-effects` to inspect inferred transitive effects/capabilities and apply deterministic missing-authority fixes.

## 1) Inspect suggestions

```bash
aic suggest-effects examples/e7/suggest_effects_demo.aic
```

Each suggestion includes:

- `function`
- `current_effects`
- `required_effects`
- `missing_effects`
- `current_capabilities`
- `required_capabilities`
- `missing_capabilities`
- `reason` (effect-to-call-chain explanation)
- `capability_reason` (capability-to-call-chain explanation)

## 2) Preview missing-authority fixes

```bash
aic diag apply-fixes examples/e7/suggest_effects_demo.aic --dry-run --json
```

This emits deterministic edit plans for missing declared effects/capabilities diagnostics (`E2001`, `E2005`, `E2009`).

## 3) Apply fixes

```bash
aic diag apply-fixes examples/e7/suggest_effects_demo.aic --json
```

After apply mode, rerun `aic suggest-effects ...`; `suggestions` should be empty for fixed functions.
