# Type System (MVP)

- Strong static typing.
- No general implicit casts/coercions.
- Types:
  - `Int`, `Int8`, `Int16`, `Int32`, `Int64`, `Int128`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `UInt128`, `ISize`, `USize`, `UInt`, `Float32`, `Float64`, `Float`, `Bool`, `Char`, `String`, `Bytes`, `()`
  - named structs/enums
  - parametric surface syntax for ADTs (`Option[T]`, `Result[T,E]`) and wrapper types (`Async[T]`, `Ref[T]`, `RefMut[T]`)
  - tuple types and literals (`(T, U)`, `(1, "x")`)
  - function types (`Fn(T) -> U`)
  - trait-object types (`dyn Trait`)
- Generic parameters support trait bounds (`T: Trait` and `T: TraitA + TraitB`).
- Function and trait method signatures also accept `where` clauses, and inline bounds are equivalent to `where` bounds.
- Trait bounds are satisfied only through explicit `impl Trait[Type];` declarations.
- Traits can declare methods, and inherent/trait impl blocks can define method bodies.
- Match exhaustiveness checking for Bool/Option/Result/enums.
- Match overlap/dead-arm detection with deterministic diagnostics.
- Pattern bindings are unique within a single pattern tree.
- Pattern-or (`p1 | p2`) alternatives must bind identical name sets.
- Pattern-or bindings must have compatible types across alternatives.
- Match guards (`if <expr>`) must type-check to `Bool`.
- Guarded arms do not satisfy exhaustiveness coverage.
- Tuple projections use numeric field syntax (`.0`, `.1`, ...).
- `await` requires `Async[T]` and is valid only inside `async fn`.
- Result propagation `expr?` requires `expr: Result[T, E]` and enclosing return type `Result[U, E]`.
- `?` never performs implicit error conversion; mismatched `E` types are diagnostics.
- Borrow expressions produce reference wrapper types:
  - `&x` => `Ref[T]`
  - `&mut x` => `RefMut[T]`
- Assignment is type-checked (`name = expr`) and must match binding type.
- Typed holes (`_`) are accepted in type annotations (parameter, return, let, struct-field positions), infer from usage context, and emit warning `E6003` (not a hard error).
- Borrow/alias checks (MVP):
  - mutable borrow requires mutable binding (`E1267`)
  - conflicting mutable/immutable borrows are rejected (`E1263`, `E1264`)
  - assignment while borrowed is rejected (`E1265`)
  - assignment to immutable binding is rejected (`E1266`)
  - borrow target must be a local variable (`E1268`)
- Match-pattern diagnostics:
  - `E1270`: non-`Bool` guard expression
  - `E1271`: or-pattern binding name-set mismatch
  - `E1272`: or-pattern binding type mismatch
- `null` is forbidden; absence is modeled only via `Option[T]`.
- Unknown symbols and type mismatches are reported with structured diagnostics.
- `Float` is a compatibility alias to `Float64`.

## Integer primitives and `Int`

- `Int` is the general integer primitive and currently uses signed 64-bit range:
  - `Int`: `-9223372036854775808..=9223372036854775807`
- Fixed-width primitives:
  - `Int8`: `-128..=127`
  - `Int16`: `-32768..=32767`
  - `Int32`: `-2147483648..=2147483647`
  - `Int64`: `-9223372036854775808..=9223372036854775807`
  - `UInt8`: `0..=255`
  - `UInt16`: `0..=65535`
  - `UInt32`: `0..=4294967295`
  - `UInt64`: `0..=18446744073709551615`
- `Int` and `Int64` currently share 64-bit range, but are separate named source types with explicit typing rules.

### Wave 1 contract (`#317`) for 128-bit primitives

- Current behavior:
  - Fixed-width integer primitives are limited to 8/16/32/64-bit signed and unsigned types.
  - Integer literal suffixes are limited to `i8/i16/i32/i64/u8/u16/u32/u64`.
- Target behavior:
  - Add `Int128` (`-170141183460469231731687303715884105728..=170141183460469231731687303715884105727`).
  - Add `UInt128` (`0..=340282366920938463463374607431768211455`).
  - Add literal suffixes `i128` and `u128`.
  - Keep unsuffixed literals defaulting to `Int` unless expected-type context narrows.

