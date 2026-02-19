# Syntax Reference (MVP)

```ebnf
program        = module_decl? import_decl* item* EOF ;
module_decl    = "module" path ";" ;
import_decl    = "import" path ";" ;
path           = ident ("." ident)* ;

item           = fn_decl | struct_decl | enum_decl ;
fn_decl        = "fn" ident generics? "(" params? ")" "->" type effects? contract* block ;
struct_decl    = "struct" ident generics? "{" fields? "}" invariant? ;
enum_decl      = "enum" ident generics? "{" variants? "}" ;

generics       = "[" ident ("," ident)* "]" ;
params         = param ("," param)* ;
param          = ident ":" type ;
fields         = field ("," field)* ;
field          = ident ":" type ;
variants       = variant ("," variant)* ;
variant        = ident ("(" type ")")? ;

effects        = "effects" "{" ident ("," ident)* "}" ;
contract       = ("requires" expr) | ("ensures" expr) ;
invariant      = "invariant" expr ;

type           = "()" | ident ("[" type ("," type)* "]")? ;
block          = "{" stmt* expr? "}" ;
stmt           = let_stmt | return_stmt | expr ";" ;
let_stmt       = "let" ident (":" type)? "=" expr ";" ;
return_stmt    = "return" expr? ";" ;

expr           = logical_or ;
pattern        = "_" | ident | int | bool | "()" | ident "(" pattern ("," pattern)* ")" ;
```
