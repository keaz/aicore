# AI Agent Implementation Guide (E4)

This document explains the EPIC E4 implementation in a code-oriented way for autonomous contributors.

## Compiler pipeline touch points

- Frontend orchestration: `src/driver.rs`
- Effect normalization/validation: `src/effects.rs`
- Effect + type checking (including call graph): `src/typecheck.rs`
- Contract static verification + runtime lowering: `src/contracts.rs`
- Runtime behavior tests: `tests/execution_tests.rs`

## E4-T1 and E4-T2 (Effects)

### Declaration normalization

`src/effects.rs`

- `normalize_effect_declarations(program, file)` is the canonical pass.
- Known taxonomy: `io`, `fs`, `net`, `time`, `rand`.
- Behavior:
  - rejects unknown effects (`E2003`)
  - rejects duplicates (`E2004`)
  - canonicalizes each function signature to sorted unique known effects

`src/driver.rs` runs this pass before resolver/typecheck so downstream signatures are deterministic.

### Transitive effect checking

`src/typecheck.rs`

- Checker records interprocedural call edges while typechecking function bodies.
- A closure pass computes transitive required effects per function.
- Direct undeclared usage remains `E2001`.
- New transitive call-path diagnostic:
  - `E2005`: caller missing effect that appears only through deeper call chain
  - message includes path like `a -> b -> c`

## E4-T3, E4-T4, E4-T5 (Contracts + invariants)

### Static verifier

`src/contracts.rs`

- `verify_static(program, file)` now uses tri-state proof (`True | False | Unknown`).
- Supported proof domain: restricted integer logic with simple interval reasoning and boolean composition.
- Outcomes:
  - proven false contracts => compile-time errors (`E4001`, `E4002`, `E4004`)
  - proven true contracts => discharge note (`E4005`)
  - unknown => runtime checks remain

### Runtime lowering for all exits

`src/contracts.rs`

- `lower_runtime_asserts(program)` now enforces:
  - `requires` once at function entry
  - `ensures` at every explicit `return` and at implicit function exit
- Implementation strategy:
  - rewrites return sites into `let __aic_result_*; assert ensures(result); return __aic_result_*`
  - traverses nested `if`/`match` blocks to instrument embedded explicit returns

### Struct invariant runtime enforcement

`src/contracts.rs`

- Invariants are enforced by rewriting struct literals to synthesized helper constructors:
  - helper name: `__aic_invariant_ctor_<StructName>`
  - helper performs invariant assert, then returns the struct value
- Existing function bodies are rewritten so `StructInit` expressions call the helper.
- This avoids requiring block expressions in IR and works for nested construction sites.

## Determinism and IDs

`src/contracts.rs`

- `IdAlloc` now allocates symbol IDs, node IDs, and type IDs (`next_type`) for synthesized helpers.
- `intern_type(...)` ensures generated helper return types are present in `Program.types`.

## Tests and examples to update when changing E4

- Unit/integration:
  - `src/contracts.rs` tests
  - `src/typecheck.rs` tests
  - `tests/unit_tests.rs`
- Runtime:
  - `tests/execution_tests.rs`
- Examples:
  - `examples/e4/*`
  - CI hook: `scripts/ci/examples.sh`

## Safe extension checklist for agents

1. Keep diagnostics stable and register new codes in `src/diagnostic_codes.rs`.
2. Preserve lowering determinism (stable helper naming and ID allocation).
3. Add both static and runtime tests for any verifier/lowering change.
4. If proof logic expands, keep unknown paths conservative (fallback runtime assert).
5. Re-run `make ci` before commit.
