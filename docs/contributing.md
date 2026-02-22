# Contributing

See also: [Architecture](./architecture.md), [Spec](./spec.md), [Reference Manual](./reference/syntax.md)

This guide defines the implementation workflow for contributors working in this repository.

## Prerequisites

- Rust stable toolchain
- `clang` in `PATH`
- `make`
- `python3`

## Local Setup

```bash
cargo build
make ci-fast
```

Install hooks:

```bash
make init
```

## Required Quality Bar

- Deliver complete behavior for the scoped change.
- Do not leave stub, dummy, or no-op success paths in touched code.
- Update tests for both positive and failure paths.
- Update docs/spec/reference when behavior changes.
- Ensure deterministic outputs remain stable (formatting, diagnostics ordering, contract outputs).

## Development Workflow

1. Identify subsystem ownership
- Frontend grammar/parsing: `src/lexer.rs`, `src/parser.rs`, `src/ast.rs`
- Semantic checks: `src/resolver.rs`, `src/typecheck.rs`, `src/effects.rs`, `src/contracts.rs`
- Backend/runtime lowering: `src/codegen.rs`
- Package/workspace flows: `src/package_loader.rs`, `src/package_workflow.rs`, `src/package_registry.rs`

2. Implement and keep docs synchronized
- Update relevant files under `docs/reference/` for language behavior.
- Update `docs/spec.md` when externally visible guarantees change.

3. Add or update diagnostics when needed
- Register new diagnostic code in `src/diagnostic_codes.rs`.
- Update `docs/diagnostic-codes.md`.
- Verify `aic explain <CODE>` coverage.

4. Run targeted tests first
- Run the smallest suite that directly exercises the changed subsystem.

5. Run full validation gate
- `make ci`

6. Validate example behavior for language/runtime changes
- `make examples-check`
- `make examples-run`

## Test Infrastructure Guide

### Unit and golden

- Library/unit tests:
  - `cargo test --locked --lib`
  - `cargo test --locked --test unit_tests`
- Parser/formatter golden tests:
  - `cargo test --locked --test golden_tests`

### Execution and CLI/LSP integration

- LLVM execution tests:
  - `cargo test --locked --test execution_tests`
- CLI/LSP and harness integration:
  - `cargo test --locked --test e7_cli_tests`
  - `cargo test --locked --test lsp_smoke_tests`
  - `cargo test --locked --test agent_protocol_tests`
  - `cargo test --locked --test agent_recipe_tests`

### Verification quality gates (E8)

- Conformance:
  - `cargo test --locked --test e8_conformance_tests`
- Fuzz regression and stress:
  - `cargo test --locked --test e8_fuzz_tests`
  - `cargo test --locked --test e8_fuzz_tests -- --ignored`
- Differential roundtrip:
  - `cargo test --locked --test e8_differential_tests`
- Execution matrix:
  - `cargo test --locked --test e8_matrix_tests`
- Performance gate:
  - `cargo test --locked --test e8_perf_tests`

### Release and security operations (E9)

- `cargo test --locked --test e9_release_ops_tests`

### Fixture harness

Run categorized `.aic` fixtures (`run-pass`, `compile-fail`, `golden`):

```bash
cargo run --quiet --bin aic -- test examples/e7/harness --mode all
cargo run --quiet --bin aic -- test examples/e7/harness --mode compile-fail --json
```

## Docs and Static Validation

- `make docs-check` validates required docs presence and schema JSON shape.
- For touched docs, run a marker scan to ensure no unfinished notes remain.

## Common Change Playbooks

### Adding syntax or expression features

- Update lexer/parser/AST/IR lowering/formatter.
- Add unit + golden + execution coverage.
- Refresh `docs/reference/syntax.md` and impacted reference pages.

### Adding type/effect/contracts rules

- Update semantic pass (`typecheck.rs`, `effects.rs`, `contracts.rs`).
- Add deterministic diagnostics with registered codes.
- Add compile-fail and positive tests.

### Adding std/runtime APIs

- Update std surface in `std/*.aic` and backend lowering in `src/codegen.rs`.
- Validate with execution tests and examples.
- Keep compatibility/deprecation policies coherent (`aic std-compat --check`).

## Final Pre-Submission Checklist

- `make ci` passes.
- Targeted subsystem tests pass.
- Example validation run (when applicable).
- Docs/spec/reference updated for behavior changes.
- Touched content is production-ready and free of unfinished notes.
