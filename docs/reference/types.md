# Types Reference

See also: [Syntax](./syntax.md), [Generics](./generics.md), [Memory](./memory.md), [Expressions](./expressions.md)

This page describes the surface type grammar and the type relations enforced in `compiler/aic/libs/typecheck/src/main.aic`.

## Grammar

```ebnf
type           = unit_type | tuple_type | named_type | dyn_type | fn_type ;
unit_type      = "(" ")" ;
tuple_type     = "(" type ("," type)+ ","? ")" ;
named_type     = type_name type_args? ;
dyn_type       = "dyn" type_name ;
type_name      = ident ("::" ident)* ;
type_args      = "[" type ("," type)* ","? "]" ;

fn_type        = "Fn" "(" type_list? ")" "->" type ;
type_list      = type ("," type)* ","? ;
```

## Semantics and Rules

- Primitive built-in types include `Int`, `Int8`, `Int16`, `Int32`, `Int64`, `Int128`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `UInt128`, `ISize`, `USize`, `UInt`, `Float32`, `Float64`, `Float`, `Bool`, `Char`, `String`, `Bytes`, and `()`.
- User-defined nominal types come from `struct` and `enum` declarations.
- Every type name in a function signature, `let` or closure annotation, struct field, enum payload, type alias, impl header, or trait method signature must name a built-in type, a generic parameter of the enclosing declaration (`Self` inside traits and impls), or a visible struct, enum, trait, or type alias. Unknown names fail `aic check` with `E1228` and a "did you mean" hint, so typos never reach code generation. Spellings from other languages are mapped in the hint: `i32` -> `Int32`, `u8` -> `UInt8`, `f64` -> `Float64`, `bool` -> `Bool`, `str` -> `String`, `Unit` -> `()`, `HashMap` -> `Map`.
- The checker is strict: there are no implicit casts or coercions between unrelated types.
- Generic type arity is validated for all known generic families, including built-ins such as `Option`, `Result`, `Async`, `Ref`, `RefMut`, and tuple wrappers.
- Function values use `Fn[...]` internally; surface syntax `Fn(A, B) -> R` is parsed and lowered into that shape.
- Borrow expressions synthesize wrapper types:
  - `&x` has type `Ref[T]`
  - `&mut x` has type `RefMut[T]`
- `dyn Trait` is supported for runtime-dispatch trait objects. See [Dyn Trait Objects](./dyn-trait-objects.md) for object-safety and runtime details.
- `async fn` calls produce `Async[T]`; only `await` can unwrap the `T`.
- Postfix `?` requires `Result[T, E]` and preserves `T` while checking `E` compatibility with the enclosing function return type.
- Type inference is local and deterministic. When inference cannot resolve a concrete type, the checker reports an error and uses unresolved internal marker `<?>` for continued analysis.
- `null` is forbidden both as a symbol and as a type fragment; use `Option[T]` for absence.
- Equality (`==`, `!=`) is defined for integers, floats, `Bool`, `Char`, and `String`. Comparing values of any other type (structs, enums, `Option`, `Vec`, tuples, ...) fails `aic check` with `E1291`. A generic function may compare values of a type parameter `T`; every call must then instantiate `T` with a type that supports equality, also through generic callers (`f[T]` calling `g[T]`). Compare fields individually or use `match` for other types.
- `Map[K, V]` and `Set[K]` store keys of type `String`, `Bytes`, `Int`, `UInt64`, or `Bool`, and `Map` values of the same types. Other concrete element types fail with `E1292`; a type parameter is checked where it becomes concrete.
- `Ref[T]` and `RefMut[T]` have the same runtime representation as `T`.
- Recursive types are not supported yet: a struct or enum that contains itself, directly or through other types (including `Option`, `Vec`, and other generic arguments), fails with `E1294`, and the message names the containment path.
- Generic functions are monomorphized. A generic function that calls itself with a growing type argument (polymorphic recursion, `f[T]` calling `f[Option[T]]`) fails with `E1293`. Mutual polymorphic recursion through several functions is caught in code generation (`E5037`).
- The entry function `main` takes no parameters and returns an integer type, `Bool`, or `()` (`async fn main` returns the same inside `Async`). Other signatures fail with `E1295`. Read command-line arguments with `std.env`.
- Integer and float literal suffixes (`65u8`, `1.5f32`) are kept on the literal itself, so they type the literal in every module of a program.
- Tuple types use `(T, U, ...)`; a single parenthesized type remains grouping.
- Extern C-ABI signatures currently accept only C-compatible scalar/value forms for parameters and returns (`Int`, `Int8`, `Int16`, `Int32`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `Bool`, `Float`, `Char`, and `()`, with no unresolved generics).
- Extern C-ABI parameters additionally accept `String` as the current target's `ptr-len-cap` value view (`{ i8*, i64, i64 }` in LLVM); keep native consumers aligned with that layout.
