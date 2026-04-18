# Self-Hosted LLVM Backend

`compiler/aic/libs/backend_llvm` is the self-hosted backend package for the current compiler port. It consumes `compiler.ir.IrProgram` values and emits deterministic LLVM-text artifacts that the driver can materialize into object files, archives, or executables.

## Artifact Contract

The backend artifact format is `aicore-selfhost-backend-llvm-v1`.

Each artifact records:

- artifact name and deterministic file name
- target triple, or native target metadata when no triple is provided
- requested artifact kind
- LLVM IR text for successful LLVM-text emission
- backend symbols for functions, structs, enums, traits, impls, aliases, and constants
- feature metadata for the lowered IR surface
- native-link metadata copied from package or driver inputs
- deterministic digest metadata
- backend diagnostics

The package emits LLVM text directly. Requests for native object, static library, or executable materialization return `E5105`; the driver is responsible for turning validated LLVM text into native artifacts and linking the existing runtime C sources when executable materialization needs runtime symbols.

## Lowered Surface

The backend emits executable LLVM for the backend-covered primitive forms:

- primitive function signatures using integer, boolean, unit, and `String` return forms
- aggregate function signatures for backend-covered structs and enums, including `Vec[T]`, `Option[T]`, `Result[T, E]`, `SourceId`, `DriverSource`, `DriverCommandResult`, `ProcOutput`, and `SourceLoadState`
- `String` values as `%aic.String = type { i8*, i64, i64 }`, matching the runtime `ptr`, `len`, and `cap` ABI
- integer and boolean literal returns
- string literal returns using deterministic module-level LLVM constants
- runtime-backed `string.replace` return expressions over string literals, string parameters, and nested replacement calls
- runtime-backed `string.contains`, `string.starts_with`, and `string.ends_with` predicate calls over backend-covered string operands
- string field extraction from backend-covered struct parameters when passed to supported string calls
- parameter returns
- local `let` bindings for backend-covered primitive, string, aggregate call, struct field, and struct literal values
- return-position struct literals whose fields are backend-covered primitive, string, or aggregate parameter values
- lossless integer widening on returns, such as `Int32` aliases returning through an `Int` function boundary
- integer `+`, `-`, and `*` over primitive operands
- direct primitive function calls
- declared-call ABI typing for the imported compiler support functions used by `aic_selfhost`; imported definition emission is still tracked by the bootstrap gate
- return-position `if` expressions over primitive comparisons and string equality/inequality
- nested block expression branches with backend-covered local bindings and tail returns
- structured `Result[String, FsError]` matches over filesystem string intrinsics when arms return backend-covered `String`, `Int`, or compatible `Result` values; payload bindings use the actual `Ok(...)`/`Err(...)` names, including wildcard arms
- tail-position `print_str` and `eprint_str` calls lowered to runtime stdout/stderr calls for unit-returning functions
- runtime-backed `std.env.arg_at` option matches in the canonical `Some(value) => value, None => ""` form
- runtime-backed `string.join` over backend-covered `Vec[String]` values, including vectors built with `vec.new_vec` and `vec.push`
- runtime-backed `vec.get` for backend-covered vectors, plus direct `Some`/`None` match-return lowering used by parser cursor helpers
- runtime-backed `std.fs.read_text`, `std.fs.temp_file`, `std.fs.write_text`, and `std.fs.delete` return expressions for the supported `Result[String, FsError]` and `Result[Bool, FsError]` ABI layouts
- generated executable entry points call the runtime stack-limit guard before user code, so large self-host compiler inputs do not depend on shell `ulimit` state on Linux
- explicit `return` statements for backend-supported result expressions, including `return;` in unit functions, branch-local returns in backend-supported block expressions, and non-terminal early returns that terminate the current function body before unreachable tail code
- integer range `for name in start..end { ... }` statements with backend-covered `Int` bounds and unit loop bodies, lowered to explicit LLVM compare/body/increment blocks; unit `continue;` branches to the loop increment block and unit `break;` branches to the loop end block, including block-local terminators before unreachable unit tails
- iterator-style `for name in values { ... }` statements over backend-covered `Vec[Int]` values, lowered through the runtime vector-get probe with deterministic compare/body/increment/end blocks; unsupported iterator operand or element types remain rejected with `E5101`
- backend-covered `while` loop bodies with unit `continue;` and `break;` statements lowered to the condition and end blocks respectively, including block-local terminators before unreachable unit tails
- backend-covered `loop { ... }` expressions, including unit loop statements before unreachable unit tails and typed `break value` exits for backend-covered result types, even when the value-bearing break appears before an unreachable unit tail
- void call expression statements when the callee and arguments are backend-covered, including `write_stdout(result.stdout)` and `write_stderr(result.stderr)` style field accesses
- static literal `match` expressions whose selected arm is known from literal patterns

The backend also preserves deterministic metadata for structs, enums, tuples, generic definitions, generic instantiations, closures, async/future functions, trait and impl declarations, const/global declarations, resource-handle-shaped types, and native-link declarations. These metadata surfaces let parity tools verify coverage without pretending that unsupported executable forms have native code.

## Diagnostics

Backend diagnostics are part of the artifact and suppress LLVM text emission when present.

| Code | Meaning |
| --- | --- |
| `E5101` | Missing executable lowering hook, unsupported executable statement, unsupported return expression, or intentional non-goal such as unsafe blocks, template literals, assert statements, or non-range/non-`Vec[Int]` `for` iterables |
| `E5102` | Unsupported ABI or type form that has no deterministic LLVM layout in the self-host backend |
| `E5103` | Invalid native-link metadata |
| `E5104` | Invalid backend input, including empty artifact names or missing required entry points |
| `E5105` | Native materialization requested from the LLVM-text backend package instead of the driver |

IR input validation diagnostics (`E5010`, `E5011`, and `E5013`) are also propagated before backend emission when schema, source-map, symbol, or type metadata is not usable by the backend. Canonical serialization ordering remains validated by `compiler.ir`.

## Verification

The backend is validated through `compiler/aic/tools/source_diagnostics_check`, `tests/selfhost_parity_tests.rs`, and `tests/selfhost/parity_manifest.json`.

Required local checks for backend changes:

```bash
cargo run --quiet --bin aic -- check compiler/aic/libs/backend_llvm --max-errors 120
cargo run --quiet --bin aic -- check compiler/aic/tools/source_diagnostics_check --max-errors 200
cargo run --quiet --bin aic -- build compiler/aic/tools/source_diagnostics_check
cargo run --quiet --bin aic -- run compiler/aic/tools/source_diagnostics_check
cargo test --locked --test selfhost_parity_tests
make selfhost-parity
make docs-check
make ci
```
