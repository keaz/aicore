# Syntax Reference

See also: [Types](./types.md), [Expressions](./expressions.md), [Statements](./statements.md), [Modules](./modules.md), [Frozen EBNF Artifact](../grammar.ebnf)

This page documents the concrete grammar accepted by `src/lexer.rs` and `src/parser.rs`.
The canonical machine-readable artifact is [`docs/grammar.ebnf`](../grammar.ebnf), sourced from the frozen contract in `docs/syntax.md`.

## Grammar

### Lexical tokens

```ebnf
ident      = ("A".."Z" | "a".."z" | "_") ("A".."Z" | "a".."z" | "0".."9" | "_")* ;
int        = decimal_digits ;
float      = decimal_digits "." decimal_digits (exponent)?
           | decimal_digits exponent ;
string     = '"' { char | escape } '"' ;

keyword    = "module" | "import" | "async" | "extern" | "unsafe" | "fn" |
             "struct" | "enum" | "trait" | "impl" |
             "let" | "mut" | "return" |
             "if" | "else" | "match" | "while" | "loop" | "break" | "continue" |
             "true" | "false" | "await" |
             "requires" | "ensures" | "invariant" | "effects" | "capabilities" | "null" ;
```

### Top-level grammar

```ebnf
program        = module_decl? import_decl* item* EOF ;
module_decl    = "module" path ";" ;
import_decl    = "import" path ";" ;
path           = ident ("." ident)* ;

item           = fn_decl | extern_fn_decl | intrinsic_fn_decl | unsafe_fn_decl | struct_decl | enum_decl | trait_decl | impl_decl ;
fn_decl        = "async"? "fn" ident generics? "(" params? ")" "->" type where_clause? effects_clause? capabilities_clause? contract_clause* block ;
unsafe_fn_decl = "unsafe" "fn" ident generics? "(" params? ")" "->" type where_clause? effects_clause? capabilities_clause? contract_clause* block ;
extern_fn_decl = "extern" string "fn" ident generics? "(" params? ")" "->" type ";" ;
intrinsic_fn_decl = "intrinsic" "fn" ident "(" params? ")" "->" type effects_clause? ";" ;

generics       = "[" generic_param ("," generic_param)* ","? "]" ;
generic_param  = ident (":" ident ("+" ident)*)? ;
params         = param ("," param)* ","? ;
param          = ident ":" type ;

effects_clause = "effects" "{" ident ("," ident)* ","? "}" ;
capabilities_clause = "capabilities" "{" ident ("," ident)* ","? "}" ;
where_clause   = "where" where_item ("," where_item)* ;
where_item     = ident ":" ident ("+" ident)* ;
contract_clause = "requires" expr | "ensures" expr ;

struct_decl    = "struct" ident generics? "{" field ("," field)* ","? "}" ("invariant" expr)? ;
field          = ident ":" type ;

enum_decl      = "enum" ident generics? "{" variant ("," variant)* ","? "}" ;
variant        = ident | ident "(" type ")" ;

trait_decl     = "trait" ident generics? (";" | "{" trait_method_sig* "}") ;
trait_method_sig = ("async" | "unsafe")* "fn" ident generics? "(" params? ")" "->" type where_clause? effects_clause? capabilities_clause? ";" ;

impl_decl      = "impl" type (";" | "{" impl_method* "}") ;
impl_method    = ("async" | "unsafe")* "fn" ident generics? "(" params? ")" "->" type where_clause? effects_clause? capabilities_clause? contract_clause* block ;
```

### Expression precedence

```text
lowest
  ||
  &&
  == !=
  < <= > >=
  + -
  * / %
  unary: |closure|, &, &mut, await, -, !
  postfix: call (...), field ., try ?
highest
```

## Semantics and Rules

- Parsing is deterministic and single-pass with explicit error recovery at item and statement boundaries.
- `module` is optional for entry files, but non-entry modules loaded through imports must declare `module ...;`.
- `import` is explicit; transitive imports are not automatically visible.
- Imported modules expose public symbols only.
- `if` is an expression and always requires an `else` branch.
- `while`, `loop`, `break`, and `continue` are expressions, not special statement-only forms.
- Assignment is statement-only (`name = expr;`), never an expression.
- `let`, `return`, and assignment require trailing semicolons; parser emits deterministic fix suggestions when missing.
- `null` token is lexed but rejected semantically; absence must be modeled with `Option`.
- `extern` declarations are signatures only and must end with `;`; effects/contracts are not allowed on extern signatures.
- `intrinsic` declarations are signature-only runtime bindings, must end with `;`, and preserve effect declarations for callers.
- Intrinsic declarations with function bodies, contracts, or generics are rejected with stable parser diagnostics (`E1093`).
- `impl` declarations support:
  - inherent blocks for named type heads (`impl User { ... }`, `impl Status { ... }`)
  - trait impl declarations (`impl Score[Meter];`) and trait impl blocks (`impl Score[Meter] { ... }`)
- Inherent `impl` blocks are valid for both structs and enums.
- Tuple types and tuple literals use parentheses with commas; a single parenthesized expression remains grouping.
- Tuple field access uses numeric indices after `.` (`t.0`, `t.1`, ...).
- `|` is overloaded with context:
  - in expressions, `|...| -> ... { ... }` starts a closure
  - in patterns, `p1 | p2` is an or-pattern
- Line comments use `//` and run to end-of-line.
