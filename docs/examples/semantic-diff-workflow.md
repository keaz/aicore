# `aic diff --semantic` workflow

Use semantic diff to compare API-level behavior changes between two AIC files.

## 1) Run semantic diff

```bash
aic diff --semantic examples/e7/semantic_diff_v1.aic examples/e7/semantic_diff_v2.aic
```

Output is deterministic JSON:

- `changes[]` entries with `kind`, `module`, `function`, `breaking`
- `summary.breaking`
- `summary.non_breaking`

## 2) Gate breaking changes in CI

```bash
aic diff --semantic examples/e7/semantic_diff_v1.aic examples/e7/semantic_diff_v2.aic --fail-on-breaking
```

If any semantic breaking change is detected, the command exits non-zero.

