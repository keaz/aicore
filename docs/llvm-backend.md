# LLVM Backend Overview (E5)

This document describes the current native backend contract implemented in `src/codegen/mod.rs`.

## Backend Flow

1. Frontend pipeline produces typed IR (`run_frontend`).
2. Contract lowering inserts runtime assertions (`lower_runtime_asserts`).
3. LLVM IR text is emitted (`emit_llvm` or `emit_llvm_with_options`).
4. `clang` compiles LLVM IR and runtime C shim into requested artifact kind.

## Toolchain Strategy (E5-T1)

- Backend uses the system `clang` executable.
- Toolchain probing runs `clang --version` and parses LLVM major.
- Minimum supported LLVM major is `14` (`MIN_SUPPORTED_LLVM_MAJOR`).
- Optional reproducible pin: `AIC_LLVM_PIN_MAJOR=<major>`.

Failure behavior:

- Missing `clang`: actionable error indicating PATH setup.
- Unsupported major: explicit minimum version diagnostic.
- Pin mismatch: explicit expected/detected major diagnostic.

## Core ABI Layouts (E5-T2, E5-T4)

### Primitive mappings

- `Int` -> `i64`
- `Int8` -> `i8`
- `Int16` -> `i16`
- `Int32` -> `i32`
- `Int64` -> `i64`
- `UInt8` -> `i8`
- `UInt16` -> `i16`
- `UInt32` -> `i32`
- `UInt64` -> `i64`
- `Bool` -> `i1`
- `()` -> `void`

Notes:

- `Int` and `Int64` both lower to `i64` in LLVM, but remain distinct named source types for type-checking rules.
- Unsigned primitives share the same LLVM bit-width as signed peers; signedness affects selected operations (`s*` vs `u*`, `ashr` vs `lshr`), not the raw storage width.

### Integer lowering policy (fixed-width)

- Fixed-width primitive identity is preserved in backend typing (`render_type`/`parse_type_repr`/`sig_matches_shape`).
- Integer ops lower with width-aware LLVM ops:
  - add/sub/mul: `add`/`sub`/`mul`
  - signed: `sdiv`/`srem`, signed comparisons (`slt`/`sle`/`sgt`/`sge`)
  - unsigned: `udiv`/`urem`, unsigned comparisons (`ult`/`ule`/`ugt`/`uge`)
  - shifts use arithmetic/logical behavior based on signedness (`ashr` vs `lshr`).
- `>>>` always lowers to logical right shift (`lshr`).

### Integer coercion policy in codegen

- When typed values cross ABI/helper boundaries with different integer widths, codegen emits explicit casts:
  - widening signed: `sext`
  - widening unsigned: `zext`
  - narrowing: `trunc`
- This keeps source fixed-width behavior stable even when a runtime helper uses `i64` transport values.
- Example: typed `std.buffer` read/write/patch helpers cast between runtime `i64` slots and declared `Int16`/`UInt32`/etc payload types.

### Extern C ABI policy

- Extern wrappers use exact declared primitive widths at the LLVM boundary.
- Supported extern C scalar primitives in MVP type-checking include:
  - `Int`, `Int8`, `Int16`, `Int32`, `Int64`
  - `UInt8`, `UInt16`, `UInt32`, `UInt64`
  - `Bool`, `Float`, `Char`, `()`
- Extern declarations are still restricted to plain signatures (`extern "C" fn ...;`) without async/generics/effects/contracts.

### Runtime scalar ABI policy

- LLVM bridge types remain explicit (`i8/i16/i32/i64`) at function boundaries.
- Runtime C entrypoints representing scalar values should use fixed-width C types (`int8_t`/`uint8_t`/`int16_t`/`uint16_t`/`int32_t`/`uint32_t`/`int64_t`/`uint64_t`) rather than platform-dependent `long`/`int`.
- Size/capacity/index runtime fields remain `i64`/`int64_t` in LLVM ABI and are range-checked in runtime helpers.

### String ABI (`ptr-len-cap`)

- AIC `String` -> `{ i8*, i64, i64 }`
- Runtime receives string values as scalar args: `(ptr, len, cap)`

Runtime helpers:

- `aic_rt_print_str(i8*, i64, i64)`
- `aic_rt_strlen(i8*, i64, i64)`

### Vec ABI helpers

Runtime helper declarations are present for ptr-len-cap vectors:

- `aic_rt_vec_len(i8*, i64, i64)`
- `aic_rt_vec_cap(i8*, i64, i64)`

### Struct layout

Structs are lowered as positional LLVM aggregates in declared field order.

Example:

- `struct Pair { left: Int, right: Int }` -> `{ i64, i64 }`

### Enum / ADT layout

Enums are lowered to a tagged aggregate:

- field 0: `i32` tag
- one payload slot per variant in declaration order
- payload-less variants use `i8` placeholder

Example:

- `enum Wrap[T] { Empty, Full(T) }` with `T=Pair` -> `{ i32, i8, { i64, i64 } }`

Built-in templates are included:

- `Option[T]` -> variants `None`, `Some(T)`
- `Result[T, E]` -> variants `Ok(T)`, `Err(E)`

Match lowering notes:

- Bool/enum `match` supports top-level or-pattern alternatives (`p1 | p2`).
- Guarded arms (`pattern if cond => ...`) are currently frontend-only; backend emits `E5023`.

## Generic Monomorphization (E5-T3)

