# AICore MVP Spec

## 1. Design goals

- IR-first architecture: canonical IR is source of truth.
- Deterministic formatting and diagnostics.
- No nulls: absence modeled with `Option[T]`.
- Verifiability: static type/effect checks, contracts lowered to runtime checks.
- LLVM native code generation.

## 2. Concrete syntax

Grammar contract version: `mvp-grammar-v3` (see `docs/syntax.md`).

### 2.1 Modules and imports

```aic
module app.main;
import std.io;
import std.string;
import app.math;

fn main() -> Int {
    math.add(40, 2)
}
```

- `module` is optional for single-file inputs.
- `import` is explicit and deterministic.
- Unqualified symbol lookup is limited to the current module plus directly imported modules.
- Qualified calls use `<module_tail>.<symbol>(...)` (e.g. `math.add(...)`).
- Transitive imports are not implicitly re-exported.

### 2.2 Types

- Builtins: `Int`, `Bool`, `String`, `()`
- Named types: `MyType`
- Generic types: `Option[Int]`, `Result[Int, String]`
- Generic arity is checked statically (`Option[Int, Int]` is invalid).

### 2.3 Functions

```aic
fn abs(x: Int) -> Int requires true ensures result >= 0 {
    if x >= 0 { x } else { 0 - x }
}

fn read_line() -> String effects { io } {
    "todo"
}

async fn fetch_plus_one(x: Int) -> Int {
    x + 1
}

async fn use_fetch() -> Int {
    await fetch_plus_one(41)
}

trait Sortable[T];
impl Sortable[Int];

fn pick[T: Sortable](a: T, b: T) -> T {
    a
}
```

- Functions are pure by default.
- `async fn` declares asynchronous call boundaries.
- Effects are explicit: `effects { io, fs, net, time, rand }`.
- Calls to `async fn` produce `Async[T]` values that must be consumed with `await`.
- `await` is only valid inside `async fn`.
- Result propagation uses postfix `?` and requires explicit `Result[_, E]` compatibility.
- Generic function parameters are inferred from call arguments.
- Generic parameters may include explicit trait bounds: `T: TraitA + TraitB`.
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

### 2.5 Traits and impls

- Marker traits are declared with `trait Name[T, ...];`.
- Concrete implementations are declared with `impl Name[ConcreteType, ...];`.
- Bounded generics are validated at call sites; missing impls are diagnostics.
- Conflicting duplicate impls for the same `(trait, type args)` are rejected deterministically.

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

- Result propagation:

```aic
fn parse_num(x: Int) -> Result[Int, Int] { Ok(x) }

fn bump(x: Int) -> Result[Int, Int] {
    let value = parse_num(x)?;
    if true { Ok(value + 1) } else { Err(0) }
}
```

### 2.6 Namespaces

- Value namespace: functions.
- Type namespace: `struct` and `enum` names.
- Module namespace: imported module aliases.
- Value/type shadowing with the same identifier is legal.
- Duplicate declarations within the same namespace and module are errors.

## 3. Canonical IR

IR entities use stable IDs:

- `SymbolId(u32)`
- `TypeId(u32)`
- `NodeId(u32)`

Top-level IR shape:

```text
Program {
  schema_version: u32,
  module: Option<Vec<String>>,
  imports: Vec<Vec<String>>,
  items: Vec<Item>,
  symbols: Vec<Symbol>,
  types: Vec<TypeDef>
}
```

`aic ir --emit json` prints canonical JSON serialization.
`aic ir-migrate <ir.json>` migrates legacy IR JSON to current schema version.

## 4. Type system

- No implicit coercions.
- `Option[T]`/`Result[T, E]` are standard tagged ADTs.
- `Async[T]` is a compiler-managed type wrapper produced by async calls.
- Trait-bounded generics are checked with explicit impl lookup; no implicit trait satisfaction.
- Local let bindings infer from initializer expressions.
- If inferred types remain unresolved (for example `None` as `Option[<?>]`), explicit annotations are required.
- Generic substitution is enforced across function calls, struct literals, field access, and enum variants.
- Match exhaustiveness is enforced for:
  - `Bool`
  - `Option[T]`
  - `Result[T, E]`
  - declared enums

## 5. Effect system

- Default function effect set is empty (pure).
- Known effects: `io`, `fs`, `net`, `time`, `rand`.
- Calls require callee effects to be subset of caller declared effects.
- Async calls participate in the same explicit effect accounting and transitive effect analysis.
- Effect declarations are canonicalized to deterministic sorted signatures.
- Interprocedural call-graph analysis enforces transitive effect safety with call-path diagnostics.
- Contracts are checked as pure contexts.

## 6. Contracts

- `requires` checked at function entry.
- `ensures` checked at all function exits (explicit `return` and implicit exits).
- Struct `invariant` is validated statically for type and enforced at runtime on construction.
- Restricted static verifier proves/discharges some integer obligations and flags statically false contracts.

## 7. Diagnostics schema

Each diagnostic includes:

- `code`: stable identifier (e.g. `E2001`)
- `severity`: `error|warning|note`
- `message`
- `spans[]`: `{file,start,end,label?}`
- `help[]`
- `suggested_fixes[]`: `{message,replacement?,start?,end?}`

`aic check --json` and `aic diag --json` return JSON arrays.
Parser recovery is enabled at item and statement boundaries, so malformed files can emit multiple diagnostics in one pass.
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
- Stable reproducibility manifest output (`aic release manifest`) for identical source trees.

## 10. Release and Security Ops

- `aic release manifest` / `aic release verify-manifest`: deterministic source-input lock.
- `aic release sbom`: lockfile-derived SBOM generation.
- `aic release provenance` / `aic release verify-provenance`: signed artifact provenance checks.
- `aic release security-audit`: threat-model and workflow hardening checks.
- `aic run --sandbox none|ci|strict`: runtime profile-based resource limits (Linux via `prlimit`).
