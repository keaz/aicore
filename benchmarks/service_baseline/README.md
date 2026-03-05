# Service Baseline Benchmark Suite

Versioned benchmark policy and per-target trend baselines for `QV-T5`.

Files:

- `budget.v1.json`: threshold policy used by CI perf gates.
- `baselines.v1.json`: target-specific regression baselines.
- `dataset-fingerprint.txt`: lock for the benchmark dataset contents.
- `async-net-gate.v1.json`: async event-loop vs thread-per-connection ratio gate.
- `rest-runtime-soak-gate.v1.json`: parse/router/json/async churn gate policy for CI.

Dataset:

- `examples/e8/large_project_bench/`

Usage:

```bash
cargo test --locked --test e8_perf_tests
python3 scripts/ci/rest-runtime-soak-gate.py
```

Reports:

- `target/e8/perf-report.json`
- `target/e8/perf-report-<target>.json`
- `target/e8/perf-trend-<target>.json`
- `target/e8/rest-runtime-soak-report.json`
- `target/e8/rest-runtime-soak-report-<target>.json`
