# Architecture

See also: [Contributing](./contributing.md), [Syntax Reference](./reference/syntax.md), [Type Reference](./reference/types.md)

This document describes the current AICore compiler/runtime architecture as implemented in `src/`.

## Repository Layers

- CLI orchestration: `src/main.rs`, `src/cli_contract.rs`, `src/coverage.rs`, `src/impact.rs`, `src/profile.rs`
- Frontend pipeline: `src/package_loader.rs`, `compiler/aic/libs/parser/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `compiler/aic/libs/typecheck/src/main.aic`, `compiler/aic/libs/frontend/src/main.aic`, `compiler/aic/libs/typecheck/src/main.aic`, `compiler/aic/libs/typecheck/src/main.aic`
- Backend/code generation: `compiler/aic/libs/backend_llvm/src/main.aic`
- Tooling surfaces: `src/lsp.rs`, `src/daemon.rs`, `src/docgen.rs`, `src/diag_fixes.rs`
- Package/workspace workflows: `src/package_workflow.rs`, `src/package_registry.rs`
- Verification and test harness support: `src/conformance.rs`, `src/differential.rs`, `src/fuzzing.rs`, `src/execution_matrix.rs`, `src/perf_gate.rs`, `src/test_harness.rs`
- Release and policy operations: `src/release_ops.rs`, `src/std_policy.rs`, `src/sandbox.rs`, `src/telemetry.rs`

## Compiler Pipeline

Core frontend entrypoint: `driver::run_frontend_with_options`.

1. Load modules and dependencies
- `package_loader::load_entry_with_options` resolves entry/module graph and merges parsed module items.

2. Parse source to AST
- `lexer::lex` tokenizes.
- `parser::parse` builds `ast::Program` with error recovery.

3. Lower AST to canonical IR
- `ir_builder::build` generates `ir::Program` with stable `SymbolId`, `TypeId`, and `NodeId` allocation.

4. Normalize effect declarations
- `effects::normalize_effect_declarations` validates effect names, removes duplicates, and sorts signatures.

5. Resolve symbols and namespaces
- `resolver::resolve_with_item_modules` builds function/type/trait/impl/module visibility maps.

6. Type/effect/borrow/pattern checking
- `typecheck::check` enforces type rules, effect usage, contract purity, generic bounds, borrow discipline, match exhaustiveness, and protocol checks.

7. Static contract verification
- `contracts::verify_static` classifies obligations as false/true/residual.

Frontend output is `driver::FrontendOutput` (`ir`, `resolution`, `typecheck`, `diagnostics`, timing metrics).

## Build and Run Pipeline

Build and run commands compose frontend with backend lowering:

1. Frontend (`run_frontend_with_options`)
2. Runtime-contract lowering (`contracts::lower_runtime_asserts`)
3. LLVM emission (`codegen::emit_llvm_with_options`)
4. Native artifact generation via clang (`codegen::compile_with_clang_artifact_with_options`)

Artifacts:
- `exe`
- `obj`
- `lib`

Workspace builds use `package_workflow::workspace_build_plan` and deterministic member ordering with incremental fingerprint skipping.

## Command-to-Module Flow

- `aic check` / `aic diag`: frontend diagnostics, optional JSON/SARIF output (`src/sarif.rs`)
- `aic impact`: frontend + typecheck call graph + test/contract cross-reference (`src/impact.rs`)
- `aic coverage`: deterministic coverage JSON from scanned source functions + diagnostics (`src/coverage.rs`)
- `aic fmt`: parse + IR format (`canonical formatting implementation`)
- `aic ir`: frontend + IR emit
- `aic build`: frontend + contract lowering + codegen + clang
- `aic run`: build pipeline + sandboxed process execution (`src/sandbox.rs`)
- `aic run --profile`: profiled build/execute pipeline with deterministic profile report JSON (`src/profile.rs`)
- `aic doc`: frontend + doc rendering (`src/docgen.rs`)
- `aic test`: fixture harness (`src/test_harness.rs`)
- `aic lsp`: language server endpoint (`src/lsp.rs`)
- `aic daemon`: incremental daemon endpoint (`src/daemon.rs`)
- `aic release ...`: reproducibility/SBOM/provenance/policy/security flows (`src/release_ops.rs`)

## Extension Points

### 1) Language grammar and AST
- Edit: `compiler/aic/libs/lexer/src/main.aic`, `compiler/aic/libs/parser/src/main.aic`, `compiler/aic/libs/ast/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `canonical formatting implementation`
- Update reference docs: `docs/reference/*.md`
- Validate with: `tests/golden_tests.rs`, `tests/unit_tests.rs`, `tests/e8_differential_tests.rs`