## Integer literal typing and diagnostics

- Unsuffixed integer literals:
  - default to `Int`
  - if an expected integer type exists (annotation/argument/return context), they are checked against that expected type and typed accordingly
- Suffixed integer literals force fixed-width type:
  - signed: `i8`, `i16`, `i32`, `i64`, `i128`
  - unsigned: `u8`, `u16`, `u32`, `u64`, `u128`
- `ISize`/`USize` are type names (not literal suffixes) and follow deterministic 64-bit range policy.
- `UInt` aliases `USize`.
- Range diagnostics:
  - expression literals out of range: `E1204`
  - pattern integer literals out of range: `E1234`
- Lexer diagnostics:
  - invalid integer suffix: `E0009`
  - float literal with integer suffix: `E0010`

Examples:

```aic
let a: UInt8 = 255;   // ok
let b: UInt8 = 256;   // E1204
let c: Int8 = -128;   // ok
let d: Int8 = -129;   // E1204
let e = 1u16;         // e: UInt16
let f: Int32 = 42;    // unsuffixed literal narrows to Int32 in context
```

## Integer conversion policy

- No explicit cast syntax exists in MVP.
- The checker allows only lossless implicit integer conversion where source range is fully contained in target range.
- This policy is used in typed boundaries (for example let/assignment/function argument/function return compatibility checks).
- `UInt` and `USize` are equivalent for compatibility/operator-kind checks because `UInt` is an alias of `USize`.

Examples:

```aic
fn ok_widen(a: Int16) -> Int32 { a }      // allowed
fn ok_u8_to_u16(a: UInt8) -> UInt16 { a } // allowed
fn ok_size(a: UInt32) -> USize { a }      // allowed
fn ok_alias(a: USize) -> UInt { a }       // allowed (alias)
fn bad_narrow(a: Int16) -> Int8 { a }     // E1204
fn bad_sign(a: Int16) -> UInt16 { a }     // E1204
fn bad_u64_to_int(a: UInt64) -> Int { a } // E1204
```

## Integer operator typing semantics

- Arithmetic (`+`, `-`, `*`, `/`, `%`) requires integer operands with exact same signedness and width.
- Bitwise and shift (`&`, `|`, `^`, `<<`, `>>`, `>>>`) require exact same integer signedness and width.
- Equality/comparison on integer operands also require exact integer kind match.
- `UInt`/`USize` are treated as the same integer kind in exact-match checks (alias canonicalization).
- Diagnostic mapping:
  - `E1230`: arithmetic/bitwise/shift operand mismatch
  - `E1231`: equality operand mismatch
  - `E1232`: comparison operand mismatch
  - `E1233`: logical operator requires `Bool`
- No overflow trap diagnostics are emitted by the type checker for integer arithmetic.

Examples:

```aic
fn bad_ops(a: Int8, b: UInt16) -> Int {
    let _x = a + b; // E1230
    let _y = a < b; // E1232
    0
}
```

## Float primitives and compatibility alias

- `Float32` and `Float64` are distinct float primitives.
- `Float` is a compatibility alias of `Float64` and is treated as the same kind in type/operator checks.
- Float operator checks remain deterministic and do not add implicit int/float coercions.

## Float literal typing and diagnostics (`#319`)

- Unsuffixed float literals default to `Float` (`Float64`).
- Float literal suffixes:
  - `f32` forces `Float32`
  - `f64` forces `Float64`
- Expected-type context may type unsuffixed float literals as `Float32` or `Float64`.
- Lexer diagnostics:
  - float literal with integer suffix: `E0010`

Examples:

```aic
let x = 1.5;            // x: Float (alias Float64)
let y = 1.5f32;         // y: Float32
let z: Float32 = 1.25;  // unsuffixed literal narrowed by context
```

## Float operator typing semantics (`#319`)

- Arithmetic (`+`, `-`, `*`, `/`, `%`) requires exact float kind match.
- Equality/comparison on float operands also require exact float kind match.
- `Float` and `Float64` are treated as the same kind in exact-match checks due to alias canonicalization.
- `Float32` with `Float64`/`Float` is rejected without implicit promotion:
  - `E1230`: arithmetic mismatch
  - `E1231`: equality mismatch
  - `E1232`: comparison mismatch

