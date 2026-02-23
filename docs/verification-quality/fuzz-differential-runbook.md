# Fuzzing and Differential Runbook

This runbook covers QV-T3 and QV-T4 quality gates.

## Fuzzing Architecture (QV-T3)

Implementation:

- engine: `src/fuzzing.rs`
- corpus: `tests/fuzz/corpus/`
- regressions: `tests/fuzz/regressions/`
- tests: `tests/e8_fuzz_tests.rs`

Deterministic triage outputs:

- crash dedup by stable crash id
- minimized reproducer payloads under `target/e8/fuzz-crashers/`
- unresolved triage entries fail release gates

Run:

```bash
make test-e8
cargo test --locked --test e8_fuzz_tests
make test-e8-nightly-fuzz
cargo test --locked --test e8_fuzz_tests -- --ignored
```

## CI + Nightly Execution Map (Issue #105 / #63)

- PR/push gate workflow: `.github/workflows/ci.yml` (`tests-linux-full` -> `E8 verification gates`) runs `make test-e8` and includes `e8_fuzz_tests` + `e8_differential_tests`.
- Nightly stress workflow: `.github/workflows/nightly-fuzz.yml` (`fuzz-nightly`) runs `make test-e8-nightly-fuzz`.
- Nightly schedule: `cron: "15 3 * * *"` (03:15 UTC daily) plus `workflow_dispatch`.
- Nightly artifact paths (`nightly-fuzz-report`):
  - `target/e8/nightly-fuzz-report.json`
  - `target/e8/fuzz-crashers`
- Related CI perf artifact paths:
  - `target/e8/perf-report.json`
  - `target/e8/perf-report-*.json`
  - `target/e8/perf-trend-*.json`

## Differential Architecture (QV-T4)

Implementation:

- runner: `src/differential.rs`
- corpus: `tests/differential/`
- seed: `examples/e8/roundtrip_random_seed.aic`
- tests: `tests/e8_differential_tests.rs`

Pipeline:

- parse -> IR -> format -> parse -> IR
- compare semantic snapshots
- report first divergence line
- emit minimized mismatch snippet

Run:

```bash
cargo test --locked --test e8_differential_tests
```

## Triage Workflow

1. Confirm deterministic reproduction with the same seed/corpus entry.
2. Minimize input while preserving failure signal.
3. Classify root cause: lexer, parser, typecheck, formatter, IR lowering.
4. Add minimized fixture to regression corpus.
5. Add targeted unit/integration test before fix merge.
