# AI Agent Implementation Guide (E4 + E5 + E6)

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

## Validation Inventory

### Tests

- Library/unit tests in:
  - `src/package_workflow.rs`
  - `src/docgen.rs`
  - `src/std_policy.rs`
  - `tests/unit_tests.rs`
- Runtime/execution tests in:
  - `tests/execution_tests.rs`

### Examples

- `examples/e6/std_smoke.aic`
- `examples/e6/pkg_app/`
- `examples/e6/deps_checksum.aic`
- `examples/e6/doc_sample.aic`
- `examples/e6/deprecated_api_use.aic`

Examples are integrated into `scripts/ci/examples.sh`.

## Safe Extension Checklist

1. Keep lockfile schema deterministic; sort dependencies and outputs.
2. Do not bypass checksum validation when lockfile is present.
3. Keep offline mode conservative: fail fast on missing/corrupted cache.
4. When changing std API surface, update baseline intentionally and review deprecations.
5. Keep new diagnostics registered in `src/diagnostic_codes.rs`.
6. Run `make ci` before commit.
