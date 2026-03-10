# `aic diff --semantic` Agent Guide

Related docs:
- [Agent-First aic Command Playbook](../aic-command-playbook.md)
- [`docs/cli-contract.md`](../../cli-contract.md)
- [`src/semantic_diff.rs`](../../src/semantic_diff.rs)

## What it does

`aic diff --semantic <old-file> <new-file>` compares semantic function-level API behavior between two AIC inputs and emits deterministic JSON.

Implementation source: [`src/semantic_diff.rs`](../../src/semantic_diff.rs).

## When to use

Use semantic diff when an agent must gate refactors or API edits for compatibility impact.

High-value moments:

- pre-merge compatibility checks
- automated upgrade/migration validations
- contract/effect signature change audits

## Usage

```bash
aic diff --semantic old/main.aic new/main.aic
aic diff --semantic old/main.aic new/main.aic --fail-on-breaking
```

`--fail-on-breaking` exits non-zero when `summary.breaking > 0`.

## Output model

Top-level fields:

- `changes[]`
- `summary.breaking`
- `summary.non_breaking`

Common `changes[].kind` values:

- `function_added`
- `function_removed`
- `generics_changed`
- `params_changed`
- `return_changed`
- `effects_changed`
- `requires_changed`
- `ensures_changed`

Contract-classification detail values:

- `new_precondition`, `removed_precondition`, `precondition_changed`
- `new_postcondition`, `removed_postcondition`, `postcondition_changed`

## Interpretation guide

- `function_removed`: breaking
- `params_changed` / `return_changed` / `generics_changed`: breaking
- `effects_changed`:
  - adding required effects is treated as breaking
  - removing effects is non-breaking
- `requires_changed`:
  - adding/changing preconditions is breaking
  - removing preconditions is non-breaking
- `ensures_changed`:
  - adding postconditions is non-breaking
  - removing/changing postconditions is breaking
- `function_added`: non-breaking

## Practical gate pattern

```bash
# baseline check
aic check old/main.aic --json
aic check new/main.aic --json

# compatibility gate
aic diff --semantic old/main.aic new/main.aic --fail-on-breaking
```

## Pre/post refactor snapshot workflow

```bash
cp src/main.aic before/main.aic

# apply automated or manual refactor
aic fmt src/main.aic --check
aic check src/main.aic --json

# compatibility decision
aic diff --semantic before/main.aic src/main.aic --fail-on-breaking
```

## Failure handling

Semantic diff can fail before comparison if parsing/import resolution fails.

If that happens:

1. Run `aic check <file> --json` on each side first.
2. Fix parse/type/import errors.
3. Re-run semantic diff.
