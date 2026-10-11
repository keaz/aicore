# Diagnostic Code Registry (MVP)

AICore diagnostics use stable codes in `E####` format.

Registry source of truth:

- `diagnostic registry` (`REGISTERED_DIAGNOSTIC_CODES`)

Per-code reference: [`docs/errors/catalog.md`](errors/catalog.md) gives every registered code a
precise description, a minimal trigger program and a fixed program (or, for codes that one source
file cannot produce, the command or fixture that produces them). `tests/diagnostic_catalog_tests.rs`
runs every trigger and fix, so the catalog is the authoritative per-code text; this page lists ranges,
policy and notable codes.

Codes that are registered but not reported by the Rust compiler:

- Reserved (never emitted): `E0002`, `E0003`, `E1056`, `E1057`, `E1207`.
- Retired: `E6007`.
- Unreachable backstops (`internal` in the catalog): for example `E0007`, `E1061`, `E1099`, `E1206`,
  and the code generation checks the type checker always pre-empts.
- Self-host only: `E1042`, `E1060` and `E1071` are emitted only by the self-host parser (with self-host
  meanings), and `E5101`-`E5105`, `E5200`-`E5205` only by the self-host backend and driver.

## Subsystem ranges

- `E0001-E0099`: lexer
- `E1001-E1099`: parser
- `E1100-E1199`: resolver
- `E1200-E1399`: type checker and exhaustiveness
- `E2001-E2099`: effect checker
- `E2100-E2199`: package/module loading and lockfile workflow
- `E4001-E4099`: contracts
- `E5001-E5099`: LLVM/codegen/runtime lowering
- `E6001-E6099`: std compatibility and deprecation policy

## Enforcement

- `Diagnostic::error` validates code format and registry membership.
- New diagnostics must add a code to `diagnostic registry`.
- Unit tests fail if emitted codes are not registered.
- `aic explain <CODE>` provides deterministic remediation guidance for all registered codes.

Recent core-language additions:

