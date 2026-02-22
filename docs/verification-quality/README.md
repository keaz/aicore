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

## Fast Command Set

```bash
make test-e8
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
