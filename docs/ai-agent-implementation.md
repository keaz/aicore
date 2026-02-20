# AI Agent Implementation Guide (E4 + E5 + E6 + E7 + E8)

This document is implementation-oriented and intended for autonomous contributors working on compiler/runtime/package changes.

## Pipeline Touch Points

- Frontend orchestration: `src/driver.rs`
- Package loading and imports: `src/package_loader.rs`
- Manifest/lock/checksum/cache: `src/package_workflow.rs`
- Effects normalization + validation: `src/effects.rs`
- Type/effect checking + generic instantiation recording: `src/typecheck.rs`
- Contract static verification + runtime lowering: `src/contracts.rs`
- LLVM backend + runtime ABI: `src/codegen.rs`
- API doc generation: `src/docgen.rs`
- Std compatibility/deprecation policy: `src/std_policy.rs`
- SARIF diagnostics export: `src/sarif.rs`
- Diagnostic explain metadata: `src/diagnostic_explain.rs`
- LSP server: `src/lsp.rs`
- Built-in fixture harness: `src/test_harness.rs`
- Conformance suite runner: `src/conformance.rs`
- Fuzzing engine + corpus replay: `src/fuzzing.rs`
- Differential roundtrip engine: `src/differential.rs`
- Cross-target execution matrix runner: `src/execution_matrix.rs`
- Performance budget runner: `src/perf_gate.rs`
- CLI command surface: `src/main.rs`

## E4 Summary (Effects + Contracts)

### Effects

- Canonical pass: `normalize_effect_declarations(program, file)` in `src/effects.rs`
- Known taxonomy: `io`, `fs`, `net`, `time`, `rand`
- Diagnostics:
  - unknown effect: `E2003`
  - duplicate declaration: `E2004`
  - direct undeclared use: `E2001`
  - transitive missing effect path: `E2005`

### Contracts

- Static verifier: `verify_static(program, file)` in `src/contracts.rs`
- Runtime lowering: `lower_runtime_asserts(program)`
- Guarantees:
  - `requires` checks at function entry
  - `ensures` checks on every explicit return and implicit tail return
  - struct invariants via synthesized helper constructors (`__aic_invariant_ctor_*`)

## E5 Summary (LLVM Backend + Runtime ABI)

### Toolchain contract

In `src/codegen.rs`:

- `probe_toolchain()` inspects `clang --version`
- `MIN_SUPPORTED_LLVM_MAJOR = 14`
- optional pin `AIC_LLVM_PIN_MAJOR`
- deterministic actionable errors for missing/unsupported/mismatched toolchains

### ADT lowering and generics

- Struct: LLVM aggregate in field order
- Enum: `{ i32 tag, payload-slots... }`
- Built-in enum templates: `Option[T]`, `Result[T, E]`
- Generic monomorphization emitted from frontend instantiation metadata with stable symbols

### Runtime ABI and debug mapping

- `String` lowered as `{ i8*, i64, i64 }` (ptr-len-cap)
- runtime panic ABI: `aic_rt_panic(ptr, len, cap, line, column)`
- `aic build --debug-info` emits debug metadata and source-mapped panic locations

### Artifact modes

- `aic build --artifact exe|obj|lib`

## E6 Summary (Std + Package Ecosystem)

### Standard library modules (E6-T1)

Current std set under `std/`:

- `io`, `fs`, `net`, `time`, `rand`, `string`, `vec`, `option`, `result`

Notes:

- Effects are declared on side-effecting std APIs.
- `std.time.now` is compatibility API and intentionally deprecated in policy metadata.

### Manifest + lockfile workflow (E6-T2)

`src/package_workflow.rs`:

- Parses `aic.toml` package/dependency metadata.
- Generates deterministic `aic.lock` via `aic lock`.
- Lockfile entries include dependency name, resolved path, checksum.
- Build/check/run consume lockfile through package loader when present.

Diagnostics:

- `E2106`: lockfile drift (`aic.toml` vs `aic.lock`).

### Checksum verification + offline cache (E6-T3)

`src/package_workflow.rs`:

- Computes deterministic package checksum from `aic.toml` + `.aic` files.
- Verifies dependency checksums before compilation.
- Maintains `.aic-cache/` copies for offline builds.
- Supports offline mode via CLI flag `--offline` on check/build/run/ir/diag.

Diagnostics:

- `E2107`: dependency checksum/source mismatch.
- `E2108`: missing offline lock/cache prerequisites.
- `E2109`: corrupted offline cache entry.

### API doc command (E6-T4)

`src/docgen.rs` + CLI `aic doc`:

- Emits deterministic docs to output directory (default `docs/api`).
- Produces:
  - `index.md`
  - `api.json`
- Includes signatures, effects, contracts, invariants, and deprecation metadata.

### Compatibility + deprecation policy (E6-T5)

`src/std_policy.rs`:

- Deprecated APIs declared in static policy table.
- Typecheck emits warning diagnostics for deprecated API usage.
  - `E6001` warning with replacement guidance.
- Compatibility baseline:
  - `docs/std-api-baseline.json`
- Policy lint command:
  - `aic std-compat --check --baseline docs/std-api-baseline.json`

Diagnostics:

- `E6002`: std compatibility check failure (CLI policy lint output).

## E7 Summary (CLI + Diagnostics + IDE Tooling)

### CLI contract and deterministic exits (E7-T1)

- Command/flag/exit contract metadata: `src/cli_contract.rs`
- CLI contract command: `aic contract --json`
- Exit code mapping:
  - `0`: success
  - `1`: diagnostic/runtime failure
  - `2`: command-line usage error
  - `3`: internal/tooling failure
- Contract doc: `docs/cli-contract.md`

### SARIF export (E7-T2)

