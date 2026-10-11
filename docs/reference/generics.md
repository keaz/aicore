# Generics Reference

See also: [Types](./types.md), [Expressions](./expressions.md), [Pattern Matching](./pattern-matching.md)

This page documents generic syntax and inference/coherence behavior from resolver and type checker passes.

## Grammar

```ebnf
generics         = "[" generic_param ("," generic_param)* ","? "]" ;
generic_param    = ident (":" trait_bounds)? ;
trait_bounds     = ident ("+" ident)* ;
where_clause     = "where" where_item ("," where_item)* ;
where_item       = ident ":" trait_bounds ;

type_args        = "[" type ("," type)* ","? "]" ;
call_type_args   = "::" type_args ;   (* before a call's arguments: f::[Int](x) *)

fn_decl          = "fn" ident generics? "(" ... ")" "->" type where_clause? ... ;
struct_decl      = "struct" ident generics? "{" ... "}" ... ;
enum_decl        = "enum" ident generics? "{" ... "}" ;
trait_decl       = "trait" ident generics? (";" | "{" trait_method_sig* "}") ;
trait_method_sig = ("async" | "unsafe")* "fn" ident generics? "(" ... ")" "->" type where_clause? ... ";" ;
impl_decl        = "impl" type (";" | "{" impl_method* "}") ;
```

## Semantics and Rules

- Generic parameters are supported on functions, structs, enums, and traits.
- Type arguments use square-bracket syntax (`Name[T]`).
- Generic method declarations are supported inside both inherent impl blocks and trait impl blocks.
- Function and trait-method declarations support `where` clauses, and inline bounds are equivalent to the same constraints written in `where`.
- Generic arity is checked for all known generic families.
- Function generic inference is constraint-based and deterministic:
  - from argument types
  - from expected return context when available
- Generic arguments flow into nested expressions:
  - an argument is checked against its parameter type with the callee's generic parameters replaced by what is already known, never by the callee's type-variable names
  - a generic struct literal or generic call used directly as an argument is inferred from its own fields or arguments (`unbox(Box { value: 41 })`)
  - an argument whose type is still incomplete after all arguments were checked is checked again once the sibling arguments have fixed the bindings, so generic constructors need no annotation: `vec.push(vec.new_vec(), 5)` is `Vec[Int]`
- If generic inference is incomplete, checker reports inference failure and requests stronger type context.
- Explicit type arguments: `f::[Int](x)`, `pair::[Int, Bool](1, true)`, `vec.new_vec::[Int]()`, `x.map_or::[Int, Int](0, f)`, `Some::[Int](5)`, `Ok::[Int, String](1)`, `Filled::[Int](5)`. They give the callee's generic parameters in declaration order (for a method, all of the method's generic parameters, including the ones its receiver fixes) and take precedence over inference, so arguments are checked against them (`E1214` on a mismatch). A wrong count, type arguments on a non-generic callee, on trait-bound or built-in methods, an empty list, or `::[..]` without call arguments is `E1312`. `::[` is used instead of `f[Int](x)` because `f[Int]` already means indexing.
- Trait bounds on generic params are validated:
  - bound traits must exist
  - bound arity must match current bound syntax model
  - concrete substitutions must have matching `impl Trait[ConcreteType];`
- Resolver enforces impl coherence for identical `(trait, type-argument tuple)` combinations.
- Inherent impl targets are named types (`struct` or `enum`) and can expose generic behavior through method-level generics.
- Trait methods participate in the same static resolution path as trait-bounded generic calls.
- Traits do not support associated types (`E1084`), associated constants (`E1085`) or default method bodies (`E1098`) yet. Model an associated type as an extra trait generic parameter (`trait Shape[T, Output]`) and implement every method body in the `impl` block.
- Built-in constructor behavior participates in generic inference:
  - `Some(v)` => `Option[T]`
  - `None` => `Option[<?>]` until constrained
  - `Ok(v)` / `Err(e)` => `Result[T, E]` forms with partial inference until constrained
- Generic function values cannot be used as first-class values without specialization.
- Concrete generic instantiations are recorded in IR metadata (`generic_instantiations`) with deterministic mangled keys.
- Generic functions that share a bare name across modules (a user `first[T]` and std `first[T]`) are recorded as separate instantiations; their mangled keys include the function symbol (`fn_first__s<symbol>_Int`).
- Mangling collisions for distinct instantiations are rejected.
