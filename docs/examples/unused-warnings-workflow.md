# Unused warnings workflow (`--warn-unused`)

Use this workflow to produce deterministic, agent-readable unused-symbol warnings and apply safe autofixes.

## 1) Inspect warnings

```bash
aic check examples/e7/unused_warnings.aic --warn-unused --json
```

Expected warning codes in output:

- `E6004` unused import
- `E6005` unreachable/unused function
- `E6006` unused variable

## 2) Preview safe fixes

```bash
aic diag apply-fixes examples/e7/unused_warnings.aic --warn-unused --dry-run --json
```

This emits deterministic edit plans without writing files.

## 3) Apply safe fixes

```bash
aic diag apply-fixes examples/e7/unused_warnings.aic --warn-unused --json
```

Safe unused-symbol autofixes currently include:

- removing unused imports
- prefixing unused variables with `_`