### 2) Type/effect/contracts semantics
- Edit: `compiler/aic/libs/typecheck/src/main.aic`, `compiler/aic/libs/typecheck/src/main.aic`, `compiler/aic/libs/typecheck/src/main.aic`, `compiler/aic/libs/frontend/src/main.aic`
- Add/maintain diagnostic codes: `diagnostic registry`, `docs/diagnostic-codes.md`
- Validate with: `tests/unit_tests.rs`, `tests/e7_cli_tests.rs`, `tests/e8_conformance_tests.rs`

### 3) Backend lowering and runtime ABI
- Edit: `compiler/aic/libs/backend_llvm/src/main.aic`
- Keep behavior aligned with std APIs in `std/*.aic`
- Validate with: `tests/execution_tests.rs`, `tests/e8_matrix_tests.rs`

### 4) Package/workspace and registry behavior
- Edit: `src/package_loader.rs`, `src/package_workflow.rs`, `src/package_registry.rs`
- Validate with: `tests/e7_cli_tests.rs`, `examples/pkg/*`, `make examples-check`

### 5) Tooling and automation surfaces
- LSP/daemon: `src/lsp.rs`, `src/daemon.rs`
- Agent protocol schemas/docs: `docs/agent-tooling/*`, `docs/agent-recipes/*`
- Validate with: `tests/lsp_smoke_tests.rs`, `tests/agent_protocol_tests.rs`, `tests/agent_recipe_tests.rs`

## Open Issue Touchpoints

For open language issues `#128`, `#130`, `#136`, `#137`, `#138`, `#139`, use
`docs/reference/open-issue-contracts.md` as the behavior contract and route work through these modules.

| Issue | Primary implementation files | Minimum validation focus |
| --- | --- | --- |
| `#128` tuple types | `compiler/aic/libs/lexer/src/main.aic`, `compiler/aic/libs/parser/src/main.aic`, `compiler/aic/libs/ast/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `compiler/aic/libs/typecheck/src/main.aic`, `canonical formatting implementation`, `compiler/aic/libs/backend_llvm/src/main.aic` | `tests/golden_tests.rs`, `tests/unit_tests.rs`, compile-fail fixtures, `tests/execution_tests.rs` |
| `#130` struct methods | `compiler/aic/libs/parser/src/main.aic`, `compiler/aic/libs/ast/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `compiler/aic/libs/frontend/src/main.aic`, `compiler/aic/libs/typecheck/src/main.aic`, `canonical formatting implementation`, `compiler/aic/libs/backend_llvm/src/main.aic` | parser/resolver/typecheck tests, method call execution tests |
| `#136` trait methods + dispatch | `compiler/aic/libs/parser/src/main.aic`, `compiler/aic/libs/ast/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `compiler/aic/libs/frontend/src/main.aic`, `compiler/aic/libs/typecheck/src/main.aic`, `compiler/aic/libs/backend_llvm/src/main.aic` | trait conformance tests, generic bound call tests, dispatch-path execution tests |
| `#137` borrow completeness | `compiler/aic/libs/typecheck/src/main.aic` (borrow model), supporting IR/type utilities | borrow compile-fail matrix + run-pass safety cases |
| `#138` constraints + `where` | `compiler/aic/libs/lexer/src/main.aic`, `compiler/aic/libs/parser/src/main.aic`, `compiler/aic/libs/ast/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `compiler/aic/libs/ir/src/main.aic`, `compiler/aic/libs/frontend/src/main.aic`, `compiler/aic/libs/typecheck/src/main.aic`, `canonical formatting implementation` | generic-bound parser/typecheck tests and equivalence tests |
| `#139` inference improvements | `compiler/aic/libs/typecheck/src/main.aic` (constraint solving/inference), related resolver/type utilities | inference-focused unit tests with ambiguity/failure cases |

## Test Infrastructure Map

Primary orchestration is Make-based:
- `make ci`: full gate (`fmt`, `clippy`, build, tests, examples, docs, security, reproducibility)
- `make ci-fast`: fast local loop
- `make test-e8`: conformance/fuzz/differential/matrix/perf verification pack
- `make test-e9`: release/security operations suite

Detailed test suite ownership and commands are documented in [Contributing](./contributing.md).

## Determinism and Observability

Determinism contracts:
- stable parse/lower traversals
- deterministic diagnostic ordering
- canonical formatting from IR
- deterministic workspace build order and lockfile/checksum workflow

Observability hooks:
- phase/metric emission in frontend, codegen, and run paths via `src/telemetry.rs`
- machine-readable diagnostics JSON and SARIF output contracts
