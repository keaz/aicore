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
mul_expr       = cast_expr (("*" | "/" | "%") cast_expr)* ;
cast_expr      = unary_expr ("as" type)* ;

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

postfix_expr   = primary_expr (call_suffix | field_suffix | index_suffix | try_suffix)* ;
index_suffix   = "[" expr "]" ;
list_literal   = "[" (expr ("," expr)* ","?)? "]" ;
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
               | labeled_loop
               | while_expr
               | loop_expr
               | break_expr
               | continue_expr
               | return_expr
               | match_expr
               | unsafe_block
               | block_expr ;

if_expr        = "if" expr block ("else" (block | if_expr))? ;
block_expr     = block ;
labeled_loop   = label ":" (while_expr | loop_expr | for_expr) ;
label          = "'" ident ;
while_expr     = "while" expr block ;
loop_expr      = "loop" block ;
break_expr     = "break" label? expr? ;
continue_expr  = "continue" label? ;
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
- Loop labels: `'name: while ...`, `'name: loop ...` and `'name: for ...` name a loop. `break 'name` leaves that loop and `continue 'name` starts its next iteration (the next element for `for`, the condition for `while`), from any depth of nested loops. `break 'name value` gives a labeled `loop` its value, and it is typed together with the loop's other breaks (`E1274` on a mismatch). `while` and `for` loops have type `()`, so a break with a value cannot target them (`E1274`). A label that no enclosing loop has is `E1307`; a nested loop may not reuse an enclosing loop's label (`E1308`); a label not followed by `while`, `loop` or `for`, or without its `:`, is `E1306`. Labels do not reach into closures (`E1275`/`E1276`). Without a label, `break` and `continue` apply to the innermost loop. `'a'` is still a character literal: a quote followed by a name and no closing quote is a label.
- `if let PATTERN = expr { ... } else { ... }` runs the first block with the pattern's bindings when it matches, otherwise the `else` block (optional, as for `if`; `else if` and `else if let` chain). `while let PATTERN = expr { ... }` repeats while the pattern matches; `break` and `continue` work as in `loop`. A pattern that always matches is reported (`E1251`). Both are written back unchanged by `aic fmt`.
- Indexing `base[index]` reads one element and panics with `index_out_of_bounds{op:index}` when it does not exist. It works on `Vec[T]` (`Int` index, element `T`), `Map[K, V]` (key `K`, value `V`; a missing key panics), `String` (`Int` character index, one-character `String`) and `Bytes` (`Int` byte index, `UInt8`). The operation comes from the type's std module, which must be imported (`E1304`). Use `vec.get`, `map.get`, `string.char_at` or `bytes.byte_at` for an `Option`/`Result` instead of a panic. Assignment `v[i] = x` is described in [Statements](./statements.md). There is no `Slice[T]` type (it is reported as unknown, `E1228`); use `Vec[T]`.
- Numeric casts `x as T` convert between numeric types only when every value of the source type is kept exactly: integer widening that keeps the sign (`Int8` to `Int16`, `UInt8` to `Int16`, `UInt32` to `UInt64`), integers of 16 bits or fewer to `Float32`, integers of 32 bits or fewer to `Float64`, and `Float32` to `Float64`. Every other pair, and any non-numeric type, is `E1305`, whose help names the method to use. `as` binds tighter than the binary operators and looser than the unary ones (`-x as Int64` is `(-x) as Int64`; `a * b as Int` is `a * (b as Int)`). `as` is a contextual keyword, so it is still usable as a name.
- Numeric conversion methods state what happens to values that do not fit. They exist on every integer and float type, with a target suffix `int`, `isize`, `usize`, `i8`..`i128`, `u8`..`u128`, `f32` or `f64`:
  - `x.checked_to_<int>()` returns `Option[T]`: `None` when the value does not fit, and for NaN and infinities.
  - `x.saturating_to_<int>()` clamps to the target range; NaN becomes `0`.
  - `x.wrapping_to_<int>()` (integers only) keeps the low bits in two's complement (`300.wrapping_to_u8()` is `44`, `(-1).wrapping_to_u8()` is `255`).
  - `x.to_f32()` / `x.to_f64()` round to the nearest float; `Float64` values beyond the `Float32` range become infinities, and NaN stays NaN.
  - A float converted to an integer truncates toward zero (`3.9.saturating_to_i8()` is `3`, `(-0.5).checked_to_u8()` is `Some(0)`).
  - Methods that do not exist for a target (`to_int`, `checked_to_f64`, `wrapping_to_` from or to a float) are `E1305`; an unknown suffix is an unknown method (`E1228`). `aic fmt` writes casts and conversion methods back unchanged.
