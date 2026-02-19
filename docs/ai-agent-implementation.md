# AI Agent Implementation Guide (E4 + E5)

This document is implementation-oriented and intended for autonomous contributors working on compiler/runtime changes.

## Pipeline Touch Points

- Frontend orchestration: `src/driver.rs`
- Effects normalization + validation: `src/effects.rs`
- Type/effect checking + generic instantiation recording: `src/typecheck.rs`
- Contract static verification + runtime lowering: `src/contracts.rs`
- LLVM backend + runtime ABI: `src/codegen.rs`
- CLI command surface: `src/main.rs`
- Runtime and backend execution validation: `tests/execution_tests.rs`

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

### Toolchain contract (E5-T1)

In `src/codegen.rs`:

- `probe_toolchain()` inspects `clang --version`
- `MIN_SUPPORTED_LLVM_MAJOR = 14`
- optional pin `AIC_LLVM_PIN_MAJOR`
- deterministic actionable errors for missing/unsupported/mismatched toolchains

### ADT lowering (E5-T2)

Lowering model:

- Struct: LLVM aggregate in field order
- Enum: `{ i32 tag, <payload slot per variant> }`
- Built-in templates synthesized in backend metadata:
  - `Option[T]`
  - `Result[T, E]`

Code paths:

- Template collection: `collect_type_templates(...)`
- Type lowering: `parse_type_repr(...)`
- Constructors: `gen_struct_init(...)`, `gen_variant_constructor(...)`
- Match lowering: `gen_match_enum(...)`

### Generic monomorphization (E5-T3)

- Typecheck records `program.generic_instantiations` with stable mangles.
- Backend builds `generic_fn_instances` and emits one function per concrete mangle.
- Dedupe key is mangled symbol (deterministic sort + `dedup_by`).
- Generic call dispatch selects the matching concrete instance by argument type.

### Runtime ABI (E5-T4)

`String` ABI is ptr-len-cap:

- AIC type: `{ i8*, i64, i64 }`
- runtime signatures pass scalar `(ptr, len, cap)`

Helpers:

- `aic_rt_print_str`
- `aic_rt_strlen`
- `aic_rt_vec_len`
- `aic_rt_vec_cap`

Panic ABI is source-aware:

- `aic_rt_panic(ptr, len, cap, line, column)`

### Artifact modes (E5-T5)

CLI and driver support:

- `exe`
- `obj`
- `lib`

Key APIs:

- `compile_with_clang_artifact_with_options(...)`
- `build_with_artifact(...)`
- `build_with_artifact_options(...)`

### Debug metadata + panic mapping (E5-T6)

- CLI flag: `aic build --debug-info`
- Codegen option: `CodegenOptions { debug_info: true }`
- Compile option: `CompileOptions { debug_info: true }`

When enabled:

- emits `!DICompileUnit`, `!DISubprogram`, `!DILocation`
- panic/assert callsites are tagged with debug locations
- runtime panic prints mapped line/column

## E5 Validation Inventory

### Tests

- Codegen unit coverage in `src/codegen.rs` tests:
  - toolchain parsing/pinning
  - ADT layout snapshot
  - monomorphization dedupe/determinism
  - debug metadata + panic line mapping
  - panic ABI declaration/runtime signature consistency
- Runtime execution coverage in `tests/execution_tests.rs`:
  - nested ADT match execution
  - multi-concrete generic execution
  - object/library external linkage
  - debug panic line mapping behavior

### Examples

- `examples/e5/hello_int.aic`
- `examples/e5/enum_match.aic`
- `examples/e5/generic_pair.aic`
- `examples/e5/string_len.aic`
- `examples/e5/object_link_main.aic`
- `examples/e5/panic_line_map.aic`

Examples are wired into `scripts/ci/examples.sh`.

## Safe Extension Checklist

1. Keep ABI changes explicit and documented in `docs/llvm-backend.md`.
2. Preserve deterministic output ordering (metadata, symbol names, instantiations).
3. Add both unit and execution tests for any backend/ABI change.
4. Do not change panic/runtime signatures without updating declarations and runtime C together.
5. Keep default build mode unaffected; gate debug-only behavior behind explicit options.
6. Run `make ci` before commit.
