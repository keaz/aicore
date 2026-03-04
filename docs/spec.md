# AICore MVP Spec

## 1. Design goals

- IR-first architecture: canonical IR is source of truth.
- Deterministic formatting and diagnostics.
- No nulls: absence modeled with `Option[T]`.
- Verifiability: static type/effect checks, contracts lowered to runtime checks.
- LLVM native code generation.

## 2. Concrete syntax

Grammar contract version: `mvp-grammar-v6` (see `docs/syntax.md`).

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

- Builtins: `Int`, `Int8`, `Int16`, `Int32`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `Float32`, `Float64`, `Float`, `Bool`, `String`, `()`
- Standard-library binary payload type: `Bytes` (declared in `std.bytes`)
- Named types: `MyType`
- Generic types: `Option[Int]`, `Result[Int, String]`
- Generic arity is checked statically (`Option[Int, Int]` is invalid).
- `Int` is the general integer type and currently uses signed 64-bit range (`-9223372036854775808..=9223372036854775807`).
- Fixed-width integer literals can use suffixes: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`.
- Unsuffixed integer literals default to `Int` unless an expected integer type context narrows them with range validation.
- `Float` is a compatibility alias to `Float64`.
- Unsuffixed float literals default to `Float` (`Float64`) unless expected-type context narrows to `Float32`.
- Float literal suffixes are `f32` and `f64`.
- Mixed-width float operators are rejected unless operands are the same float kind (`Float` and `Float64` are treated as the same kind).

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

intrinsic fn aic_fs_exists_intrinsic(path: String) -> Bool effects { fs };

fn file_exists(path: String) -> Bool effects { fs } {
    aic_fs_exists_intrinsic(path)
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
- `intrinsic fn ...;` declares runtime-bound signature-only APIs (no function body).
- Intrinsic declarations may declare effects and are serialized in IR/JSON with `is_intrinsic` and `intrinsic_abi` metadata.
- Calls to `async fn` produce `Async[T]` values that must be consumed with `await`.
- `await` is only valid inside `async fn`.
- Result propagation uses postfix `?` and requires explicit `Result[_, E]` compatibility.
- Bindings are immutable unless declared with `let mut`.
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

- Literals: `1`, `3.14`, `2.5e-3`, `1.0f32`, `1.0f64`, `true`, `"x"`, `()`
- Calls: `f(x)`
- `if`: `if cond { ... } else { ... }`
- `match`:

```aic
match maybe {
    None | Some(_) if allow_fallback => 0,
    Some(v) => v,
    None => 0,
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

- Mutability and borrow model (MVP):

```aic
fn counter() -> Int {
    let mut x = 1;
    let r = &x;
    x = x + 1;
    x
}
```

Rules:
- `let mut` is required for reassignment.
- `&mut x` requires `x` to be mutable.
- Multiple mutable borrows of the same binding are rejected.
- Assignments while active borrows exist are rejected.
- Borrows are lexical: borrows introduced in nested blocks do not escape that block.
- Or-pattern alternatives must bind the same names with compatible types.
- Match guards must type-check as `Bool`.
- Guarded arms do not count toward exhaustiveness coverage.

### 2.6 Namespaces

- Value namespace: functions.
- Type namespace: `struct` and `enum` names.
- Module namespace: imported module aliases.
- Value/type shadowing with the same identifier is legal.
- Duplicate declarations within the same namespace and module are errors.

### 2.7 Open issue implementation contracts

Detailed AI-agent implementation contracts for open language issues live in:

- `docs/reference/open-issue-contracts.md`

Status summary (`current` -> `target`):

- `#128` tuple types: unit/grouping-only parentheses -> tuple types/literals/patterns/projections.
- `#130` methods: marker-trait impl declarations only -> inherent methods + method call and associated call syntax.
- `#136` trait methods: marker traits only -> trait method signatures/impl bodies with method resolution (static dispatch MVP; dyn dispatch optional).
- `#137` borrow completeness: lexical/local alias checks -> move tracking, cross-call/field-aware borrow reasoning, and stronger ownership diagnostics.
- `#138` generic constraints: inline bounds only -> normalized inline + `where` constraints (with equivalent semantics).
- `#139` inference: limited local inference and explicit closure parameter types -> broader local deterministic inference while keeping function signatures explicit.
- `#317` 128-bit integer primitives: fixed-width family up to 64-bit -> add `Int128`/`UInt128` with deterministic literal/range/type-operator semantics.
- `#318` size-family integers: no dedicated size-typed integer family -> add deterministic `ISize`/`USize` with `UInt` aliasing `USize`.
- `#319` float-width primitives: single `Float` primitive only -> add `Float32`/`Float64` while keeping `Float` as alias to `Float64` with deterministic literal/operator policy.
- `#320` numeric stdlib surface: no dedicated numeric module -> add `std.numeric` for explicit conversion/overflow-policy APIs without changing implicit-cast policy.

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
`aic migrate <path> --dry-run --json` provides deterministic source + IR migration planning.

## 4. Type system

- No general implicit coercions (only lossless integer conversion at typed boundaries).
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
- Pattern-or (`p1 | p2`) is supported with binding-set/type consistency checks.
- Guarded match arms are type-checked but excluded from coverage proofs.

### 4.1 Integer families and ranges

- `Int`, `ISize`, `USize`, and fixed-width integer primitives are distinct source-level integer kinds.
- `UInt` is an alias of `USize`.
- Deterministic Wave 2A policy: `ISize` and `USize` are pinned to 64-bit domains in type-checking/lowering.
- Fixed-width ranges:
  - `ISize`: `-9223372036854775808..=9223372036854775807`
  - `USize` / `UInt`: `0..=18446744073709551615`
  - `Int8`: `-128..=127`
  - `Int16`: `-32768..=32767`
  - `Int32`: `-2147483648..=2147483647`
  - `Int64`: `-9223372036854775808..=9223372036854775807`
  - `Int128`: `-170141183460469231731687303715884105728..=170141183460469231731687303715884105727`
  - `UInt8`: `0..=255`
  - `UInt16`: `0..=65535`
  - `UInt32`: `0..=4294967295`
  - `UInt64`: `0..=18446744073709551615`
  - `UInt128`: `0..=340282366920938463463374607431768211455`
- Integer literal range diagnostics:
  - expression contexts use `E1204`
  - pattern contexts use `E1234`

### 4.2 Integer conversion policy

- There is no explicit numeric cast operator in MVP.
- Implicit integer conversion is permitted only when lossless (source range is fully contained in target range).
- Lossless conversion applies at typed boundaries such as let/assignment/function argument/function return checking.
- `UInt` and `USize` are equivalent for compatibility/operator-kind checks because `UInt` aliases `USize`.
- Non-lossless conversions are rejected with type-mismatch diagnostics (commonly `E1204`).

Examples:

```aic
fn ok(a: Int16) -> Int32 { a }     // lossless widening
fn ok_size(a: UInt32) -> USize { a } // lossless widening into size domain
fn ok_alias(a: USize) -> UInt { a }  // alias-compatible
fn bad(a: Int16) -> UInt16 { a }   // rejected: signed -> unsigned is not lossless
fn bad2(a: Int16) -> Int8 { a }    // rejected: narrowing
```

### 4.3 Integer operator policy

- Arithmetic (`+`, `-`, `*`, `/`, `%`) requires exact integer kind match (signedness + width).
- Bitwise/shift (`&`, `|`, `^`, `<<`, `>>`, `>>>`) requires exact integer kind match.
- Integer equality/comparison (`==`, `!=`, `<`, `<=`, `>`, `>=`) requires exact integer kind match.
- `UInt`/`USize` are treated as the same integer kind for exact-match checks because `UInt` aliases `USize`.
- Mismatches report deterministic diagnostics:
  - `E1230` arithmetic/bitwise/shift mismatch
  - `E1231` equality mismatch
  - `E1232` comparison mismatch
- Integer overflow is not diagnosed by the type system; runtime arithmetic follows backend integer-width semantics.

### 4.4 Float families and literal policy (`#319`)

- `Float32` and `Float64` are distinct source-level float kinds.
- `Float` is a compatibility alias to `Float64`.
- Unsuffixed float literals default to `Float` (`Float64`).
- Float literals may be explicitly suffixed with `f32` or `f64`.
- Expected-type context may type unsuffixed float literals as `Float32` or `Float64`.
- Float operators require exact float-kind match:
  - arithmetic operators (`+`, `-`, `*`, `/`, `%`) use `E1230` on mismatch
  - equality operators use `E1231` on mismatch
  - comparison operators use `E1232` on mismatch
- `Float` and `Float64` are treated as the same kind in exact-match checks because of alias canonicalization.
- No implicit `Int`/float coercions are introduced by this policy.

### 4.5 Migration reference

- Fixed-width migration examples and rollout guidance: `docs/fixed-width-primitives-migration.md`.

### 4.6 Wave numeric expansion contracts (`#317`, `#318`, `#319`, `#320`)

- Current behavior:
  - Built-in integer primitives include `Int`, `ISize`, `USize` (`UInt` alias), `Int8/16/32/64/128`, `UInt8/16/32/64/128`.
  - Literal suffixes include `i8/i16/i32/i64/i128/u8/u16/u32/u64/u128`.
  - Built-in float primitives include `Float32` and `Float64`; `Float` aliases `Float64`.
  - Float literals default to `Float` (`Float64`) and support explicit `f32`/`f64` suffixes.
  - No dedicated `std.numeric` module is part of the documented standard library surface.
- Target behavior (remaining contract scope):
  - Keep `ISize`/`USize` deterministic (64-bit) and keep `UInt` as an alias of `USize`.
  - Keep `Float` as compatibility alias to `Float64` and keep `Float32`/`Float64` mixed-width diagnostics deterministic.
  - Preserve existing implicit conversion policy: only lossless conversions are allowed; there is still no general cast operator.
  - Add `std.numeric` as the explicit numeric-conversion/overflow-policy module (checked/wrapping/saturating style helpers and cross-width conversion helpers), keeping arithmetic type rules deterministic.
  - Keep diagnostic categories stable: out-of-range/type-mismatch diagnostics remain deterministic and continue to use the integer diagnostic families already documented in this spec.

## 5. Effect system

- Default function effect set is empty (pure).
- Known effects: `io`, `fs`, `net`, `time`, `rand`, `env`, `proc`, `concurrency`.
- Calls require callee effects to be subset of caller declared effects.
- Async calls participate in the same explicit effect accounting and transitive effect analysis.
- Effect declarations are canonicalized to deterministic sorted signatures.
- Interprocedural call-graph analysis enforces transitive effect safety with call-path diagnostics.
- Contracts are checked as pure contexts.
- Binary payload APIs use `Bytes` as the transport type (`std.fs.read_bytes/write_bytes/append_bytes`, `std.net.tcp_send/tcp_recv/udp_send_to`, and `std.net.UdpPacket.payload`).
- `std.fs` uses stable `FsError` categories (`NotFound`, `PermissionDenied`, `AlreadyExists`, `InvalidInput`, `Io`) and returns `Result` for fallible operations.
- `std.regex` provides `compile/is_match/find/captures/find_all/replace` APIs with stable `RegexError` categories.

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
- `Int`, `Int8`, `Int16`, `Int32`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `Float32`, `Float64`, `Float`, `Bool`, `String`, `()`
- `std.bytes.Bytes` payload wrapper for binary filesystem/network APIs
- `Option[T]` (core path)
- calls, `if`, `match`, arithmetic/comparison/logical ops
- runtime panic + print helpers
  - filesystem runtime ABI (`read/write/append/copy/move/delete/metadata/walk/temp`)
- Match-or lowers for bool/enum matches.
- Match guards currently emit backend diagnostic `E5023` (frontend check-only support).

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
