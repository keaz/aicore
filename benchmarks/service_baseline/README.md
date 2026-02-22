# Service Baseline Benchmark Suite

Versioned benchmark policy and per-target trend baselines for `QV-T5`.

Files:

- `budget.v1.json`: threshold policy used by CI perf gates.
- `baselines.v1.json`: target-specific regression baselines.
- `dataset-fingerprint.txt`: lock for the benchmark dataset contents.
- `async-net-gate.v1.json`: async event-loop vs thread-per-connection ratio gate.

Dataset:

- `examples/e8/large_project_bench/`

Usage:

```bash
cargo test --locked --test e8_perf_tests
```

Reports:

- `target/e8/perf-report.json`
- `target/e8/perf-report-<target>.json`
- `target/e8/perf-trend-<target>.json`
