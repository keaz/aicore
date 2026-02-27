# Concurrency Stress Replay Gate (AGX3-T3)

This runbook defines deterministic concurrency stress validation and replay for channels, mutexes, tasks, and select semantics.

## Scope

- Plan source: `examples/e8/concurrency-stress-plan.json`
- Gate test: `tests/e8_concurrency_stress_tests.rs`
- Trigger command: `cargo test --locked --test e8_concurrency_stress_tests`
- Included in CI via `make test-e8`

## Determinism Controls

- Seeds and round counts are fixed in the machine-readable plan.
- Run order is deterministic via seed-driven schedule generation.
- Every run emits a stable replay token: `<seed>:<round>:<case_id>`.
- Test harness enforces deterministic scheduling (`concurrency_stress_schedule_is_deterministic`).

## Runtime Budget and Flaky Policy

- Runtime budget is enforced by `max_runtime_seconds` in the stress plan.
- CI retry policy: no retries for this gate.
- If the gate fails, replay exactly one failing token first; only then classify as regression/flake.

## CI Artifacts

- `target/e8/concurrency-stress-report.json`
- `target/e8/concurrency-stress-schedule.json`
- `target/e8/concurrency-stress-replay.txt`

CI uploads these artifacts in `.github/workflows/ci.yml` as `e8-concurrency-stress-linux`.

## Replay Workflow

1. Open `target/e8/concurrency-stress-replay.txt`.
2. Pick one replay command for a failing token.
3. Run exactly that command with `--test-threads=1`.
4. If it reproduces, inspect the matching entry in `concurrency-stress-report.json` (`stdout_excerpt`, `stderr_excerpt`, `case_path`).
5. Validate fix with `make test-e8` and `make ci`.

Example:

```bash
AIC_CONC_STRESS_REPLAY='2717908993:1:worker_pool' cargo test --locked --test e8_concurrency_stress_tests -- --exact concurrency_stress_suite_is_replayable_and_within_budget --nocapture --test-threads=1
```
