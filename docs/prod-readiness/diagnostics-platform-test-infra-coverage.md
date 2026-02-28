# Diagnostics, Cross-Platform, and Test Infrastructure Coverage (PROD-T10, PROD-T11, PROD-T12)

This document maps production-readiness tickets to concrete implementation evidence, tests, CI wiring, and runnable examples.

## PROD-T10: Diagnostic Quality

Coverage highlights:
- Structured diagnostic outputs and SARIF shape:
  - `tests/e7_cli_tests.rs` (`diagnostics_json_and_sarif_outputs_are_structured`)
- Deterministic capped/multi-error behavior:
  - `tests/e7_cli_tests.rs` (`diagnostics_are_deduplicated_and_keep_deterministic_capped_prefix`)
- Parser recovery reporting multiple diagnostics:
  - `tests/unit_tests.rs` (`unit_parser_recovery_reports_multiple_errors`)
- Type mismatch and domain diagnostics in check/typecheck suites:
  - `tests/unit_tests.rs`
  - `tests/execution_tests.rs`
- Stable diagnostic code registry coverage:
  - `tests/unit_tests.rs` (`unit_diagnostic_registry_covers_all_emitted_codes`)

Examples:
- `examples/e7/diag_errors.aic`
- `examples/e7/diag_many_errors.aic`

## PROD-T11: Cross-Compilation (Linux/macOS/WASM)

Coverage highlights:
- Build target surface in CLI:
  - `src/main.rs` (`BuildTarget`: `x86_64-linux`, `aarch64-linux`, `x86_64-macos`, `aarch64-macos`, `x86_64-windows`, `wasm32`)
- Hermetic build tests:
  - `tests/e7_build_hermetic_tests.rs`
    - `build_accepts_explicit_host_target`
    - `build_wasm_target_emits_wasm_magic_and_manifest_target`
    - `build_wasm_io_program_binds_runtime_calls_as_imports`
    - `build_rejects_wasm_target_for_non_executable_artifact`
- CI cross-platform matrix:
  - `.github/workflows/ci.yml` (`cross-platform-build` job)
  - Explicit host-target smoke + cross-target object matrix smoke

Examples:
- `examples/core/cross_compile_targets.aic`
- `examples/interop/wasm_hello_world.aic`

## PROD-T12: Comprehensive Test Infrastructure

Coverage highlights:
- E8 quality gates:
  - `tests/e8_conformance_tests.rs`
  - `tests/e8_fuzz_tests.rs`
  - `tests/e8_differential_tests.rs`
  - `tests/e8_matrix_tests.rs`
  - `tests/e8_perf_tests.rs`
  - `tests/e8_concurrency_stress_tests.rs`
- Nightly fuzz workflow:
  - `.github/workflows/nightly-fuzz.yml`
- Bench/coverage CLI contract + checks:
  - `tests/e7_cli_tests.rs` (coverage + bench suites)
- CI gate entrypoints:
  - `Makefile` (`make test-e8`, `make test-e8-nightly-fuzz`)
  - `make ci`

Examples:
- `examples/e8/conformance_pack/`
- `examples/e8/roundtrip_random_seed.aic`
- `examples/e8/matrix_program.aic`

## Quick Validation Commands

```bash
cargo test --locked --test e7_cli_tests diagnostics_json_and_sarif_outputs_are_structured
cargo test --locked --test e7_build_hermetic_tests build_accepts_explicit_host_target
cargo test --locked --test e7_build_hermetic_tests build_wasm_target_emits_wasm_magic_and_manifest_target
cargo test --locked --test e8_conformance_tests
cargo test --locked --test e8_fuzz_tests
cargo test --locked --test e8_differential_tests
cargo test --locked --test e8_matrix_tests
cargo test --locked --test e8_perf_tests
cargo test --locked --test e8_concurrency_stress_tests
make ci
```