- List literals `[a, b, c]` build a `Vec` (needs `import std.vec`); `[]` needs a known element type (`let empty: Vec[Int] = [];`).
- Method syntax on std collections: when a `Vec`, `Map`, `Set`, `String`, `Bytes` or `Deque` value has no inherent method of that name, `value.f(args)` calls the std module function `f(value, args)`, or `<module>_f` (`v.len()` is `vec.vec_len(v)`, `v.push(x)` is `vec.push(v, x)`). The module must be imported.
- Enum variants as values: a unit variant is written bare (`Red`, `None`) or qualified (`Color::Red`, `Option::None`); a payload variant is called (`Color::Blue(3)`, `Option::Some(x)`, `Result::Ok(v)`). A bare name that more than one visible enum declares needs a typed context or a qualifier (`E2104`). Local bindings shadow variant names. The legacy `None()` spelling is accepted, and `aic fmt` prints it as `None`.
- Block expressions (`{ stmts; tail }`) open a new scope; bindings declared inside end with the block. The block's value is its tail expression, or `()` without one: `let x = { let a = 1; a + 1 };`.
- `if cond { ... }` without `else` has type `()`: the branch's value must be `()` (end it with `;` to discard a value). Otherwise `E1220` reports "`if` without `else` has type '()'".
- Expressions that end in a block (`if`, `match`, `while`, `loop`, `for`, `{ ... }`, `unsafe { ... }`) can be statements without a trailing `;`; `aic fmt` keeps the `;` form.
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
  - a non-generic, pure function or impl method named as a value (`apply(double, 2)`, `apply(C::get, c)`) becomes a `Fn(...) -> ...` value; a method's `self` receiver is its first parameter, so `C::get` with `fn get(self) -> Int` has type `Fn(C) -> Int`. Generic functions (`E1282`), effectful functions (`E1283`) and `&mut self` methods (`E1303`: the receiver is passed by pointer, and an `Fn(C) -> R` value receives `C` by value) cannot be used as values
  - arguments may be positional (`f(1, 2)`) or named (`f(x: 1, y: 2)`)
  - when mixed, positional arguments must come first; otherwise `E1092` is reported
  - named arguments can be supplied in any order and are matched by parameter name
  - unknown named arguments report `E1213` and include nearest-name suggestions
- Method calls are parsed as postfix field access followed by `(...)` and resolve during type checking.
- Call resolution rejects ambiguous unqualified names when multiple modules export the same symbol.
- Calls to `unsafe fn` or `extern` declarations require an explicit unsafe boundary (`unsafe fn` context or `unsafe { ... }`).
- `await` is valid only inside `async fn` and requires `Async[T]`.
- Postfix `?` works on `Result[T, E]` (the enclosing function or closure returns `Result[_, E]`) and on `Option[T]` (it returns `Option[_]`); on `Err`/`None` it returns that value early. See [Option and Result](../std-api/option-result.md).
- Struct literals require all fields exactly once; missing, duplicate, and unknown fields are diagnostics.
- Field access requires a struct type and a declared field.
- Tuple values use the same parentheses form as grouping expressions: one element groups, two or more elements build a tuple.
- Tuple field access uses numeric indices (`.0`, `.1`, ...).
- Match guards must have type `Bool`.
- Effectful calls contribute to effect usage; in contract contexts, any effectful call is rejected.
