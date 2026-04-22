# Pattern Matching Reference

See also: [Expressions](./expressions.md), [Types](./types.md), [Generics](./generics.md)

This page describes `match` patterns and coverage rules implemented in `compiler/aic/libs/parser/src/main.aic` and `compiler/aic/libs/typecheck/src/main.aic`.

## Grammar

```ebnf
match_expr      = "match" expr "{" match_arm ("," match_arm)* ","? "}" ;
match_arm       = pattern guard_clause? "=>" expr ;
guard_clause    = "if" expr ;

pattern         = or_pattern ;
or_pattern      = pattern_atom ("|" pattern_atom)* ;
pattern_atom    = "_"
               | ident
               | int
               | "true"
               | "false"
               | "(" ")"
               | tuple_pattern
               | variant_pattern ;

tuple_pattern   = "(" pattern ("," pattern)+ ","? ")" ;
variant_pattern = ident "(" pattern ("," pattern)* ","? ")"
               | ident ;
```

## Semantics and Rules

- Scrutinee expression is typed first; each arm pattern must be compatible with that scrutinee type.
- Supported coverage-proven domains:
  - `Bool` (`true`, `false`)
  - `Option[T]` (`None`, `Some(...)`)
  - `Result[T, E]` (`Ok(...)`, `Err(...)`)
  - declared enums and their variants
- Binding rules:
  - each variable name may be bound only once per pattern tree
  - bound variable types are derived from the scrutinee and variant payload structure
- Or-pattern rules (`p1 | p2 | ...`):
  - each alternative must bind the same set of variable names
  - each shared binding name must resolve to compatible types across alternatives
- Guard rules:
  - guard expression must type-check as `Bool`
  - guarded arms are typed, but they do not count toward exhaustiveness coverage
- Exhaustiveness diagnostics are emitted when unguarded coverage is incomplete for supported domains.
- Redundant arm detection marks unreachable arms when earlier coverage already subsumes them.
- Tuple patterns destructure by position and use the same comma-based shape as tuple literals.
- Variant disambiguation at parse time:
  - identifier with payload syntax `Name(...)` is variant pattern
  - bare uppercase identifier is treated as zero-arg variant pattern
  - bare lowercase identifier is treated as variable-binding pattern
- Backend note: guarded match arms lower for `Bool`, tuple, and enum-like ADT matches in the current LLVM backend coverage.
