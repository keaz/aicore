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
| Effect checker (`io`, `fs`, `net`, `time`, `rand`) | Implemented |
| Contracts (`requires`, `ensures`, `invariant`) | Implemented (runtime lowering + static constant checks) |
| Match + exhaustiveness (Bool/Option/Result + enums) | Implemented |
| LLVM backend (native via clang) | Implemented (toolchain checks + ADT lowering + monomorphization) |
| Generics | Implemented (deterministic instantiation + codegen) |
| Artifact emission | Implemented (`exe`, `obj`, `lib`) |
| Debug info + panic source mapping | Implemented (`aic build --debug-info`) |
| Standard library modules (`io`, `fs`, `net`, `time`, `rand`, `string`, `vec`, `option`, `result`) | Implemented |
| Package lock/checksum/offline cache workflow | Implemented (`aic lock`, `--offline`) |
| API docs generation | Implemented (`aic doc`) |
| Std compatibility/deprecation policy lint | Implemented (`aic std-compat --check`) |

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
  - Linux full validation (unit/golden/execution tests, examples, CLI smoke, docs/schema checks)
  - cross-platform build matrix (Linux/macOS/Windows build + library tests)
- `Release` (`.github/workflows/release.yml`):
  - runs on tags `v*`
  - builds release binaries on Linux/macOS/Windows
  - uploads archives + checksums and publishes a GitHub Release

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
cargo run -- doc examples/e6/doc_sample.aic -o docs/api
cargo run -- std-compat --check --baseline docs/std-api-baseline.json
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
- `aic build`
- `aic doc`
- `aic std-compat`
- `aic run`

## Project layout

- `src/`: compiler implementation
- `std/`: minimal standard library modules
- `examples/`: runnable and checker-focused examples
- `docs/`: MVP language and compiler specs
- `tests/`: golden, unit, and execution tests

## Test suite

- Unit tests: 60 (`src/*` + `tests/unit_tests.rs`)
- Golden tests: 10 (`tests/golden_tests.rs`)
- Execution tests: 11 (`tests/execution_tests.rs`)

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
