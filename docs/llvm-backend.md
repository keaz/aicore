# LLVM Backend Overview (E5)

This document describes the current native backend contract implemented in `src/codegen.rs`.

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
- `Bool` -> `i1`
- `()` -> `void`

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
