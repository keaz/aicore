# Syntax Reference (Frozen MVP)

This file is the frozen grammar contract for the current parser implementation.
If parser behavior changes, this file must be updated in the same change.

Version: `mvp-grammar-v7`

## Lexical tokens

- `ident`: `[A-Za-z_][A-Za-z0-9_]*`
- `int`: decimal integer literal (`0`, `1`, `42`, ...)
- `float`: decimal/scientific literal (`3.14`, `0.5`, `1e10`, `2.5e-3`)
- `string`: double-quoted UTF-8 string with escape support
- `template_string`: prefixed string literal (`f"..."` or `$"..."`) with `{expr}` interpolation; use `{{` / `}}` (or `\{` / `\}`) for literal braces
- `char`: single-quoted Unicode scalar literal with escape support (examples: `'a'`, `'😀'`; escapes like backslash-n and unicode codepoint escapes)
- `bool`: `true | false`
- punctuation: `(` `)` `{` `}` `[` `]` `,` `;` `:` `.` `=>` `->`
- operators: `+ - * / % == != < <= > >= && || ! ? & = |`

## Top-level grammar

```ebnf
program        = module_decl? import_decl* item* EOF ;
module_decl    = "module" path ";" ;
import_decl    = "import" path ";" ;
path           = ident ("." ident)* ;

item           = item_visibility? (fn_decl | unsafe_fn_decl | extern_fn_decl | intrinsic_fn_decl | struct_decl | enum_decl | trait_decl | impl_decl) ;
item_visibility = "pub" | "priv" | "pub" "(" "crate" ")" ;

fn_decl           = async_prefix? "fn" ident generics? "(" params? ")" "->" type effects? contracts? block ;
unsafe_fn_decl    = "unsafe" "fn" ident generics? "(" params? ")" "->" type effects? contracts? block ;
extern_fn_decl    = "extern" string "fn" ident "(" params? ")" "->" type ";" ;
intrinsic_fn_decl = "intrinsic" "fn" ident "(" params? ")" "->" type effects? ";" ;
async_prefix      = "async" ;
effects           = "effects" "{" effect_list? "}" ;
effect_list       = ident ("," ident)* ","? ;
contracts         = (requires_clause | ensures_clause)* ;
requires_clause   = "requires" expr ;
ensures_clause    = "ensures" expr ;

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
field          = field_visibility? ident ":" type ("=" expr)? ;
field_visibility = item_visibility ;
```

## Type grammar

```ebnf
type           = unit_type | named_type | hole_type ;
unit_type      = "(" ")" ;
named_type     = type_name type_args? ;
type_name      = ident ("::" ident)* ;
type_args      = "[" type ("," type)* ","? "]" ;
hole_type      = "_" ;
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
               | template_string
               | char
               | bool
               | unit_lit
               | ident
               | struct_init
               | grouped_expr
               | if_expr
               | match_expr ;

grouped_expr   = "(" expr ")" ;
unit_lit       = "(" ")" ;
template_string = ("f" | "$") "\"" template_item* "\"" ;
template_item   = template_interp | template_escaped_brace | string_char ;
template_interp = "{" expr "}" ;
template_escaped_brace = "{{" | "}}" | "\\{" | "\\}" ;

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


Template literals:
- Supported prefixes are `f"..."` and `$"..."`.
- Interpolation segments use `{expr}` and are lowered to `aic_string_format_intrinsic(template, args)`.
- Literal braces can be written as `{{` and `}}` (or escaped as `\{` and `\}`).
- Interpolated values must type-check as `String`; use explicit conversion helpers like `int_to_string(...)` when needed.

Visibility and access control:
- Top-level `fn`, `struct`, `enum`, `trait`, and `impl` items default to private visibility.
- Visibility modifiers are `pub`, `pub(crate)`, and `priv`.
- Struct fields default to private; mark fields `pub` when cross-module reads/writes are part of the API.
- User-authored direct calls to runtime intrinsic symbols (`aic_*`) are rejected during type checking.
- `intrinsic fn` declarations are signature-only and must end with `;`.
- Intrinsic declarations may include `effects { ... }`, but `requires`/`ensures`, generics, and function bodies are rejected (`E1093`).
- Canonical IR/JSON encodes intrinsic runtime metadata with `is_intrinsic` and `intrinsic_abi` fields.
Struct default values:
- Struct declarations may define defaults with `field: Type = expr`.
- Struct literals may omit fields that have defaults.
- `TypeName::default()` is synthesized when all fields declare defaults.
- Default expressions are compile-time evaluable (literals, const references, and const arithmetic).

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
