# Self-Host Conformance Coverage

The Rust-vs-self-host production conformance manifest is `tests/selfhost/rust_vs_selfhost_manifest.json`.

The machine-readable coverage map is `tests/selfhost/conformance_coverage.json`. Every manifest case must appear in at least one coverage area, and every required core-language area must have at least one case.

## Coverage Areas

| Area | Scope |
| --- | --- |
| `parser_syntax` | Modules, imports, declarations, intrinsic syntax, expressions, statements, blocks, and parser recovery diagnostics. |
| `resolver_visibility` | Module/import resolution, namespaces, missing symbols, visibility-shaped examples, and deterministic symbol surfaces. |
| `semantics_generics_traits` | Generic arity, trait bounds, trait/impl discovery, trait dispatch, and generic instantiation metadata. |
| `typecheck_core` | Primitive operators, calls, named arguments, variant payloads, match guards, exhaustiveness, loops, vectors, and return typing. |
| `effects_capabilities_contracts` | Declared effects, capability authority, static contracts, effectful contract rejection, and async signatures. |
| `borrow_resource` | Move/use tracking, branch reinitialization, borrow/resource diagnostics, and terminal resource protocol reuse. |
| `ir_lowering` | Canonical self-host IR JSON for accepted core-language forms. |
| `backend_build_run` | Backend build/run materialization for representative executable self-host cases. |
| `deterministic_output` | Stable comparison modes for artifacts, self-host IR JSON, and primary diagnostic codes. |

## Case Policy

Each manifest entry must be a real executable parity comparison against the Rust reference compiler and `aic_selfhost`.

Positive cases use `check` and `ir-json` when the case is intended to prove front-end and IR parity. Build/run cases use `artifact-exists` for build materialization while native binary byte parity remains reserved for the default cutover gate.

Negative cases use `diagnostic-code` only when both compilers intentionally fail and agree on the primary diagnostic code. Do not add expected-fail entries for behavior that should pass in production. If a core-language feature is not ready, keep the relevant implementation issue open instead of encoding a fake pass.

Cases normally use the parity harness default timeout. A case may define `timeout` for all of its actions or `timeouts` for action-specific overrides when a real compiler-source execution is intentionally heavier. Keep those overrides bounded, documented by the case name and manifest context, and prefer action-specific values such as the smoke suite's `source_diagnostics_check` `run` timeout.

## Update Workflow

When adding or changing a core-language feature:

1. Add or update at least one positive case when the feature should compile.
2. Add or update at least one negative case for the stable diagnostic behavior.
3. Add the case name to `tests/selfhost/conformance_coverage.json`.
4. Run `python3 scripts/selfhost/parity.py --manifest tests/selfhost/rust_vs_selfhost_manifest.json --list`.
5. Run `make selfhost-parity-candidate`.
6. Run `cargo test --locked --test selfhost_parity_tests`.

The conformance suite is part of the supported self-hosting gate. Missing coverage-map entries, duplicate case names, unsupported comparison modes, or non-existent source paths must fail locally before issue closure.
