# `aic suggest-effects` workflow

Use `aic suggest-effects` to inspect inferred transitive effects and apply deterministic missing-effect fixes.

## 1) Inspect suggestions

```bash
aic suggest-effects examples/e7/suggest_effects_demo.aic
```

Each suggestion includes:

- `function`
- `current_effects`
- `required_effects`
- `missing_effects`
- `reason` (effect-to-call-chain explanation)

## 2) Preview missing-effect fixes

```bash
aic diag apply-fixes examples/e7/suggest_effects_demo.aic --dry-run --json
```

This emits deterministic edit plans for missing declared effects diagnostics (`E2001`, `E2005`).

## 3) Apply fixes

```bash
aic diag apply-fixes examples/e7/suggest_effects_demo.aic --json
```

After apply mode, rerun `aic suggest-effects ...`; `suggestions` should be empty for fixed functions.
