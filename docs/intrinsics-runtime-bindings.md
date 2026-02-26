# Intrinsic Runtime Bindings Guide

This guide defines how to implement and verify runtime-bound standard-library APIs without placeholder behavior.

## Declaration Model

Use declaration-only intrinsics:

```aic
intrinsic fn aic_fs_exists_intrinsic(path: String) -> Bool effects { fs };
```

Key rules:

- Intrinsics are signatures only; they never contain bodies.
- Public APIs should be thin wrappers delegating to intrinsics.
- Runtime behavior lives in codegen lowering + runtime symbols, not in `std/*.aic` intrinsic bodies.

## Runtime Linkage Expectations

Every intrinsic-backed API must satisfy all of the following:

- Source declaration exists (`intrinsic fn ...;`).
- Codegen lowering exists (`src/codegen.rs` intrinsic binding table).
- Runtime symbol mapping exists and is non-empty.
- Declared signature matches expected lowering signature.

Validate with:

```bash
aic verify-intrinsics std --json
```

## Side-Effect Boundaries

Effect authority is carried by intrinsic signatures and wrapper APIs:

- Keep effects on the intrinsic declaration aligned with runtime behavior (`fs`, `net`, `proc`, `concurrency`, etc.).
- Wrapper functions must not hide additional side effects.
- If an API composes multiple effectful operations, expose those effects explicitly on the wrapper.

## Diagnostics and Guardrails

Common diagnostics:

- `E1093`: malformed intrinsic declaration (body, generics, contracts, or missing `;`).
- `VI1001`: unsupported intrinsic ABI metadata.
- `VI1002`: missing backend lowering.
- `VI1003`: signature mismatch against backend expectation.
- `VI1004`: missing runtime symbol metadata.

CI policy guard for AGX1 runtime-bound modules:

```bash
make intrinsic-placeholder-guard
```

This rejects source-level body implementations for `aic_conc_*_intrinsic`, `aic_net_*_intrinsic`, and `aic_proc_*_intrinsic` in `std/` policy paths.

## Executable Examples

- Positive declaration example: `examples/core/intrinsic_declaration_demo.aic`
- Negative declaration example: `examples/core/intrinsic_declaration_invalid_body.aic`
- Migration wrapper example: `examples/core/intrinsic_std_wrapper_migration.aic`
- Verification fixtures:
  - `examples/verify/intrinsics/valid_bindings.aic`
  - `examples/verify/intrinsics/invalid_bindings.aic`

<!-- docs-test:start -->
aic check examples/core/intrinsic_declaration_demo.aic
! aic check examples/core/intrinsic_declaration_invalid_body.aic
aic verify-intrinsics examples/verify/intrinsics/valid_bindings.aic --json
! aic verify-intrinsics examples/verify/intrinsics/invalid_bindings.aic --json
<!-- docs-test:end -->
