# Types Reference

See also: [Syntax](./syntax.md), [Generics](./generics.md), [Memory](./memory.md), [Expressions](./expressions.md)

This page describes the surface type grammar and the type relations enforced in `src/typecheck.rs`.

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
- Tuple types use `(T, U, ...)`; a single parenthesized type remains grouping.
- Extern C-ABI signatures currently accept only C-compatible scalar/value forms for parameters and returns (`Int`, `Int8`, `Int16`, `Int32`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `Bool`, `Float`, `Char`, and `()`, with no unresolved generics).
- Extern C-ABI parameters additionally accept `String` as the current target's `ptr-len-cap` value view (`{ i8*, i64, i64 }` in LLVM); keep native consumers aligned with that layout.
