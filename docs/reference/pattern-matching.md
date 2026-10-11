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
- Exhaustiveness and reachability use the usefulness algorithm of Maranget ("Warnings for pattern matching") over the whole pattern, not only the top-level constructor. Nested payloads, tuples, struct fields, literals, wildcards, bindings, and or-patterns are all taken into account:
  - `Some(Red)` and `Some(Green)` are different arms, and a missing `Some(Blue)` is reported
  - `(Red, true)`, `(Green, _)`, `(Blue, _)` is missing `(Red, false)`
  - `Some(1)` followed by `Some(_)` is reachable
- Types with finitely many values: `Bool`, `()`, `Option[T]`, `Result[T, E]`, declared enums, tuples and structs of such types. Types with infinitely many values (`Int` and the other integer types, `String`, `Char`, floats, generic parameters) are covered only by a `_` or binding arm.
- Binding rules:
  - each variable name may be bound only once per pattern tree
  - bound variable types are derived from the scrutinee and variant payload structure
- Or-pattern rules (`p1 | p2 | ...`):
  - each alternative must bind the same set of variable names
  - each shared binding name must resolve to compatible types across alternatives
- Guard rules:
  - guard expression must type-check as `Bool`
  - guarded arms are typed, but they do not count toward exhaustiveness coverage
- A non-exhaustive match is an error that lists up to 8 witness patterns of values no unguarded arm matches, and the suggested fix inserts one arm per witness:
  - `E1246` for `Bool`, `E1247` for `Option`, `E1248` for `Result`, `E1249` for declared enums
  - `E1290` for every other type (tuples, structs, integers, strings, chars)
  - when every missing value is a whole variant (`Blue`, `Some(_)`), the message names the variants; otherwise it shows the witness patterns (`Some(Blue)`, `(Red, false)`, `_`)
- An arm is unreachable (`E1251`) when every value it matches is already matched by an earlier unguarded arm.
- Tuple patterns destructure by position and use the same comma-based shape as tuple literals.
- Variant disambiguation at parse time:
  - identifier with payload syntax `Name(...)` is variant pattern
  - bare uppercase identifier is treated as zero-arg variant pattern
  - bare lowercase identifier is treated as variable-binding pattern
- Code generation lowers every pattern the checker accepts, so `aic check` and `aic build` agree on matches:
  - integer literals of every width and char literals compare with `icmp`; strings compare by content
  - enum variants test the tag; variant payloads, tuple elements, and struct fields are matched recursively to any depth (`Some(Some(Some(7)))`, `Rect((w, 0))`, `Pixel { lit: true, color: Red, level }`)
  - or-patterns, bindings at any depth, and guards are supported everywhere; a guard that fails falls through to the next arm
  - arms are tried in order; matches whose arms are all flat (bindings, wildcards, `true`/`false`, variants with a binding or wildcard payload) use a single tag switch
  - the checker proves exhaustiveness, so no arm matching is unreachable; if it ever happened at run time the program panics with `match_failure`
