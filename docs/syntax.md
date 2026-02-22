# Syntax Reference (Frozen MVP)

This file is the frozen grammar contract for the current parser implementation.
If parser behavior changes, this file must be updated in the same change.

Version: `mvp-grammar-v6`

## Lexical tokens

- `ident`: `[A-Za-z_][A-Za-z0-9_]*`
- `int`: decimal integer literal (`0`, `1`, `42`, ...)
- `float`: decimal/scientific literal (`3.14`, `0.5`, `1e10`, `2.5e-3`)
- `string`: double-quoted UTF-8 string with escape support
- `bool`: `true | false`
- punctuation: `(` `)` `{` `}` `[` `]` `,` `;` `:` `.` `=>` `->`
- operators: `+ - * / % == != < <= > >= && || ! ? & = |`

## Top-level grammar

```ebnf
program        = module_decl? import_decl* item* EOF ;
module_decl    = "module" path ";" ;
import_decl    = "import" path ";" ;
path           = ident ("." ident)* ;

item           = fn_decl | struct_decl | enum_decl | trait_decl | impl_decl ;

fn_decl        = async_prefix? "fn" ident generics? "(" params? ")" "->" type
                 effects? contracts? block ;
async_prefix   = "async" ;
effects        = "effects" "{" effect_list? "}" ;
effect_list    = ident ("," ident)* ","? ;
contracts      = (requires_clause | ensures_clause)* ;
requires_clause = "requires" expr ;
ensures_clause  = "ensures" expr ;

struct_decl    = "struct" ident generics? "{" fields? "}" invariant_clause? ;
invariant_clause = "invariant" expr ;

enum_decl      = "enum" ident generics? "{" variants? "}" ;
variants       = variant ("," variant)* ","? ;
variant        = ident ("(" type ")")? ;

trait_decl     = "trait" ident generics? ";" ;
impl_decl      = "impl" ident "[" type ("," type)* ","? "]" ";" ;

generics       = "[" generic_param ("," generic_param)* ","? "]" ;
generic_param  = ident trait_bounds? ;
trait_bounds   = ":" ident ("+" ident)* ;
params         = param ("," param)* ","? ;
param          = ident ":" type ;
fields         = field ("," field)* ","? ;
field          = ident ":" type ;
```

## Type grammar

```ebnf
type           = unit_type | named_type ;
unit_type      = "(" ")" ;
named_type     = type_name type_args? ;
type_name      = ident ("::" ident)* ;
type_args      = "[" type ("," type)* ","? "]" ;
```

## Statement grammar

```ebnf
block          = "{" stmt* tail_expr? "}" ;
tail_expr      = expr ;

stmt           = let_stmt | assign_stmt | return_stmt | expr_stmt ;
let_stmt       = "let" "mut"? ident (":" type)? "=" expr ";" ;
assign_stmt    = ident "=" expr ";" ;
return_stmt    = "return" expr? ";" ;
expr_stmt      = expr ";" ;
```

## Expression grammar and precedence

Highest precedence appears lowest in the tree below.

```ebnf
expr           = or_expr ;
or_expr        = and_expr ("||" and_expr)* ;
and_expr       = equality_expr ("&&" equality_expr)* ;
equality_expr  = compare_expr (("==" | "!=") compare_expr)* ;
compare_expr   = term_expr (("<" | "<=" | ">" | ">=") term_expr)* ;
term_expr      = factor_expr (("+" | "-") factor_expr)* ;
factor_expr    = unary_expr (("*" | "/" | "%") unary_expr)* ;
unary_expr     = ("await" | "-" | "!" | borrow_prefix) unary_expr | postfix_expr ;
borrow_prefix  = "&" "mut"? ;

postfix_expr   = primary_expr (call_suffix | field_suffix | try_suffix)* ;
call_suffix    = "(" arg_list? ")" ;
arg_list       = expr ("," expr)* ","? ;
field_suffix   = "." ident ;
try_suffix     = "?" ;

primary_expr   = int
               | float
               | string
               | bool
               | unit_lit
               | ident
               | struct_init
               | grouped_expr
               | if_expr
               | match_expr ;

grouped_expr   = "(" expr ")" ;
unit_lit       = "(" ")" ;

if_expr        = "if" expr block "else" (block | if_expr) ;
match_expr     = "match" expr "{" match_arms? "}" ;
match_arms     = match_arm ("," match_arm)* ","? ;
match_arm      = pattern guard_clause? "=>" expr ;
guard_clause   = "if" expr ;

struct_init    = ident "{" struct_init_fields? "}" ;
struct_init_fields = struct_init_field ("," struct_init_field)* ","? ;
struct_init_field = ident ":" expr ;
```

## Pattern grammar

```ebnf
pattern        = or_pattern ;
or_pattern     = pattern_atom ("|" pattern_atom)* ;
pattern_atom   = "_"
               | ident_pattern
               | int
               | bool
               | unit_pattern
               | variant_pattern ;

unit_pattern   = "(" ")" ;
ident_pattern  = ident ;
variant_pattern = ident ("(" pattern ("," pattern)* ","? ")")? ;
```

Pattern disambiguation:
- bare uppercase identifier is treated as a zero-arg variant pattern
- bare lowercase identifier is treated as a variable binding pattern
- `|` inside patterns is pattern-or; logical-or in expressions remains `||`
- match guards (`if <expr>`) are checked as `Bool` expressions

Result propagation:
- `expr?` is a postfix propagation operator.
- `expr?` requires `expr: Result[T, E]` and an enclosing function return type `Result[U, E]`.

Mutability and references:
- Bindings are immutable by default; use `let mut name = ...;` for reassignment.
- Assignment is a statement (`name = expr;`), never an expression.
- Borrow expressions are `&name` and `&mut name`.

## Canonical formatting contract

- Formatting is IR-driven (not token-preserving).
- `aic fmt` output is deterministic for equivalent IR.
- Formatting is idempotent: applying formatter to formatter output must produce byte-identical text.
