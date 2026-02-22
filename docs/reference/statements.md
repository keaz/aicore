# Statements Reference

See also: [Expressions](./expressions.md), [Memory](./memory.md), [Contracts](./contracts.md)

This page documents statement grammar and block-level typing rules.

## Grammar

```ebnf
block          = "{" stmt* tail_expr? "}" ;
tail_expr      = expr ;

stmt           = let_stmt | assign_stmt | return_stmt | expr_stmt | assert_stmt ;
let_stmt       = "let" "mut"? ident (":" type)? "=" expr ";" ;
assign_stmt    = ident "=" expr ";" ;
return_stmt    = "return" expr? ";" ;
expr_stmt      = expr ";" ;
assert_stmt    = "assert" "(" expr ")" ";" ;   (* IR/runtime form; not user surface syntax *)
```

## Semantics and Rules

- Blocks are expression-producing:
  - if a tail expression exists, the block type is the tail expression type
  - otherwise the block type is `()`
- `let` bindings:
  - are immutable by default
  - require `mut` for later reassignment
  - may include explicit type annotation; otherwise type is inferred from initializer
- Let inference failures that remain unresolved are rejected and require explicit annotations.
- Assignment (`name = expr;`):
  - target must already exist in scope
  - target must be mutable
  - assigned value type must be compatible with the target binding type
- Return statements are checked against the enclosing function return type.
- Expression statements evaluate side effects and discard values.
- Runtime assertions are represented as `Stmt::Assert` in IR after contract lowering and must type-check as `Bool`.
- Missing statement semicolons are parser errors with deterministic autofix hints for `let`, assignment, and `return`.
- Scope is lexical; nested blocks create nested binding scopes.
