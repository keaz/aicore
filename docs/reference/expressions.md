# Expressions Reference

See also: [Statements](./statements.md), [Pattern Matching](./pattern-matching.md), [Effects](./effects.md), [Memory](./memory.md)

This page covers expression grammar and typing/effect rules implemented by `src/parser.rs` and `src/typecheck.rs`.

## Grammar

```ebnf
expr           = or_expr ;
or_expr        = and_expr ("||" and_expr)* ;
and_expr       = bit_or_expr ("&&" bit_or_expr)* ;
bit_or_expr    = bit_xor_expr ("|" bit_xor_expr)* ;
bit_xor_expr   = bit_and_expr ("^" bit_and_expr)* ;
bit_and_expr   = eq_expr ("&" eq_expr)* ;
eq_expr        = cmp_expr (("==" | "!=") cmp_expr)* ;
cmp_expr       = shift_expr (("<" | "<=" | ">" | ">=") shift_expr)* ;
shift_expr     = add_expr (("<<" | ">>" | ">>>") add_expr)* ;
add_expr       = mul_expr (("+" | "-") mul_expr)* ;
mul_expr       = unary_expr (("*" | "/" | "%") unary_expr)* ;

unary_expr     = closure_expr
               | "&" "mut"? unary_expr
               | "await" unary_expr
               | "-" unary_expr
               | "!" unary_expr
               | "~" unary_expr
               | postfix_expr ;

closure_expr   = "|" closure_params? "|" "->" type block ;
closure_params = closure_param ("," closure_param)* ","? ;
closure_param  = ident (":" type)? ;

postfix_expr   = primary_expr (call_suffix | field_suffix | try_suffix)* ;
call_suffix    = "(" arg_list? ")" ;
arg_list       = call_arg ("," call_arg)* ","? ;
call_arg       = ident ":" expr | expr ;
field_suffix   = "." ident | "." int ;
try_suffix     = "?" ;

primary_expr   = literal
               | ident
               | struct_init
               | "(" expr ")"
               | "(" ")"
               | if_expr
               | while_expr
               | loop_expr
               | break_expr
               | continue_expr
               | match_expr
               | unsafe_block ;

if_expr        = "if" expr block "else" (block | if_expr) ;
while_expr     = "while" expr block ;
loop_expr      = "loop" block ;
break_expr     = "break" expr? ;
continue_expr  = "continue" ;
match_expr     = "match" expr "{" match_arm ("," match_arm)* ","? "}" ;
match_arm      = pattern ("if" expr)? "=>" expr ;
unsafe_block   = "unsafe" block ;

struct_init    = ident "{" struct_field ("," struct_field)* ","? "}" ;
struct_field   = ident ":" expr ;

literal        = int | float | string | "true" | "false" ;
int            = decimal_int | hex_int ;
hex_int        = "0x" hexdigit+ ;
```

## Semantics and Rules

- Binary operators are left-associative at each precedence tier.
- Arithmetic:
  - `+ - * /` require matching numeric operands (`Int` with `Int`, or `Float` with `Float`)
  - `%` requires `Int` operands
- Bitwise:
  - `& | ^ << >> >>>` require `Int` operands and produce `Int`
  - `>>` is arithmetic right-shift, `>>>` is logical right-shift
  - unary `~` requires `Int` and produces `Int`
- Comparison operators (`< <= > >=`) require matching numeric operands and return `Bool`.
- Equality operators (`== !=`) require compatible operand types and return `Bool`.
- Logical operators (`&& ||`) require `Bool` operands.
- `if` requires a `Bool` condition and both branches must resolve to compatible result types.
- `while` requires a `Bool` condition. Its expression type is unit unless broken with typed flow.
- `loop` and `break` are typed together: all breaks in one loop must agree on one break type.
- `continue` and `break` are valid only inside loop contexts.
- Function calls support:
  - direct function names
  - qualified module calls (`module.symbol(...)`)
  - first-class `Fn(...) -> ...` values
  - arguments may be positional (`f(1, 2)`) or named (`f(x: 1, y: 2)`)
  - when mixed, positional arguments must come first; otherwise `E1092` is reported
  - named arguments can be supplied in any order and are matched by parameter name
  - unknown named arguments report `E1213` and include nearest-name suggestions
- Method calls are parsed as postfix field access followed by `(...)` and resolve during type checking.
- Call resolution rejects ambiguous unqualified names when multiple modules export the same symbol.
- Calls to `unsafe fn` or `extern` declarations require an explicit unsafe boundary (`unsafe fn` context or `unsafe { ... }`).
- `await` is valid only inside `async fn` and requires `Async[T]`.
- Postfix `?` is valid only on `Result[T, E]` and requires enclosing return type compatibility `Result[_, E]`.
- Struct literals require all fields exactly once; missing, duplicate, and unknown fields are diagnostics.
- Field access requires a struct type and a declared field.
- Tuple values use the same parentheses form as grouping expressions: one element groups, two or more elements build a tuple.
- Tuple field access uses numeric indices (`.0`, `.1`, ...).
- Match guards must have type `Bool`.
- Effectful calls contribute to effect usage; in contract contexts, any effectful call is rejected.
