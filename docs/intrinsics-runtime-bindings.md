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

Every `intrinsic fn` in `std` has exactly one backend lowering, of one of two kinds:

- **Runtime binding**: the call becomes one call to a C runtime symbol. The declaration must match an entry in `INTRINSIC_BINDING_EXPECTATIONS` (`src/codegen/mod.rs`): the runtime symbol mapping exists and is non-empty, and the declared signature matches the expected lowering signature.
- **Builtin lowering**: the code generator lowers the call by name with dedicated code (element sizes, struct layouts, handles), for example `aic_vec_push_intrinsic[T]`. The name must be listed in `BUILTIN_INTRINSIC_LOWERINGS` (`src/codegen/mod.rs`); a codegen unit test checks that the generator dispatches every listed name. Builtin intrinsics may be generic.

No std function that the backend lowers has an AICore body: a declaration with a missing lowering fails at compile time (`E5020`) instead of running a placeholder body, and `aic verify-intrinsics` reports it as `VI1002`.

Validate with:

```bash
aic verify-intrinsics std --json
```

The report counts `verified_bindings` (runtime bindings) and `builtin_lowerings`; for `std` their sum equals `intrinsic_declarations` and `issue_count` is 0. The command exits non-zero when any issue is found, and `make verify-intrinsics` (part of `make ci`) runs it on `std`.

The verification set covers runtime-bound IO-adjacent modules such as `std.io`, `std.fs`, `std.env`, `std.proc`, `std.net`, `std.tls`, `std.http_server`, and `std.router`. Higher-level composition modules like `std.config` remain wrapper logic over those bindings.

## PROD-T1 Acceptance Coverage

`[PROD-T1]` requires real runtime behavior for networking, crypto, and concurrency intrinsics.
These acceptance checks are wired into repository tests/examples:

- `tcp_connect("127.0.0.1:...")` opens a real loopback socket:
  - `tests/execution_tests.rs::exec_net_tcp_loopback_echo`
  - `tests/execution_tests.rs::exec_prod_t1_intrinsics_runtime_smoke`
- `sha256("hello")` returns the expected digest:
  - `tests/execution_tests.rs::exec_crypto_vectors_roundtrip_and_secure_compare_paths`
  - `tests/execution_tests.rs::exec_prod_t1_intrinsics_runtime_smoke`
- `spawn(|| -> Int { 42 })` runs on runtime-backed task execution:
  - `tests/execution_tests.rs::exec_concurrency_spawn_join_generic_closure_capture_is_stable`
  - `tests/execution_tests.rs::exec_prod_t1_intrinsics_runtime_smoke`
- `std.http_server` request/response helpers and `std.router` dispatch helpers are covered by the current runtime-backed REST examples and router conformance checks.
- Intrinsic binding completeness over the standard library:
  - `tests/e7_cli_tests.rs::verify_intrinsics_std_runtime_bindings_emit_stable_json`
  - `aic verify-intrinsics std --json`

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
- PROD-T1 runtime smoke example:
  - `examples/io/prod_t1_intrinsics_runtime_smoke.aic`

<!-- docs-test:start -->
aic check examples/core/intrinsic_declaration_demo.aic
! aic check examples/core/intrinsic_declaration_invalid_body.aic
aic verify-intrinsics examples/verify/intrinsics/valid_bindings.aic --json
! aic verify-intrinsics examples/verify/intrinsics/invalid_bindings.aic --json
aic verify-intrinsics std --json
aic run examples/io/prod_t1_intrinsics_runtime_smoke.aic
<!-- docs-test:end -->
