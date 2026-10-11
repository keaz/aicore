# Expressions Reference

See also: [Statements](./statements.md), [Pattern Matching](./pattern-matching.md), [Effects](./effects.md), [Memory](./memory.md)

This page covers expression grammar and typing/effect rules implemented by `compiler/aic/libs/parser/src/main.aic` and `compiler/aic/libs/typecheck/src/main.aic`.

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
               | return_expr
               | match_expr
               | unsafe_block ;

if_expr        = "if" expr block "else" (block | if_expr) ;
while_expr     = "while" expr block ;
loop_expr      = "loop" block ;
break_expr     = "break" expr? ;
continue_expr  = "continue" ;
return_expr    = "return" expr? ;
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
  - Integer division and remainder are defined on every platform and for every integer width:
    - `/` truncates toward zero; `%` has the sign of the dividend (`-7 / 2 == -3`, `-7 % 2 == -1`)
    - a zero divisor panics with `division_by_zero{op:div,type:<T>}: integer division by zero` (`op:rem` and `integer remainder by zero` for `%`)
    - for signed types, `MIN / -1` panics with `integer_overflow{op:div,type:<T>}: <T> division overflows (<MIN> / -1)`; `MIN % -1` is `0`
    - the checks are skipped when the divisor is a non-zero literal (the overflow check also needs a literal other than `-1`), or a parameter that the function's `requires` proves non-zero (`b != 0`, `b > 0`, `b >= 1`, `b < 0`, ...) and that the body never rebinds or assigns. `requires b > 0` also rules out `-1`; `requires b != 0` keeps the overflow check.
  - Float division follows IEEE 754 (division by zero yields an infinity or NaN, no panic).
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
- Diverging expressions: `return`, `break`, `continue`, `panic(...)`, a `loop` without `break`, and a block whose statements end with one of them never produce a value. Their type is the bottom type (written `!` in diagnostics), which is compatible with every type:
  - an `if` branch or `match` arm that diverges does not constrain the type of the other branches (`if x > 0 { x } else { return 0 }` has type `Int`)
  - `let v = match opt { Some(n) => n, None => return 0 };` binds `v: Int`
  - `if c { break v } else { continue }` is valid inside a loop
- `return expr` can appear anywhere an expression can, including match arms and block tails (`else { return -1 }`). It always exits the innermost enclosing function or closure and is checked against that return type.
- Inside a closure, `return` leaves the closure, and `break`/`continue` cannot target loops outside the closure (`E1275`/`E1276`).
- Closures capture the outer variables they use **by value, when the closure is created**. Later assignments to the outer variable are not seen by the closure (`let mut n = 1; let f = |d: Int| -> Int { n + d }; n = 5; f(0)` is `1`).
- A closure cannot assign to a captured variable or borrow it with `&mut` (`E1289`): the change would only reach the closure's copy. Pass the value as a parameter and assign the result instead: `let inc = |v: Int| -> Int { v + 1 }; n = inc(n);`. Variables the closure declares itself (parameters and its own `let mut` bindings) can be assigned normally. This applies equally to closures stored in struct fields, returned from functions, and nested inside other closures.
- `return` is not allowed in contract expressions or constant initializers (`E1279`).
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
