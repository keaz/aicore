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
- If generic inference is incomplete, checker reports inference failure and requests stronger type context.
- Trait bounds on generic params are validated:
  - bound traits must exist
  - bound arity must match current bound syntax model
  - concrete substitutions must have matching `impl Trait[ConcreteType];`
- Resolver enforces impl coherence for identical `(trait, type-argument tuple)` combinations.
- Inherent impl targets are named types (`struct` or `enum`) and can expose generic behavior through method-level generics.
- Trait methods participate in the same static resolution path as trait-bounded generic calls.
- Built-in constructor behavior participates in generic inference:
  - `Some(v)` => `Option[T]`
  - `None` => `Option[<?>]` until constrained
  - `Ok(v)` / `Err(e)` => `Result[T, E]` forms with partial inference until constrained
- Generic function values cannot be used as first-class values without specialization.
- Concrete generic instantiations are recorded in IR metadata (`generic_instantiations`) with deterministic mangled keys.
- Mangling collisions for distinct instantiations are rejected.
