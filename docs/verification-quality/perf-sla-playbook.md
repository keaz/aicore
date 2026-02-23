# Performance SLA Playbook

This runbook covers QV-T5 performance gates.

## Sources of Truth

- budgets: `benchmarks/service_baseline/budget.v1.json`
- per-target baselines: `benchmarks/service_baseline/baselines.v1.json`
- dataset fingerprint lock: `benchmarks/service_baseline/dataset-fingerprint.txt`
- gate engine: `src/perf_gate.rs`
- tests: `tests/e8_perf_tests.rs`

## Gate Semantics

The gate fails when either condition is true:

- absolute budget threshold exceeded
- regression threshold exceeded relative to target baseline and tolerance
- `aic bench` exits non-zero when `report.violations` is non-empty
- `aic bench --compare <baseline.json>` includes per-metric regression status in `trend`

Artifacts:

- `target/e8/perf-report.json`
- `target/e8/perf-report-<target>.json`
- `target/e8/perf-trend-<target>.json`
- `bench.json` (or `--output <path>`) from `aic bench`

## Run Commands

```bash
aic bench --budget benchmarks/service_baseline/budget.v1.json --output target/e8/bench.json
aic bench --budget benchmarks/service_baseline/budget.v1.json --compare benchmarks/service_baseline/baselines.v1.json --output target/e8/bench-compare.json
cargo test --locked --test e8_perf_tests
make test-e8
```

## Regression Triage

1. Verify dataset fingerprint matches the checked-in reference.
2. Re-run on same host target to confirm determinism.
3. Inspect dominant violating metrics in `perf-report-<target>.json`.
4. Determine if change is expected (feature cost) or regression.
5. For expected cost, update baselines with explicit review sign-off.
6. For regressions, add focused benchmark regression test and fix before baseline updates.
