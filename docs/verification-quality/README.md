# Verification Quality Runbooks (QV-T6)

This documentation set is for agents that need to write verifier-friendly AIC code and operate all verification gates in CI.

## Gate Map (QV-T1..QV-T5 + AGX3-T3)

| Gate | Outcome | Runbook |
|---|---|---|
| QV-T1 contracts | prove/reject static obligations and keep residual runtime assertions | `contracts-proof-obligations.md` |
| QV-T2 effect protocols | detect illegal resource state transitions | `effect-protocols.md` |
| QV-T3 fuzzing | detect parser/typechecker crashes with deterministic triage | `fuzz-differential-runbook.md` |
| QV-T4 differential checks | detect parse->IR->format drift | `fuzz-differential-runbook.md` |
| QV-T5 performance gates | enforce latency/throughput budgets and regression thresholds | `perf-sla-playbook.md` |
| AGX3-T3 concurrency stress/replay | detect deterministic concurrency regressions and emit replay artifacts | `concurrency-stress-replay.md` |

## CI + Nightly Mapping (Issue #105 / #63)

| Scope | Workflow/job | Command | Artifacts |
|---|---|---|---|
| QV-T1..QV-T5 PR/push gate | `.github/workflows/ci.yml` / `tests-linux-full` (`E8 verification gates`) | `make test-e8` | `target/e8/perf-report.json` (uploaded as `e8-perf-report-linux`) |
| AGX3-T3 concurrency stress artifact capture | `.github/workflows/ci.yml` / `tests-linux-full` (`Upload E8 concurrency stress artifacts`) | `cargo test --locked --test e8_concurrency_stress_tests` (via `make test-e8`) | `target/e8/concurrency-stress-report.json`, `target/e8/concurrency-stress-schedule.json`, `target/e8/concurrency-stress-replay.txt` (uploaded as `e8-concurrency-stress-linux`) |
| QV-T5 cross-host perf trend | `.github/workflows/ci.yml` / `execution-matrix` (`Run host perf gate suite`) | `cargo test --locked --test e8_perf_tests` | `target/e8/perf-report.json`, `target/e8/perf-report-*.json`, `target/e8/perf-trend-*.json` (uploaded as `e8-perf-${os}`) |
| QV-T3 nightly fuzz stress | `.github/workflows/nightly-fuzz.yml` / `fuzz-nightly` | `make test-e8-nightly-fuzz` | `target/e8/nightly-fuzz-report.json`, `target/e8/fuzz-crashers` (uploaded as `nightly-fuzz-report`) |

## Release-Blocking Policy

- `release.yml` runs a `release-preflight` job that executes `make ci`.
- `make ci` runs `check`, and `check` includes `make test-e8`.
- Result: any QV or AGX3 concurrency stress gate failure (contracts/effect protocols/fuzz/differential/perf/concurrency-stress) fails CI and blocks the release workflow.
- Nightly fuzz stress runs independently in `nightly-fuzz.yml`, with artifacts retained for triage.

## Fast Command Set

```bash
make test-e8
make test-e8-concurrency-stress
make test-e8-nightly-fuzz
cargo test --locked --test e8_conformance_tests
cargo test --locked --test e8_fuzz_tests
cargo test --locked --test e8_differential_tests
cargo test --locked --test e8_concurrency_stress_tests
cargo test --locked --test e8_perf_tests
```

## Verifier-Friendly Examples

- `examples/verify/range_proofs.aic` (mixed discharged and residual obligations)
- `examples/verify/qv_contract_proof_fail.aic` (expected `E4002`)
- `examples/verify/qv_contract_proof_fixed.aic` (proof-friendly fix)
- `examples/verify/file_protocol.aic` (valid state transitions)
- `examples/verify/file_protocol_invalid.aic` (expected `E2006`)
- `examples/verify/fs_protocol_ok.aic` (valid filesystem handle lifecycle)
- `examples/verify/fs_protocol_invalid.aic` (expected `E2006`)
- `examples/verify/net_proc_protocol_ok.aic` (valid net/proc handle lifecycle)
- `examples/verify/net_proc_protocol_invalid.aic` (expected `E2006`)
- `examples/verify/capability_protocol_ok.aic` (valid effect+capability authority)
- `examples/verify/capability_missing_invalid.aic` (expected `E2009`)
- `examples/e8/concurrency-stress-plan.json` (deterministic seed/schedule source for concurrency replay)

## Incident Reproduction

Use `incident-reproduction.md` to reproduce a verifier incident from failure signal to fix validation.
