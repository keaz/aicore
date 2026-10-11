# Statements Reference

See also: [Expressions](./expressions.md), [Memory](./memory.md), [Contracts](./contracts.md)

This page documents statement grammar and block-level typing rules.

## Grammar

```ebnf
block          = "{" stmt* tail_expr? "}" ;
tail_expr      = expr ;

stmt           = let_stmt | assign_stmt | return_stmt | expr_stmt | assert_stmt ;
let_stmt       = "let" "mut"? ident (":" type)? "=" expr ";" ;
assign_stmt    = place assign_op expr ";" ;
place          = ident place_step* ;
place_step     = "." (ident | int_lit) | "[" expr "]" ;
assign_op      = "=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | ">>>=" ;
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
- Place assignment updates part of a value: a struct field (`p.x = 1`), a nested field (`p.a.b = 1`), a tuple element (`t.0 = x`, `t.1.0 = x`), a `Vec` element (`v[i] = x`), or any mix (`items[i].qty = 4`):
  - the root binding must be `let mut`, or `self` in a `&mut self` method (`E1266` otherwise)
  - each step must exist on the value's type: fields on structs, numeric elements on tuples, `Int` indexes on `Vec` (`E1228`, `E1297`)
  - the assigned value must match the place's type (`E1269`)
  - the root must not be borrowed (`E1265`) or moved (`E1277`); a closure cannot assign through a captured variable (`E1289`)
  - writes through `Ref`/`RefMut` values are rejected (`E1298`)
  - order: index expressions left to right, then the right-hand side, then the store
  - a `Vec` element write copies the buffer first, so other values that share it are unchanged (value semantics); an index outside `0..len` panics with `index_out_of_bounds{op:assign,type:Vec}`
  - after the store, every struct on the path that declares an `invariant` is checked again; a violation panics with `contract_violation{kind:invariant,owner:<Struct>}`
  - compound operators work on field and tuple places (`p.flags |= 4;`); on a place with an index they are rejected (`E1110`)
- Methods with a `&mut self` receiver update the caller's value:
  - `value.method(...)` and `Type::method(value, ...)` pass the receiver's storage, so assignments to `self` or its fields inside the method are visible to the caller
  - the receiver must be a `let mut` binding (or a field of one, or `self` in another `&mut self` method) that is not borrowed (`E1266`, `E1263`); a temporary receiver (`make().bump()`) is allowed and its update is discarded
  - a closure cannot call a `&mut self` method on a captured variable (`E1289`)
  - `&mut self` is not allowed on `async` methods (`E1299`) or on trait methods used through `dyn Trait` (`E1214`), and an impl must use the same receiver form as its trait (`E1108`)
  - `self` and `&self` receivers take the receiver by value; assigning to `self` there is rejected (`E1266`)
- Return statements are checked against the return type of the enclosing function or closure, at any nesting depth: inside `if`, `while`, `loop`, `for`, and `match` blocks. A nested block's expected value type never changes what `return` must produce.
- `return expr` directly before a block's closing `}` may omit the `;`; it is then the block's tail expression (see `return_expr` in [Expressions](./expressions.md)).
- Statements after a diverging statement (`return ...;`, `panic(...);`, `break;`, `continue;`) are unreachable, and a block that ends with one has the bottom type, so it satisfies any expected type.
- Expression statements evaluate side effects and discard values.
- Runtime assertions are represented as `Stmt::Assert` in IR after contract lowering and must type-check as `Bool`.
- Missing statement semicolons are parser errors with deterministic autofix hints for `let`, assignment, and `return`.
- Scope is lexical; nested blocks create nested binding scopes.
