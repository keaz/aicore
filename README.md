# AICore (MVP)

AICore is an **agent-native, IR-first programming language** with deterministic formatting, structured diagnostics, a type + effect checker, contract support, and an LLVM backend.

The canonical source of truth is **IR** (`aic ir --emit json`), while text syntax is a deterministic view (`aic fmt`).

## Status

| Area | MVP status |
|---|---|
| IR-first pipeline | Implemented |
| Deterministic parser/formatter | Implemented |
| Structured diagnostics JSON (`code`, spans, fixes) | Implemented |
| Type checker (Int/Bool/String/Unit, functions, enums, structs) | Implemented |
| Effect checker (`io`, `fs`, `net`, `time`, `rand`, `env`, `proc`, `concurrency`) | Implemented |
| Contracts (`requires`, `ensures`, `invariant`) | Implemented (runtime lowering + static constant checks) |
| Match + exhaustiveness (Bool/Option/Result + enums) | Implemented |
| Pattern matching 1.0 (`|` alternatives + guard typing/coverage checks) | Implemented (guarded arms are frontend-only for now; backend emits `E5023`) |
| Async/await core model (`async fn`, `await`, `Async[T]`) | Implemented (deterministic typing + diagnostics + execution path) |
| Trait/interface MVP (`trait`/`impl` + bounded generics) | Implemented (coherence checks + deterministic bound enforcement) |
| Result propagation operator (`expr?`) | Implemented (typed error propagation with no implicit conversion) |
| Mutability + borrow discipline MVP (`let mut`, assignment, `&`/`&mut`) | Implemented (alias checks + conflict diagnostics + mutable Vec flow) |
| LLVM backend (native via clang) | Implemented (toolchain checks + ADT lowering + monomorphization) |
| Generics | Implemented (deterministic instantiation + codegen) |
| Artifact emission | Implemented (`exe`, `obj`, `lib`) |
| Debug info + panic source mapping | Implemented (`aic build --debug-info`) |
| Standard library modules (`io`, `fs`, `env`, `path`, `proc`, `net`, `time`, `rand`, `regex`, `concurrent`, `string`, `vec`, `option`, `result`) | Implemented |
| Package lock/checksum/offline cache workflow | Implemented (`aic lock`, `--offline`) |
| API docs generation | Implemented (`aic doc`) |
| Std compatibility/deprecation policy lint | Implemented (`aic std-compat --check`) |
| CLI contract + deterministic exits | Implemented (`aic contract`) |
| SARIF diagnostics export | Implemented (`aic check --sarif`) |
| Diagnostic explain command | Implemented (`aic explain`) |
| LSP server (diagnostics/hover/definition/format) | Implemented (`aic lsp`) |
| Incremental check/build daemon | Implemented (`aic daemon`) |
| Built-in fixture harness | Implemented (`aic test`) |
| Verification/fuzzing/performance gates | Implemented (E8 conformance + differential + matrix + perf budgets) |
| Release reproducibility manifest pipeline | Implemented (`aic release manifest`, `verify-manifest`) |
| SBOM + signed provenance flow | Implemented (`aic release sbom`, `provenance`, `verify-provenance`) |
| Security audit + threat model checks | Implemented (`aic release security-audit`) |
| Sandboxed run profiles | Implemented (`aic run --sandbox`) |
| Compatibility + migration policy check | Implemented (`aic release policy --check`) |
| Complete IO runtime agent playbooks | Implemented (`docs/io-runtime/`) |

## Prerequisites

- Rust (stable)
- `clang` in `PATH` (used to compile emitted LLVM IR + runtime C shim)
- `make` (for local CI orchestration)
- `python3` (used by docs/schema checks)

## Build

```bash
cargo build
```

## Local CI With Make

Run the same checks as GitHub Actions locally:

```bash
make ci
```

Useful targets:

- `make ci-fast` (quick pre-commit loop)
- `make check` (full validation except fmt/lint)
- `make examples-check`
- `make examples-run`
- `make cli-smoke`
- `make docs-check`
- `make test-e8`
- `make test-e8-nightly-fuzz`

Install git hooks:

```bash
make init
```

This installs:

- `.git/hooks/pre-commit` -> runs `make ci-fast`
- `.git/hooks/pre-push` -> runs `make ci`

## GitHub Actions

