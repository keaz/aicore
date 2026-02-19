# AICore MVP Spec

## 1. Design goals

- IR-first architecture: canonical IR is source of truth.
- Deterministic formatting and diagnostics.
- No nulls: absence modeled with `Option[T]`.
- Verifiability: static type/effect checks, contracts lowered to runtime checks.
- LLVM native code generation.

## 2. Concrete syntax

Grammar contract version: `mvp-grammar-v1` (see `docs/syntax.md`).

### 2.1 Modules and imports

```aic
module app.main;
import std.io;
import std.string;
```

- `module` is optional for single-file inputs.
- `import` is explicit and deterministic.

### 2.2 Types

- Builtins: `Int`, `Bool`, `String`, `()`
- Named types: `MyType`
- Generic types: `Option[Int]`, `Result[Int, String]`

### 2.3 Functions

```aic
fn abs(x: Int) -> Int requires true ensures result >= 0 {
    if x >= 0 { x } else { 0 - x }
}

fn read_line() -> String effects { io } {
    "todo"
}
```

- Functions are pure by default.
- Effects are explicit: `effects { io, fs, net, time, rand }`.
- Contracts:
  - `requires <bool-expr>`
  - `ensures <bool-expr>`

### 2.4 ADTs

```aic
struct NonEmptyString {
    value: String,
} invariant len(value) > 0

enum Option[T] {
    None,
    Some(T),
}
```

### 2.5 Expressions

- Literals: `1`, `true`, `"x"`, `()`
- Calls: `f(x)`
- `if`: `if cond { ... } else { ... }`
- `match`:

```aic
match maybe {
    None => 0,
    Some(v) => v,
}
```

## 3. Canonical IR

IR entities use stable IDs:

- `SymbolId(u32)`
- `TypeId(u32)`
- `NodeId(u32)`

Top-level IR shape:

```text
Program {
  module: Option<Vec<String>>,
  imports: Vec<Vec<String>>,
  items: Vec<Item>,
  symbols: Vec<Symbol>,
  types: Vec<TypeDef>
}
```

`aic ir --emit json` prints canonical JSON serialization.

## 4. Type system

- No implicit coercions.
- `Option[T]`/`Result[T, E]` are standard tagged ADTs.
- Match exhaustiveness is enforced for:
  - `Bool`
  - `Option[T]`
  - `Result[T, E]`
  - declared enums

## 5. Effect system

- Default function effect set is empty (pure).
- Known effects: `io`, `fs`, `net`, `time`, `rand`.
- Calls require callee effects to be subset of caller declared effects.
- Contracts are checked as pure contexts.

## 6. Contracts

- `requires` checked at function entry.
- `ensures` checked at function exit (tail-return style in MVP).
- `invariant` validated statically for type and as runtime obligation in planned struct codegen path.
- Static constant simplifier flags always-false contract expressions.

## 7. Diagnostics schema

Each diagnostic includes:

- `code`: stable identifier (e.g. `E2001`)
- `severity`: `error|warning|note`
- `message`
- `spans[]`: `{file,start,end,label?}`
- `help[]`
- `suggested_fixes[]`: `{message,replacement?,start?,end?}`

`aic check --json` and `aic diag --json` return JSON arrays.
Formal schema file: `docs/diagnostics.schema.json`.
Registry and ownership: `docs/diagnostic-codes.md`.

## 8. LLVM backend

- Emits LLVM IR text.
- Compiles with `clang` plus runtime C shim.
- Supported codegen subset:
  - `Int`, `Bool`, `String`, `()`
  - `Option[T]` (core path)
  - calls, `if`, `match`, arithmetic/comparison/logical ops
  - runtime panic + print helpers

## 9. Determinism

- Stable parse/lower traversal.
- Stable format output from IR.
- Stable diagnostic sort order.
- Stable ID allocation policy version `id-policy-v1` (`docs/id-allocation.md`).
