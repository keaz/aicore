# Modules and Imports Reference

See also: [Syntax](./syntax.md), [Expressions](./expressions.md), [Architecture](../architecture.md)

This page documents module/import behavior from package loading, resolution, and call visibility checks.

## Grammar

```ebnf
module_decl      = "module" module_path ";" ;
import_decl      = "import" module_path ";" ;
module_path      = ident ("." ident)* ;

qualified_call   = module_path "." ident "(" arg_list? ")" ;
unqualified_call = ident "(" arg_list? ")" ;
```

## Semantics and Rules

- Entry loading starts from:
  - file input path, or
  - directory input resolved to `aic.toml` main entry, else `src/main.aic`
- Package loader builds a module index by parsing reachable `.aic` files.
- Non-entry imported modules must declare `module ...;`.
- Duplicate module declarations for the same module path are rejected.
- Import resolution order:
  - indexed module declarations
  - fallback filesystem path lookup
  - std-module fallback under `std/`
- Import cycles are detected and reported deterministically.
- Resolver keeps separate namespaces:
  - value namespace: functions
  - type namespace: structs/enums/traits
  - module namespace: import aliases/full module paths
- Within one module and namespace, duplicate declarations are rejected.
- Trait impl coherence is checked per trait + concrete type-argument tuple.
- Direct-import visibility:
  - entry module sees symbols from itself plus directly imported modules
  - transitive imports are not re-exported automatically
- Qualified module calls must reference directly imported modules.
- Unqualified callable names become diagnostics when ambiguous across imported modules.
- Import tail alias collisions are tracked; ambiguous aliases are rejected and require explicit full qualification.

## Diagnostic mapping

- `E2100`: import cannot be resolved
- `E2101`: non-entry module missing `module` declaration
- `E2102`: symbol/module not visible without explicit import or qualification
- `E2103`: import cycle detected
- `E2104`: ambiguous import alias or callable
- `E2105`: duplicate module declaration
- `E2125` / `E2126`: workspace manifest and cycle errors for multi-package builds