- `CI` (`.github/workflows/ci.yml`):
  - quality checks (`fmt`, `clippy`, build)
  - Linux full validation (unit/golden/execution/E7/E8 tests, examples, CLI smoke, docs/schema checks)
  - host execution matrix suite on Linux/macOS (`tests/e8_matrix_tests.rs`)
  - cross-platform build matrix (Linux/macOS/Windows build + library tests)
- `Nightly Fuzz` (`.github/workflows/nightly-fuzz.yml`):
  - scheduled lexer/parser/typechecker fuzz stress suite
  - uploads nightly fuzz report artifacts
- `Release` (`.github/workflows/release.yml`):
  - runs on tags `v*`
  - builds release binaries on Linux/macOS/Windows
  - uploads archives + checksums and publishes a GitHub Release
- `Security` (`.github/workflows/security.yml`):
  - runs security audit checks on push/PR/schedule
  - enforces threat-model and workflow hardening checks

## CLI

```bash
cargo run -- init myproj
cargo run -- check examples/option_match.aic
cargo run -- check examples/effects_reject.aic --json
cargo run -- fmt examples/option_match.aic
cargo run -- ir examples/option_match.aic --emit json
cargo run -- ir-migrate old_ir.json
cargo run -- build examples/option_match.aic -o option_match
cargo run -- build examples/e5/object_link_main.aic --artifact obj -o object_link_main.o
cargo run -- build examples/e5/panic_line_map.aic --debug-info -o panic_dbg
cargo run -- lock examples/e6/pkg_app
cargo run -- check examples/e6/pkg_app --offline
cargo run -- pkg publish examples/e6/pkg_app
cargo run -- pkg search pkg
cargo run -- pkg install util@^1.0.0 --path examples/e6/pkg_app
cargo run -- pkg install corp/http_client@^1.2.0 --registry-config aic.registry.json --token "$AIC_PRIVATE_TOKEN"
cargo run -- doc examples/e6/doc_sample.aic -o docs/api
cargo run -- std-compat --check --baseline docs/std-api-baseline.json
cargo run -- check examples/e7/diag_errors.aic --sarif
cargo run -- explain E2001
cargo run -- lsp
cargo run -- daemon
cargo run -- test examples/e7/harness --json
cargo run -- contract --json
cargo run -- release manifest --output target/release/repro-manifest.json
cargo run -- release sbom --output target/release/sbom.json
cargo run -- release policy --check
cargo run -- run examples/option_match.aic
```

Commands:

- `aic init`
- `aic check`
- `aic diag --json`
- `aic fmt`
- `aic ir --emit json|text`
- `aic ir-migrate`
- `aic lock`
- `aic pkg`
- `aic build`
- `aic doc`
- `aic std-compat`
- `aic explain`
- `aic lsp`
- `aic daemon`
- `aic test`
- `aic contract`
- `aic release`
- `aic run`

## Project layout

- `src/`: compiler implementation
- `std/`: minimal standard library modules
- `examples/`: runnable and checker-focused examples
- `docs/`: MVP language and compiler specs
- `tests/`: golden, unit, and execution tests
- `tools/vscode-aic/`: prototype VS Code extension wiring to `aic lsp`

## Test suite

- Core unit tests: 94 (`src/*` library tests)
- Unit integration tests: 72 (`tests/unit_tests.rs`)
- Golden tests: 16 (`tests/golden_tests.rs`)
- Execution tests: 22 (`tests/execution_tests.rs`)
- CLI contract tests: 5 (`tests/e7_cli_tests.rs`)
- LSP smoke tests: 2 (`tests/lsp_smoke_tests.rs`)
- E8 verification tests: 11 total / 10 active (`tests/e8_*`)

## Determinism guarantees (MVP)

- Stable tokenization/parsing.
- Stable IR IDs (`SymbolId`, `TypeId`, `NodeId`) by deterministic traversal.
- Canonical formatting from IR.
- Deterministic diagnostic ordering by span/code/message.

## Diagnostics JSON shape

```json
[
  {
    "code": "E2001",
    "severity": "error",
    "message": "calling 'io_fn' requires undeclared effects: io",
    "spans": [
      { "file": "src/main.aic", "start": 95, "end": 102, "label": null }
    ],
    "help": ["add `effects { io }` on the enclosing function"],
    "suggested_fixes": []
  }
]
```