- `E0004`: malformed integer literal (`0x` with no digits, or a value out of `i64` range without an `i128`/`u128` suffix).
- `E0005`: unsupported escape sequence or malformed `\u{...}` escape in a string, char or template literal.
- `E0006`: string, char or template literal not closed before the end of the line or file.
- `E0007`: float literal text that does not convert to `f64`. Unreachable from source: the lexer only collects text Rust always parses.
- `E0009`: integer literal suffix other than `i8`-`i128` / `u8`-`u128`.
- `E0010`: float literal suffix other than `f32` / `f64`.
- `E0008`: invalid char literal (must contain exactly one Unicode codepoint).
- `E1052`: invalid `async` item form (expected `async fn`).
- `E1053-E1059`: trait/impl header syntax (`E1053` missing trait name, `E1054` missing `;`/`{` after a trait header, `E1055` body-less impl head that is not a nominal type, `E1058` missing `;` after a body-less impl, `E1059` missing trait bound after `:`/`+`); `E1056`-`E1057` are reserved.
- `E1062`: assignment statement not terminated by `;`. `E1060`/`E1061` are unreachable in the Rust parser, which only enters assignment parsing after seeing a place and an assignment operator (the self-host parser uses `E1060` for an invalid type-reference token).
- `E1063-E1068`: `extern`/`unsafe` parsing and declaration form errors.
- `E1084`: associated type (`type X;`) in a trait or impl block is not supported yet.
- `E1085`: associated constant (`const X: T;`) in a trait or impl block is not supported yet.
- `E1093`: invalid `intrinsic fn` declaration form (signature-only rules).
- `E1094`: malformed source attribute delimiter or missing attribute name.
- `E1095`: malformed source attribute argument list or argument value.
- `E1096`: framework source attribute target, duplicate, or route/filter/validation shape error.
- `E1097`: framework source attribute argument or generated validation rule error.
- `E1098`: default method body in a trait block is not supported yet; the signature is kept.
- `E1099`: parser stopped making progress and hit its token-proportional step budget. This is a compiler bug; parsing ends with this diagnostic instead of hanging.
- `E1069-E1074`: `Fn(...) -> ...` type and closure literal parsing errors (`E1071` is unreachable in the Rust parser; the self-host parser uses it for a missing `:` after a parameter name).
- `E1075`-`E1077`: type alias missing its name, `=`, or `;`. `E1078`-`E1081`: const missing its name, `:` type, `=`, or `;`.
- `E1082`: trait/impl block member that does not start with `fn`, `async fn` or `unsafe fn`.
- `E1083`: `&` / `&mut` receiver not followed by `self`.
- `E1086`: trait or impl block not closed by `}`.
- `E1087`: impl block with a body whose target is `()`, `_` or `dyn Trait` instead of a named type.
- `E1088`: `::` in a path expression not followed by an identifier.
- `E1089`: trait method signature with `requires`/`ensures`, or without its terminating `;`.
- `E1092`: positional call argument after a named argument.
- `E1100`: duplicate symbol in one module namespace. `E1101`: duplicate struct field. `E1102`: duplicate enum variant.
- `E1106`: inherent `impl` for an undeclared type.
- `E1107`: duplicate method in a trait, or a trait impl that is missing trait methods.
- `E1108`: trait impl method whose signature or receiver differs from the trait, or a method defined twice in one impl.
- `E1109`: trait impl method that the trait does not declare.
- `E1103`: unknown trait referenced in `impl`.
- `E1104`: trait impl arity mismatch.
- `E1105`: conflicting duplicate trait impl.
- `E1228`: unknown type name in a signature, field, payload, alias, annotation, impl header, or trait method signature; also an unknown struct field or out-of-range tuple index, an unknown method for the receiver type, an unknown `dyn` trait in a method call, or a `for` source without `iter()`/`next()`. For unknown type names the help line suggests the closest known type (`I32` -> `Int32`, `Strng` -> `String`, `bool` -> `Bool`).
- `E1256`: `await` used outside an async function.
- `E1290`: non-exhaustive match over a tuple, struct, integer, string, char, or other type outside the `Bool`/`Option`/`Result`/enum codes (`E1246`-`E1249`); the message lists witness patterns such as `(Red, false)` or `_`.
- `E1289`: a closure assigns to, mutably borrows, or calls a `&mut self` method on a variable captured from an enclosing scope. Closures capture by value, so the change would be lost.
- `E1279`: `return` outside a function or closure body (contract expressions, constant initializers).
- `E1291`: `==`/`!=` on a type without equality (only integers, floats, `Bool`, `Char`, `String` support it), or a generic call that instantiates a compared type parameter with such a type.
- `E1292`: `Map`/`Set` key or `Map` value type the runtime cannot store (keys and values: `String`, `Bytes`, `Int`, `UInt64`, `Bool`).
- `E1293`: polymorphic recursion: a generic function calls itself with a type argument that grows (`f[T]` -> `f[Option[T]]`).
- `E1294`: recursive struct or enum (contains itself directly or through other types); not supported yet.
- `E1295`: invalid entry `main` signature (type parameters, parameters, or a return type other than an integer type, `Bool`, or `()`).
- `E1296`: a typed hole (`_`) whose type cannot be inferred, for example a parameter of a function that is never called.
- `E1297`: invalid assignment place: a field of a value without fields, an index into a value that is not a `Vec`, or a `Vec` index that is not `Int`.
- `E1298`: assignment through a `Ref`/`RefMut` value (`r.x = 1` where `r: RefMut[P]`); writes through references are not supported.
- `E1299`: an `async` method declares a `&mut self` receiver.
- `E1303`: a `&mut self` method used as a function value (`let f = C::bump;`).
- `E1300`: call to `print_int`, `print_str`, `print_float` or `panic` from a module that does not import `std.io`.
- `E1301`: call to `len` from a module that does not import `std.string`.
- `E1288`: const type/initializer mismatch, or a const without an initializer.
- `E1313`: a std type (`Vec`, `Map`, `Set`, `Deque`, `Bytes`, `ByteBuffer`, `JsonValue`) is used in a type position, but its module (`std.vec`, `std.map`, `std.set`, `std.deque`, `std.bytes`, `std.buffer`, `std.json`) is not loaded. These types are not prelude types: code generation takes their layout from the std module. The help line names the `import` to add. A module loaded through another import (for example `std.vec` through `std.string`) is enough, and a user struct, enum, alias, or generic parameter with the same name is accepted.
- `E1304`: an index expression on a type that cannot be indexed, or indexing / a list literal without importing the std module that provides it.
- `E1306` (parser): a loop label not followed by `:` and `while`, `loop` or `for`.
- `E1307`: `break 'label` / `continue 'label` where no enclosing loop has that label.
- `E1308`: a loop label that an enclosing loop already uses.
- `E1305`: a numeric conversion that is not available: `x as T` that could change a value (narrowing, sign change, `Int64` to `Float64`, float to integer) or involves a non-numeric type, or a conversion method that does not exist for its target (`to_int`, `checked_to_f64`, `wrapping_to_` with floats).
- `E1302`: the `else` block of `let PATTERN = expr else { ... };` does not diverge (it must end in `return`, `break`, `continue` or `panic(...)`).
- `E1110` (parser): refutable pattern in a plain `let` (`let Some(x) = o;`); use `let ... else` or `if let`.
- `E1257`: `await` operand is not `Async[T]`.
- `E1258`: generic trait bound (including the `Send` marker bound) not satisfied by the concrete type.
- `E1259`: generic bound names an unknown trait or a trait whose arity is not 1.
- `E1260`: `?` operand is not `Result[T, E]`.
- `E1261`: `?` used outside a function body or in a function without a `Result` return type.
- `E1262`: `?` error type mismatch (`Result[_, E1]` in `Result[_, E2]` function).
- `E1263`: conflicting mutable borrow (`&mut x` or a `&mut self` method call while another borrow is active).
- `E1264`: immutable borrow while mutable borrow is active.
- `E1265`: assignment while borrow is active.
- `E1266`: assignment to, or `&mut self` method call on, a binding not declared `let mut` (or `self` without a `&mut self` receiver).
- `E1267`: mutable borrow of immutable binding.
- `E1268`: borrow operand is not a local variable or field path (for example `&(1 + 2)`).
- `E1269`: assignment type mismatch.
- `E1270`: match guard expression must be `Bool`.
- `E1271`: or-pattern alternatives bind different variable sets, or an or-pattern has fewer than two alternatives.
- `E1272`: or-pattern alternatives bind a variable with incompatible types.
- `E1273`: `while` condition must be `Bool`.
- `E1274`: `break` expression type does not match enclosing loop break type (a `while` loop's break type is `()`).
- `E1275`: `break` used outside loop context.
- `E1276`: `continue` used outside loop context.
- `E1277`: use of moved value.
- `E1278`: move attempted while an overlapping borrow is active.
- `E1280`: closure parameter type must be explicit.
- `E1281`: closure body type does not match declared closure return type.
- `E1282`: generic function value cannot be used without specialization.
- `E1283`: effectful function cannot be converted to first-class function value.
- `E1284`: closure parameter count mismatches expected `Fn(...) -> ...` context.
- `E1285`: closure parameter type mismatches expected `Fn(...) -> ...` context.
- `E1286`: closure return type mismatches expected `Fn(...) -> ...` context.
- `E2100`: import names a module that does not resolve to any file.
- `E2101`: an imported (non-entry) file has no `module ...;` declaration.
- `E2102`: symbol not accessible: a private item or field of another module, a module qualifier that is not directly imported, or a user-written call to a runtime intrinsic.
- `E2103`: module import cycle (including a module importing itself).
- `E2104`: ambiguous name: an unqualified enum variant declared by several enums, a callable exported by several imported modules, or two imports with the same alias.
- `E2105`: one module name declared by more than one file.
- `E2106`: `aic.lock` does not match the lockfile computed from `aic.toml`.
- `E2107`: locked path dependency missing or its checksum differs from `aic.lock`.
- `E2108`: `--offline` without `aic.lock`, or a locked dependency missing from the offline cache.
- `E2109`: offline cache checksum mismatch for a locked dependency.
- `E2110`: invalid package install spec or version requirement.
- `E2111`: package manifest read/write error for registry workflows.
- `E2112`: duplicate package version publish attempted.
- `E2113`: package metadata missing/invalid (`[package].name` or `[package].version`).
- `E2114`: semantic version resolution conflict across install requirements.
- `E2115`: package/version not found in configured registry.
- `E2116`: registry/index/package content IO or integrity failure.
- `E2117`: private registry authentication missing or invalid.
- `E2118`: registry configuration or credential source is invalid.
- `E2119`: trust policy denied package install (allow/deny/signature policy).
- `E2120`: invalid or unsupported extern ABI declaration.
- `E2121`: extern or `intrinsic fn` signature uses unsupported language features (generics, async, effects/capabilities, contracts, or a missing runtime ABI).
- `E2122`: call to an extern or `unsafe fn` outside an `unsafe` boundary.
- `E2123`: unsupported type in extern C-ABI signature.
- `E2124`: package signature verification/trusted-key validation failure.
- `E2125`: workspace manifest/member metadata is invalid or inconsistent.
- `E2126`: workspace package dependency cycle detected.
- `E4001`: `requires` proved always false. `E4002`: `ensures` proved always false. `E4004`: struct `invariant` proved always false.
- `E4003` (note): contract neither proved nor refuted; kept as a runtime assertion. `E4005` (note): contract proved at compile time; no runtime check is emitted.
- `E5021`: backend lowering failure for invalid `?` operand/result layout.
- `E5022`: backend lowering failure for incompatible function `Result` return layout.
- `E5023`: backend const evaluation could not evaluate an initializer: a cyclic const dependency, a missing initializer, or an operand failure such as division by zero (unsupported initializer forms are rejected earlier as `E1287`).
- `E5024`: backstop for an extern function with a non-`C` ABI or no native symbol name (the checker reports `E2120` first).
- `E5025`: backend encountered `break` outside a loop.
- `E5026`: backend encountered `continue` outside a loop.
- `E5031`: a closure captures a name that is not a local where the closure is created (currently reached when a closure body calls a closure defined inside that body).
- `E5032`: indirect call target is not a first-class `Fn(...) -> ...` value.
- `E5033`: closure parameter type is missing during backend lowering.
- `E5034`: backend cannot lower the referenced function as a first-class function value.
- `E5035`: closure helper return type mismatch during backend lowering.
- `E5036`: JSON encode/decode for function values is unsupported.
- `E5037`: generic instantiation exceeds the nesting depth limit (12) because of mutual polymorphic recursion that `E1293` does not catch.
- `E6003`: typed hole (`_`) warning with inferred type/context.
- `E6004`: unused import warning with safe remove-import autofix.
- `E6005`: unreachable/unused function warning.
- `E6006`: unused variable warning with safe underscore-prefix autofix.
- `E6007`: historical Windows build target net/TLS strategy guard (retained for compatibility; not emitted after Windows parity landed).

## Check/build parity

A program that passes `aic check` must also build. `tests/check_build_parity_tests.rs` checks every `.aic` source under `examples/` and `tests/`: each one that passes the checker must lower through code generation without an `E5xxx` error. The only codegen errors that may follow a clean `aic check` are:

| Code | Why the checker cannot catch it |
|---|---|
| `E5037` | Mutual polymorphic recursion (`f[T]` -> `g[Option[T]]` -> `f`) needs whole-program instantiation; the checker catches the direct form (`E1293`). |

Any other `E5xxx` after a clean check is a compiler bug: add a checker rule (or fix code generation) and keep the gate green.

### Known parity gaps

The corpus gate above passes, but these single-file programs pass `aic check` and then fail in code generation. They are open compiler bugs, not design: each is a `build` row in [`docs/errors/catalog.md`](errors/catalog.md), so `tests/diagnostic_catalog_tests.rs` fails as soon as a fix makes the checker report the program instead. When you close a gap, move that catalog row to the checker code (or to `internal`) and remove it from this table.

| Code | Program the checker accepts |
|---|---|
| `E5002` | Unary `-` on a `UInt` value (the checker rejects it for `UInt8`/`UInt64`, not for the `UInt` alias). |
| `E5004` | `Int` literal default on a sized-integer struct field (`struct S { x: Int8 = 1 }`). |
| `E5006` | Const `String` equality or const division by zero (`const A: Bool = "a" == "b";`). |
| `E5007` | Sized-integer const with an `Int` literal (`const A: Int8 = 1;`). |
| `E5010`, `E5011` | A user file that declares a `std.*` module and defines a function named like a runtime builtin (`poll_value_task`) with the wrong arity or types. |
| `E5012` | A closure body that calls a closure-valued variable captured from the enclosing function. |
| `E5019` | Cyclic type aliases that are used (`type A = B; type B = A;`). |
| `E5020` | A user-declared `intrinsic fn` with no runtime lowering. |
| `E5023` | Cyclic const dependencies (`const A: Int = B; const B: Int = A;`). |
| `E5031` | A closure body that defines a nested closure and calls it. |
| `E5036` | `json.encode` on a closure value. |

## IO + Runtime Quick Reference

The table below captures high-frequency IO/runtime diagnostics with deterministic trigger guidance from the current implementation.

| Code | Trigger (current behavior) | Deterministic remediation |
|---|---|---|
| `E2001` | Effectful call is used without required `effects { ... }` declaration. | Add the missing effects to the enclosing function signature. |
| `E2002` | An effectful call appears inside a contract (`requires`/`ensures`/invariant), which must remain pure. | Remove side effects from contracts; move checks into executable code paths. |
| `E2003` | Unknown effect name in a function signature. | Use only known effects: `io, fs, net, time, rand, env, proc, concurrency`. |
| `E2004` | Duplicate effect listed in one function signature. | Remove duplicates; keep one declaration per effect. |
| `E2005` | Transitive effect required through call graph but not declared at caller boundary. | Declare the transitive effect at the root function or refactor call boundaries. |
| `E2006` | Resource protocol violation: an operation on a handle binding after a terminal operation (such as close) on it. | Recreate resource before reuse or reorder close/use lifecycle. |
| `E2007` | Unknown capability name in a function signature. | Use only known capabilities: `io, fs, net, time, rand, env, proc, concurrency`. |
| `E2008` | Duplicate capability listed in one function signature. | Remove duplicates; keep one declaration per capability. |
| `E2009` | Missing capability authority for declared or transitive effects. | Add `capabilities { ... }` to match required effect authority and thread capability boundaries across callers. |
| `E5023` | Backend const evaluation hit a const dependency cycle, a missing initializer, or an operand failure such as division by zero. | Break the constant dependency cycle and keep initializers to evaluable literal/arithmetic forms. |
| `E5024` | Backstop for an extern function with a non-`C` ABI or no native symbol name; the checker reports `E2120` first. | Use `extern \"C\"` plain signatures and wrapper functions. |
| `E5025` | `break` reached backend outside loop context. | Ensure `break` is only emitted inside `loop`/`while`. |
| `E5026` | `continue` reached backend outside loop context. | Ensure `continue` is only emitted inside `loop`/`while`. |
| `E5101` | Self-host LLVM backend has no executable lowering hook for the IR statement or expression shape. | Keep backend-covered executable functions to supported primitive forms or implement the missing lowering case. |
| `E5102` | Self-host LLVM backend received an unsupported ABI or type form. | Use primitive backend-covered function parameters/returns or add aggregate ABI lowering. |
| `E5103` | Self-host LLVM backend received invalid native-link metadata. | Preserve non-empty native link names and required object/search-path paths from the package manifest or driver. |
| `E5104` | Self-host LLVM backend received invalid artifact input, such as an empty artifact name or missing required entry point. | Provide a deterministic artifact name and lower a checked program with the requested entry point. |
| `E5105` | Native object, archive, or executable materialization was requested from the LLVM-text backend package. | Emit LLVM text through `compiler.backend_llvm`, then let the driver materialize native artifacts. |
| `E5200` | Self-host driver received an unsupported command or malformed command shape. | Use `check`, `ir <path> --emit json`, `build <path> -o <artifact>`, or `run` when the documented driver mode supports it. |
| `E5201` | Self-host driver could not parse an entry program from the provided source set. | Provide a readable `.aic` file or package manifest with a main source file. |
| `E5202` | `aic_selfhost` could not read the requested source input. | Check the file or package path and filesystem permissions. |
| `E5203` | `aic_selfhost build` could not write the requested backend artifact. | Check the output directory and filesystem permissions. |
| `E5204` | `aic_selfhost` could not materialize a native executable from the self-host LLVM artifact. | Ensure `clang` is available and the backend-covered program emitted valid LLVM IR. |
| `E5205` | Low-level self-host driver run execution is not enabled for direct library calls. | Use the `aic_selfhost run` executable path, which materializes and runs backend-covered programs. |
| `E6001` | Deprecated std API usage warning (for example `std.time.now`). | Migrate to replacement API shown in diagnostic help (for example `std.time.now_ms`). |
| `E6002` | `aic std-compat --check` detected baseline incompatibility. | Keep compatibility (or deprecate first), then regenerate baseline only for intentional additive API change. |
| `E6003` | Typed hole (`_`) was accepted and inferred from context. | Replace `_` with the inferred concrete type when finalizing API/contracts. |
| `E1296` | Typed hole (`_`) type could not be inferred, so the program cannot be built. | Write the concrete type, or call the function/use the field so the type can be inferred. |
| `E1297` | Invalid assignment place (`x.f` on a value without fields, `p[i]` on a non-`Vec`, non-`Int` index). | Assign to a struct field, tuple element (`t.0`) or `Vec` element with an `Int` index. |
| `E1298` | Assignment through a `Ref`/`RefMut` value. | Assign to a `let mut` binding, or to `self` in a `&mut self` method. |
| `E1299` | `async` method with a `&mut self` receiver. | Take `self` and return the updated value. |
| `E1303` | `&mut self` method used as a function value. | Call the method directly or wrap the call in a closure. |
| `E1304` | Index expression on a non-indexable type, or indexing / list literal without the std module import. | Index a `Vec`, `Map`, `String` or `Bytes`, and import its std module (`import std.vec;`). |
| `E1306` | Loop label not followed by `:` and a loop. | Write `'name: while ...`, `'name: loop ...` or `'name: for ...`. |
| `E1307` | `break`/`continue` label names no enclosing loop. | Label a surrounding loop or remove the label. |
| `E1308` | Loop label already used by an enclosing loop. | Give the inner loop a different label. |
| `E1305` | Lossy or non-numeric `as`, or an unavailable conversion method. | Use `checked_to_*` (Option), `saturating_to_*` or `wrapping_to_*` for integers, `to_f32` / `to_f64` for floats. |
| `E1302` | `let ... else` block does not diverge. | End the block with `return`, `break`, `continue` or `panic(...)`. |
| `E1110` | Refutable pattern in a plain `let`. | Use `let PATTERN = expr else { return ...; };` or `if let`. |
| `E6004` | Import was declared but never used. | Remove the import or use a symbol from that module. |
| `E6005` | Function is unreachable from entrypoint or otherwise unused. | Remove dead function code or invoke it from live call paths. |
| `E6006` | Local variable is never used. | Prefix with `_` to mark intentional non-use, or remove the binding. |
| `E6007` | Historical compatibility code for the retired Windows net/TLS build guard. Current builds should not emit it after Windows parity landed. | If it appears, you are likely using stale tooling or stale generated artifacts; rebuild with the current compiler/runtime. |

## Change policy

- Never reuse a retired code for a different semantic error.
- Avoid deleting codes once published; keep compatibility for tooling.
- Diagnostic code changes are breaking for machine consumers and require migration notes.
