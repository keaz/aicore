# Self-Host Bootstrap Performance Baselines

This document defines the production performance gate for the AICore self-host bootstrap. It covers only compiler bootstrap readiness: host preflight, stage0, stage1, stage2, Rust-vs-self-host parity, stage compiler matrix validation, stage1/stage2 reproducibility, produced artifact sizes, and child-process peak RSS. Application/runtime benchmarks belong in separate manifests.

## Budget Manifest

The checked-in budget manifest is:

```text
docs/selfhost/bootstrap-budgets.v1.json
```

The manifest format is `aicore-selfhost-bootstrap-budgets-v1` with `schema_version` 1. It has production entries for supported self-host platforms:

- `linux`: used by Linux hosts, including GitHub `ubuntu-latest`.
- `macos`: used by Darwin hosts, including GitHub `macos-latest`.

Each platform entry defines:

- `baseline.total_duration_ms`: expected complete bootstrap gate duration.
- `baseline.max_step_duration_ms`: expected slowest single step duration.
- `baseline.max_artifact_size_bytes`: expected largest produced compiler/report artifact.
- `baseline.max_child_peak_rss_bytes`: expected maximum child-process resident set size observed by the gate.
- `baseline.reproducibility_duration_ms`: expected time to compare and strip-normalize stage1/stage2 artifacts.
- `baseline.steps.<step>.duration_ms`: expected duration for `host-preflight`, `stage0`, `stage1`, `stage2`, `parity`, and `stage-matrix`.
- `budgets.max_step_ms`: hard upper bound for any single step.
- `budgets.max_total_ms`: hard upper bound for the whole gate.
- `budgets.max_artifact_bytes`: hard upper bound for any produced artifact tracked by the gate.
- `budgets.max_peak_rss_bytes`: hard upper bound for child-process peak RSS.
- `budgets.max_reproducibility_ms`: hard upper bound for the reproducibility comparison.
- `budgets.per_step_ms`: hard upper bound for each required bootstrap step.

The bootstrap gate fails in supported mode when a metric exceeds its budget or when a required metric is missing.

## Reports

`make selfhost-bootstrap` writes three report files under `target/selfhost-bootstrap/`:

- `report.json`: complete readiness report with host details, steps, reproducibility, and performance status.
- `performance-report.json`: performance-focused report with the same budget source/version and full step metrics.
- `performance-trend.json`: stable trend artifact with budget values, baseline values, top-level metrics, per-step metrics, and violations.

The `performance.budget_source` object records the manifest path, schema version, platform entry, and any local overrides. CI and release workflows upload all three report files for Linux and macOS.

## Local Overrides

Release gates use the checked-in manifest defaults. Local overrides are only for investigation and are recorded in `performance.budget_source.overrides`:

```bash
AIC_SELFHOST_MAX_STEP_MS=3600000 make selfhost-bootstrap
python3 scripts/selfhost/bootstrap.py --max-total-ms 7200000 --mode supported
```

Budget values accept `0`, `off`, `none`, or `disabled` to disable a local limit. Do not use disabled budgets for release evidence.

The bootstrap timeout is separate from performance budgets:

```bash
AIC_SELFHOST_BOOTSTRAP_TIMEOUT=3600 make selfhost-bootstrap
```

CI and release workflows set the timeout to avoid killing a valid but slow bootstrap before the manifest budgets can report the real violation.

## Updating Baselines

Update `docs/selfhost/bootstrap-budgets.v1.json` only when a self-hosting issue documents why the new numbers are expected. A valid update includes:

- Local `make selfhost-bootstrap` evidence for the current host.
- Linux and macOS CI self-host artifact links when workflow behavior is affected.
- A comparison of `performance-trend.json` against the previous baseline.
- Updated tests if the schema, required metrics, or violation text changes.
- Reviewer confirmation that the budget change does not mask an unintended compiler regression.

Do not relax budgets just to pass CI. If a regression is real, keep the issue open and fix the compiler or bootstrap path before updating the manifest.

## Release Review

Before release approval, inspect the uploaded platform artifacts:

- `selfhost-bootstrap-ubuntu-latest`
- `selfhost-bootstrap-macos-latest`
- `release-selfhost-bootstrap-ubuntu-latest`
- `release-selfhost-bootstrap-macos-latest`

For each platform, confirm:

- `report.json` has `status` set to `supported-ready`.
- `performance.ok` is `true`.
- `performance.budget_source.schema_version` is `1`.
- `performance.budget_source.platform` matches the host.
- `performance.budget_source.overrides` is empty for CI/release evidence.
- `performance-trend.json` contains top-level metrics and all six required step entries.
