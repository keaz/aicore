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
cargo test --locked --test e8_fuzz_tests
cargo test --locked --test e8_fuzz_tests -- --ignored
```

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