Example:

```aic
fn bad_mixed(a: Float32, b: Float64) -> Int {
    let _x = a + b; // E1230
    0
}
```

## Numeric lowering semantics (summary)

- Signed integer division/modulo use signed operations.
- Unsigned integer division/modulo use unsigned operations.
- Right shift `>>` is arithmetic for signed integers and logical for unsigned integers.
- Unsigned-right shift `>>>` is logical.

## Wave 1 `std.numeric` contract (`#320`)

- Current behavior:
  - Numeric typing/coercion is fully language-level; there is no dedicated `std.numeric` module in documented stdlib APIs.
- Target behavior:
  - Introduce `std.numeric` as the explicit helper surface for numeric conversion and overflow-policy operations.
  - `std.numeric` APIs are additive and do not relax type-checker rules:
    - no new implicit cast behavior
    - integer operator exact-kind requirements remain unchanged
    - diagnostics remain deterministic and continue to use the current mismatch/range code families
  - Preferred migration shape is explicit helper calls at boundaries where widening/narrowing/sign-policy must be stated in source.

See also:

- `docs/llvm-backend.md` for backend/ABI mapping.
- `docs/fixed-width-primitives-migration.md` for before/after migration examples.

## Open issue contracts (current vs target)

Detailed per-issue contracts are tracked in:

- `docs/reference/open-issue-contracts.md`

Type-focused status:

- `#136` trait methods and dispatch
  - Current: trait bounds are marker-only.
  - Target: trait method signatures + impl method conformance + bounded method resolution (static dispatch MVP).
- `#137` borrow checker completeness
  - Current: alias/mutability checks for lexical local borrows (`E1263`-`E1269`).
  - Target: move/use-after-move checks, cross-call borrow reasoning, field-aware ownership checks.
- `#157` deterministic drop ordering
  - Current: runtime-drop locals (`String`, struct, enum) emit reverse-lexical `llvm.lifetime.end` cleanup at scope exits, and compiler-managed resource locals (`FileHandle`, `Map[K, V]`, `Set[T]`, `TcpReader`, `IntChannel`, `IntMutex`) additionally perform real runtime close/cleanup calls on scope exit and early-return paths (`return`, `break`, `continue`, `?`).
  - Current: concrete `Drop` trait implementations (`trait Drop[T] { fn drop(self: T) -> (); }`) are discovered during codegen and dispatched at scope exits in reverse lexical order; moved-out locals suppress destructor dispatch on the moved-from slot.
  - Target: full move-out tracking across complex expressions/control-flow joins, partial-move behavior, and unwind/panic-aware cleanup guarantees.
- `#138` generic constraints and `where`
  - Current: inline bounds (including `+`) only.
  - Target: equivalent constraint model across inline and `where` forms.
- `#139` improved inference
  - Current: local inference with deterministic unresolved failures (`E1204`, `E1212`, `E1280`).
  - Target: stronger local inference (closure-context and usage-driven) with explicit ambiguity diagnostics.
- `#317` fixed-width integer family extension
  - Current: fixed-width family includes `Int128`/`UInt128` and corresponding literal suffixes.
  - Target: preserve deterministic integer conversion/operator diagnostics while extending library-level numeric helpers.
- `#318` size-family integers
  - Current: `ISize`/`USize` are supported with deterministic 64-bit semantics, and `UInt` aliases `USize`.
  - Target: keep this aliasing/stability contract and avoid platform-dependent typechecking behavior.
- `#319` float-width primitives
  - Current: `Float32`/`Float64` are available, with `Float` as alias to `Float64`.
  - Target: keep literal and mixed-width diagnostics deterministic while preserving `Float` compatibility.
- `#320` numeric stdlib module
  - Current: no dedicated `std.numeric` conversion/overflow helper module.
  - Target: add `std.numeric` for explicit numeric conversion and overflow-policy APIs without changing implicit-coercion semantics.

Related syntax issue with type impact:

- `#128` tuple types are not currently available; target adds tuple type/literal/pattern/projection typing rules.