- SARIF emitter: `diagnostics_to_sarif(diags, tool_name, tool_version)` in `src/sarif.rs`
- CLI support:
  - `aic check --sarif`
  - `aic diag --sarif`
- SARIF docs:
  - `docs/sarif.md`

### Explain command (E7-T3)

- Explanation engine: `src/diagnostic_explain.rs`
- Commands:
  - `aic explain E####`
  - `aic explain E#### --json`
- Coverage guarantee:
  - `registry_explain_coverage()` test ensures all registered codes have explain metadata/range mapping.

### LSP support and IDE integration (E7-T4)

- Server entrypoint: `aic lsp` (`src/lsp.rs`)
- Implemented methods:
  - `initialize`, `shutdown`
  - `textDocument/didOpen`, `textDocument/didChange`, `textDocument/didSave`
  - `textDocument/hover`, `textDocument/definition`, `textDocument/formatting`
- Diagnostics parity:
  - LSP diagnostics are built from frontend diagnostics and filtered by file.
- IDE docs:
  - `docs/ide-integration.md`
- Sample workspace:
  - `examples/e7/lsp_project/`

### Built-in fixture harness (E7-T5)

- Harness module: `src/test_harness.rs`
- Command:
  - `aic test [path] --mode all|run-pass|compile-fail|golden [--json]`
- Fixture categories discovered by directory segment:
  - `run-pass`
  - `compile-fail`
  - `golden`
- Sample fixtures:
  - `examples/e7/harness/`

## E8 Summary (Verification + Fuzzing + Performance Gates)

### Conformance suites (E8-T1)

- Conformance catalog (expected behavior source of truth):
  - `examples/e8/conformance_pack/catalog.json`
- Categories:
  - `syntax`
  - `typing`
  - `diagnostics`
  - `codegen`
- Runner API:
  - `load_catalog(path)` and `run_catalog(root, catalog)` in `src/conformance.rs`
- Test:
  - `tests/e8_conformance_tests.rs`

### Lexer/parser/typechecker fuzzing (E8-T2)

- Engine:
  - `src/fuzzing.rs`
- Targets:
  - `FuzzTarget::Lexer`
  - `FuzzTarget::Parser`
  - `FuzzTarget::Typecheck`
- Seed corpus:
  - `tests/fuzz/corpus/*`
- Regression replay corpus:
  - `tests/fuzz/regressions/*`
- Nightly stress gate:
  - ignored test in `tests/e8_fuzz_tests.rs`
  - workflow `.github/workflows/nightly-fuzz.yml`

### Differential roundtrip validation (E8-T3)

- Engine:
  - `src/differential.rs`
- Flow:
  - parse -> IR build -> format -> parse -> IR build -> semantic snapshot compare
- Report includes first divergence line when mismatch exists.
- Tests:
  - `tests/e8_differential_tests.rs`
- Reference file:
  - `examples/e8/roundtrip_random_seed.aic`

### Execution matrix across targets (E8-T4)

- Matrix definition:
  - `examples/e8/execution-matrix.json`
- Runner:
  - `run_host_matrix(root, matrix)` in `src/execution_matrix.rs`
- Modes:
  - `debug`
  - `release`
- CI:
  - dedicated `execution-matrix` job in `.github/workflows/ci.yml` (Linux + macOS).
- Platform policy:
  - Windows target is explicitly marked build-only (`execute=false`) with a documented note.

### Performance budgets (E8-T5)

- Budget policy:
  - `docs/perf-budget.json`
- Baseline:
  - `docs/perf-baseline.json`
- Dataset stability lock:
  - `docs/perf-dataset-fingerprint.txt`
- Runner:
  - `run_perf_gate(...)` in `src/perf_gate.rs`
- Report artifact:
  - `target/e8/perf-report.json`
- CI:
  - `make test-e8` in Linux full validation job; report uploaded as artifact.

## Validation Inventory

### Tests

- Library/unit tests in:
  - `src/package_workflow.rs`
  - `src/docgen.rs`
  - `src/std_policy.rs`
  - `src/fuzzing.rs`
  - `src/differential.rs`
  - `src/execution_matrix.rs`
  - `src/perf_gate.rs`
  - `tests/unit_tests.rs`
- Runtime/execution tests in:
  - `tests/execution_tests.rs`
  - `tests/e8_matrix_tests.rs`
  - `tests/e8_conformance_tests.rs`
  - `tests/e8_fuzz_tests.rs`
  - `tests/e8_differential_tests.rs`
  - `tests/e8_perf_tests.rs`

### Examples

- `examples/e6/std_smoke.aic`
- `examples/e6/pkg_app/`
- `examples/e6/deps_checksum.aic`
- `examples/e6/doc_sample.aic`
- `examples/e6/deprecated_api_use.aic`
- `examples/e7/cli_smoke.aic`
- `examples/e7/diag_errors.aic`
- `examples/e7/explain_trigger.aic`
- `examples/e7/lsp_project/`
- `examples/e7/harness/`
- `examples/e8/conformance_pack/`
- `examples/e8/roundtrip_random_seed.aic`
- `examples/e8/matrix_program.aic`
- `examples/e8/large_project_bench/`

Examples are integrated into `scripts/ci/examples.sh`.

## Safe Extension Checklist

1. Keep lockfile schema deterministic; sort dependencies and outputs.
2. Do not bypass checksum validation when lockfile is present.
3. Keep offline mode conservative: fail fast on missing/corrupted cache.
4. When changing std API surface, update baseline intentionally and review deprecations.
5. Keep new diagnostics registered in `src/diagnostic_codes.rs`.
6. Run `make ci` before commit.
7. For E8 changes, update corpus/budget docs and ensure `make test-e8` remains deterministic.