- Frontend typecheck records deterministic generic instantiations in IR.
- Codegen builds per-function concrete instances from this metadata.
- Specialized symbols use stable mangled names (`aic_<instantiation-mangle>`).
- Duplicate instantiations are deduplicated by mangled key.

## Artifact Modes (E5-T5)

CLI supports:

- `aic build --artifact exe` (default)
- `aic build --artifact obj`
- `aic build --artifact lib`

Artifact behavior:

- `exe`: compiles module IR + runtime C and links executable.
- `obj`: emits module object only (no runtime object linked in).
- `lib`: archives module object + runtime object into static library.
- `exe` + `--target wasm32`: emits standalone `.wasm` with host-import runtime bindings (no runtime C object linked).

## Optimization Levels (`[PERF-T1]`)

`aic build` now supports optimization-level selection:

- `--opt-level <LEVEL>` where `<LEVEL>` is `0|1|2|3` or `O0|O1|O2|O3`
- short form: `-O0`, `-O1`, `-O2`, `-O3`
- `--release` defaults to `O2` unless explicitly overridden by `--opt-level`/`-O`

Examples:

```bash
# debug-oriented build
cargo run --quiet --bin aic -- build examples/core/opt_levels_demo.aic -O0

# release-oriented default (O2)
cargo run --quiet --bin aic -- build examples/core/opt_levels_demo.aic --release

# explicit max optimization
cargo run --quiet --bin aic -- build examples/core/opt_levels_demo.aic --release -O3
```

Validation references:

- `tests/e7_cli_tests.rs` (flag propagation + release default)
- `tests/execution_tests.rs` (semantic equivalence across O0..O3 + O2 vs O0 speed check)
- `examples/core/opt_levels_demo.aic`

## Cross-Compilation Targets (`--target`)

`aic build` now accepts explicit target labels:

- `x86_64-linux` -> `x86_64-unknown-linux-gnu`
- `aarch64-linux` -> `aarch64-unknown-linux-gnu`
- `x86_64-macos` -> `x86_64-apple-darwin`
- `aarch64-macos` -> `arm64-apple-darwin`
- `x86_64-windows` -> `x86_64-pc-windows-msvc`
- `wasm32` -> `wasm32-unknown-unknown`

Examples:

```bash
# Explicit host target build
cargo run --quiet --bin aic -- build examples/core/cross_compile_targets.aic --target aarch64-macos

# Build object/library artifacts for a non-host target
cargo run --quiet --bin aic -- build examples/e5/object_link_main.aic --artifact obj --target x86_64-linux
cargo run --quiet --bin aic -- build examples/e5/object_link_main.aic --artifact lib --target x86_64-linux

# Build WebAssembly module
cargo run --quiet --bin aic -- build examples/interop/wasm_hello_world.aic --target wasm32
```

When `--target` is omitted, `aic` uses the host build target.

CI verification for this surface runs:

- host executable smoke build with explicit `--target`
- cross-target object smoke builds for native target labels on each runner OS
- wasm example build + artifact validation (`.wasm` magic bytes, manifest target label, runtime import symbols)

## WebAssembly Runtime Strategy (`--target wasm32`)

`aic build --target wasm32` uses a wasm-specific executable pipeline:

- clang target triple: `wasm32-unknown-unknown`
- linker flags: no entrypoint CRT startup (`--no-entry`)
- no libc/runtime C shim requirement (`-nostdlib`)
- exports: `main` and `aic_main`
- unresolved runtime symbols are intentionally kept and imported from host (`--allow-undefined`)

Current wasm constraints:

- supported artifact kind: `exe` only (`obj`/`lib` rejected for wasm target)
- workspace builds are rejected for wasm target
- `--static-link` is not supported on wasm target

Behavioral notes:

- pure programs (`fn main() -> Int { ... }`) compile to `.wasm` without `aic_rt_*` imports
- IO/runtime-backed programs import runtime symbols (for example `aic_rt_print_str`) from the embedding host

## Static Link Mode (`--static-link`)

`aic build --static-link` enables static linking for executable artifacts.

- Supported today: linux targets (`x86_64-linux`, `aarch64-linux`)
- Not supported: non-executable artifacts (`obj`, `lib`)

Example:

```bash
cargo run --quiet --bin aic -- build examples/core/cross_compile_targets.aic --target x86_64-linux --static-link
```

## Panic ABI and Source Mapping (E5-T6)

Panic runtime signature:

- `aic_rt_panic(i8* ptr, i64 len, i64 cap, i64 line, i64 column)`

Codegen computes `line`/`column` from source spans and passes them for:

- explicit `panic("...")`
- lowered contract/assert failure paths

Runtime panic output includes location when available, for example:

- `AICore panic at 4:11: <message>`

## Debug Metadata Mode (E5-T6)

Use `--debug-info` on `aic build` to enable debug metadata emission and `clang -g` compilation.

Example:

```bash
cargo run --quiet --bin aic -- build examples/e5/panic_line_map.aic --debug-info -o panic_dbg
```

When enabled, emitted IR includes:

- `!DICompileUnit`
- `!DISubprogram`
- `!DILocation` on panic callsites

Default (without `--debug-info`) remains unchanged and omits debug metadata.

## Examples

- `examples/e5/hello_int.aic`
- `examples/e5/enum_match.aic`
- `examples/e5/generic_pair.aic`
- `examples/e5/string_len.aic`
- `examples/e5/object_link_main.aic`
- `examples/e5/panic_line_map.aic`
- `examples/interop/wasm_hello_world.aic`
