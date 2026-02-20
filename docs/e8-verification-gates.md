# Verification, Fuzzing, and Performance Gates (E8)

This document defines the E8 quality gates and where they are implemented.

## E8-T1: Conformance suite expansion

- Catalog: `examples/e8/conformance_pack/catalog.json`
- Fixture groups:
  - `syntax/`
  - `typing/`
  - `diagnostics/`
  - `codegen/`
- Runner: `src/conformance.rs`
- Test: `tests/e8_conformance_tests.rs`

Run locally:

```bash
cargo test --locked --test e8_conformance_tests
```

## E8-T2: Lexer/parser/typechecker fuzzing

- Seed corpus:
  - `tests/fuzz/corpus/lexer/`
  - `tests/fuzz/corpus/parser/`
  - `tests/fuzz/corpus/typecheck/`
- Regression replay corpus:
  - `tests/fuzz/regressions/lexer/`
  - `tests/fuzz/regressions/parser/`
  - `tests/fuzz/regressions/typecheck/`
- Fuzz engine: `src/fuzzing.rs`
- Tests: `tests/e8_fuzz_tests.rs`
- Nightly workflow: `.github/workflows/nightly-fuzz.yml`

Run locally:

```bash
cargo test --locked --test e8_fuzz_tests
cargo test --locked --test e8_fuzz_tests -- --ignored
```

## E8-T3: Differential roundtrip validation

- Differential runner: `src/differential.rs`
- Reference seed: `examples/e8/roundtrip_random_seed.aic`
- Test: `tests/e8_differential_tests.rs`

The runner compares semantic snapshots before and after `parse -> IR -> format -> parse -> IR` and reports the first divergence line.

Run locally:

```bash
cargo test --locked --test e8_differential_tests
```

## E8-T4: Execution matrix across targets

- Matrix definition: `examples/e8/execution-matrix.json`
- Matrix runner: `src/execution_matrix.rs`
- Matrix program: `examples/e8/matrix_program.aic`
- Test: `tests/e8_matrix_tests.rs`
- CI matrix job: `execution-matrix` in `.github/workflows/ci.yml`

Run locally:

```bash
cargo test --locked --test e8_matrix_tests
```

Platform delta policy:

- Linux + macOS: execute debug/release suites.
- Windows: build-only matrix target; execution is intentionally skipped and documented in matrix metadata.

## E8-T5: Performance budget enforcement

- Budget policy: `docs/perf-budget.json`
- Baseline: `docs/perf-baseline.json`
- Dataset fingerprint lock: `docs/perf-dataset-fingerprint.txt`
- Benchmark/perf gate engine: `src/perf_gate.rs`
- Test: `tests/e8_perf_tests.rs`
- CI artifact: `target/e8/perf-report.json`

Run locally:

```bash
cargo test --locked --test e8_perf_tests
```

## Unified command

Run all E8 gates with:

```bash
make test-e8
```
