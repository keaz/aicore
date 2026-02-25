# Verification Quality Runbooks (QV-T6)

This documentation set is for agents that need to write verifier-friendly AIC code and operate all verification gates in CI.

## Gate Map (QV-T1..QV-T5)

| Gate | Outcome | Runbook |
|---|---|---|
| QV-T1 contracts | prove/reject static obligations and keep residual runtime assertions | `contracts-proof-obligations.md` |
| QV-T2 effect protocols | detect illegal resource state transitions | `effect-protocols.md` |
| QV-T3 fuzzing | detect parser/typechecker crashes with deterministic triage | `fuzz-differential-runbook.md` |
| QV-T4 differential checks | detect parse->IR->format drift | `fuzz-differential-runbook.md` |
| QV-T5 performance gates | enforce latency/throughput budgets and regression thresholds | `perf-sla-playbook.md` |

## CI + Nightly Mapping (Issue #105 / #63)

| Scope | Workflow/job | Command | Artifacts |
|---|---|---|---|
| QV-T1..QV-T5 PR/push gate | `.github/workflows/ci.yml` / `tests-linux-full` (`E8 verification gates`) | `make test-e8` | `target/e8/perf-report.json` (uploaded as `e8-perf-report-linux`) |
| QV-T5 cross-host perf trend | `.github/workflows/ci.yml` / `execution-matrix` (`Run host perf gate suite`) | `cargo test --locked --test e8_perf_tests` | `target/e8/perf-report.json`, `target/e8/perf-report-*.json`, `target/e8/perf-trend-*.json` (uploaded as `e8-perf-${os}`) |
| QV-T3 nightly fuzz stress | `.github/workflows/nightly-fuzz.yml` / `fuzz-nightly` | `make test-e8-nightly-fuzz` | `target/e8/nightly-fuzz-report.json`, `target/e8/fuzz-crashers` (uploaded as `nightly-fuzz-report`) |

## Release-Blocking Policy

- `release.yml` runs a `release-preflight` job that executes `make ci`.
- `make ci` runs `check`, and `check` includes `make test-e8`.
- Result: any QV gate failure (contracts/effect protocols/fuzz/differential/perf) fails CI and blocks the release workflow.
- Nightly fuzz stress runs independently in `nightly-fuzz.yml`, with artifacts retained for triage.

## Fast Command Set

```bash
make test-e8
make test-e8-nightly-fuzz
cargo test --locked --test e8_conformance_tests
cargo test --locked --test e8_fuzz_tests
cargo test --locked --test e8_differential_tests
cargo test --locked --test e8_perf_tests
```

## Verifier-Friendly Examples

- `examples/verify/range_proofs.aic` (mixed discharged and residual obligations)
- `examples/verify/qv_contract_proof_fail.aic` (expected `E4002`)
- `examples/verify/qv_contract_proof_fixed.aic` (proof-friendly fix)
- `examples/verify/file_protocol.aic` (valid state transitions)
- `examples/verify/file_protocol_invalid.aic` (expected `E2006`)

## Incident Reproduction

Use `incident-reproduction.md` to reproduce a verifier incident from failure signal to fix validation.
