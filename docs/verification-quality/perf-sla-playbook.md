# Performance SLA Playbook

This runbook covers QV-T5 performance gates.

## Sources of Truth

- budgets: `benchmarks/service_baseline/budget.v1.json`
- per-target baselines: `benchmarks/service_baseline/baselines.v1.json`
- dataset fingerprint lock: `benchmarks/service_baseline/dataset-fingerprint.txt`
- REST runtime soak policy: `benchmarks/service_baseline/rest-runtime-soak-gate.v1.json`
- gate engine: `src/perf_gate.rs`
- tests: `tests/e8_perf_tests.rs`
- soak harness: `scripts/ci/rest-runtime-soak-gate.py`

## Gate Semantics

The gate fails when either condition is true:

- absolute budget threshold exceeded
- regression threshold exceeded relative to target baseline and tolerance
- `aic bench` exits non-zero when `report.violations` is non-empty
- `aic bench --compare <baseline.json>` includes per-metric regression status in `trend`
- `rest-runtime-soak-gate.py` command run fails for parse/router/json/async churn scenarios
- `rest-runtime-soak-gate.py` median runtime exceeds either `max_ms` or
  `baseline_ms * (1 + regression_tolerance_pct/100)` for the host target

Artifacts:

- `target/e8/perf-report.json`
- `target/e8/perf-report-<target>.json`
- `target/e8/perf-trend-<target>.json`
- `bench.json` (or `--output <path>`) from `aic bench`
- `target/e8/rest-runtime-soak-report.json`
- `target/e8/rest-runtime-soak-report-<target>.json`

## REST Runtime Soak Determinism Policy

The REST runtime soak gate uses deterministic command selection and deterministic scoring:

- fixed scenario list: parse pipeline benchmark, router path matching, JSON hardening path, async net lifecycle churn
- fixed warmup and measured iterations from `rest-runtime-soak-gate.v1.json`
- median of measured wall-clock samples (`observed_ms`) per scenario
- host-target thresholds from policy (`thresholds.<target>.baseline_ms` and `thresholds.<target>.max_ms`)
- single tolerance policy from `regression_tolerance_pct`

Failure output is actionable by design:

- scenario id
- observed median, baseline, delta percent, absolute budget max, regression limit
- exact repro command per failing scenario

## Run Commands

```bash
aic bench --budget benchmarks/service_baseline/budget.v1.json --output target/e8/bench.json
aic bench --budget benchmarks/service_baseline/budget.v1.json --compare benchmarks/service_baseline/baselines.v1.json --output target/e8/bench-compare.json
cargo test --locked --test e8_perf_tests
make test-e8-rest-runtime-soak
python3 scripts/ci/rest-runtime-soak-gate.py
make test-e8
```

## Regression Triage

1. Verify dataset fingerprint matches the checked-in reference.
2. Re-run on same host target to confirm determinism.
3. Inspect dominant violating metrics in `perf-report-<target>.json`.
4. Determine if change is expected (feature cost) or regression.
5. For expected cost, update baselines with explicit review sign-off.
6. For regressions, add focused benchmark regression test and fix before baseline updates.
7. For REST runtime soak regressions, inspect `rest-runtime-soak-report-<target>.json` and rerun the listed repro command for failing scenarios.
8. Baseline refresh workflow:
   - run `python3 scripts/ci/rest-runtime-soak-gate.py --update-baseline`
   - review changed `baseline_ms` values in `rest-runtime-soak-gate.v1.json`
   - keep or tighten `max_ms`; do not raise both baseline and max without explicit performance sign-off
