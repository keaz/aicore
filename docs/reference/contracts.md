# Contracts Reference

See also: [Effects](./effects.md), [Statements](./statements.md), [Types](./types.md)

This page documents function/struct contracts as parsed, type-checked, statically verified, and lowered.

## Grammar

```ebnf
requires_clause   = "requires" expr ;
ensures_clause    = "ensures" expr ;
invariant_clause  = "invariant" expr ;

fn_decl           = ... requires_clause* ensures_clause* block ;
struct_decl       = "struct" ident ... "{" ... "}" invariant_clause? ;
```

## Semantics and Rules

- `requires` and `ensures` apply to non-extern functions.
- Struct `invariant` applies to struct declarations and is checked on construction after lowering.
- Type rules:
  - `requires` must have type `Bool`
  - `ensures` must have type `Bool`
  - `invariant` must have type `Bool`
- `ensures` expressions may reference synthetic `result` bound to the function return value type.
- Contracts run in pure mode; effectful calls in contract expressions are rejected.
- Static contract verifier (`verify_static`) classifies obligations into:
  - statically false: compile-time error
  - statically true: compile-time discharge note
  - unknown: residual runtime obligation note
- Runtime lowering (`lower_runtime_asserts`) performs:
  - `requires`: inserted as entry assertions
  - `ensures`: inserted at explicit returns and implicit tail exits
  - struct invariant checks: struct literal rewrites through synthesized helper functions
- Lowered runtime contract failures use deterministic panic message shape keyed by contract kind and owner.
- Extern declarations cannot carry contracts.

## Diagnostic mapping

- `E4001`: statically false `requires`
- `E4002`: statically false `ensures`
- `E4003`: residual runtime obligation (note)
- `E4004`: statically false struct invariant
- `E4005`: statically discharged obligation (note)
