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
- The project root is the nearest ancestor directory of the entry file that contains `aic.toml`. Without a manifest it is the entry file's own directory.
- The loader parses only what the entry needs. It never walks the project directory up front, so checking a single file costs the same in a large or cluttered directory (for example `/tmp`) as in an empty one, regardless of path form or working directory.
- Non-entry imported modules must declare `module ...;`.
- Import resolution order (first match wins):
  - modules already loaded, and modules declared by resolved package dependencies
  - for `std.*` imports, the std roots (first match wins):
    - `AIC_STD_ROOT` when set
    - project-local `std/` (legacy override)
    - bundled std shipped with the CLI binary
    - global toolchain std (`~/.aic/toolchains/<aic-version>/std`) installed via `aic setup`
  - for other imports, the conventional file `<root>/a/b.aic` or `<root>/src/a/b.aic` for `import a.b;`, when that file declares `module a.b;`
  - otherwise the loader indexes the project root once, parsing every `.aic` file below it, and resolves the import by its `module` declaration. The index skips hidden directories (`.git`, `.aic-cache`, ...), `target`, `node_modules`, unreadable directories and files, and visits symlinked directories at most once.
  - finally, the conventional file path even if its `module` declaration differs.
- Errors in files that are never imported (syntax errors, invalid UTF-8) do not affect the check.
- Duplicate module declarations for the same module path are rejected with `E2105` when that module is imported. Duplicates of modules nobody imports are not reported. The entry file may declare the same module as another file (for example the copy `aic test` checks); the entry shadows the other declaration.
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
- Unqualified function names resolve with a fixed shadowing order, so user code can reuse names that std also defines (`count`, `size`, `len`, `get`, ...):
  1. a function defined in the current module
  2. a function from an imported user or package (non-std) module
  3. a function from an imported std module
  If more than one candidate remains at the winning level (for example two imported user modules both define `size`, or `std.vec` and `std.map` both provide `get` and the program defines none), the call is ambiguous (`E2104`); qualify it as `module.name(...)`. The same order applies to a function used as a value (`apply(count, 1)`).
- Method calls (`value.get()`) resolve to methods of the receiver's type and never to a free function from another module; effect and capability checks follow the method's own body.
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
