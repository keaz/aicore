# Self-Hosting Parity

This document defines the gate for moving AICore toward a compiler written in AICore while preserving the current Rust compiler behavior.

## Scope

The self-hosting path is compiler work only:

- compiler support libraries live outside `std/`
- tool entrypoints import compiler libraries instead of embedding compiler logic
- the Rust compiler remains the reference until parity gates pass
- no new core-language semantics are introduced by the self-hosting work
- runtime, protocol, service, and application helpers stay in separate libraries

Suggested package boundaries:

- `compiler/aic/libs/source`
- `compiler/aic/libs/diagnostics`
- `compiler/aic/libs/syntax`
- `compiler/aic/libs/lexer`
- `compiler/aic/libs/parser`
- `compiler/aic/libs/ast`
- `compiler/aic/libs/ir`
- `compiler/aic/libs/frontend`
- `compiler/aic/libs/backend_llvm`
- `compiler/aic/libs/driver`
- `compiler/aic/tools/aic_selfhost`
- `compiler/aic/tools/aic_parity`

Current implemented package slice:

- `compiler/aic/libs/source` models source IDs, spans, locations, range operations, and source-span relations.
- `compiler/aic/libs/diagnostics` models diagnostic severity, diagnostic codes, help text, text edits, fixes, and primary diagnostic records.
- `compiler/aic/libs/syntax` models token kinds, token spans, lexemes, and token classification helpers, including the core visibility keywords used by the Rust front end.
- `compiler/aic/libs/lexer` scans the current ASCII lexical surface for identifiers, keywords, numeric/string/char/template literals, comments, whitespace, delimiters, and operator spellings into `compiler.syntax` tokens and EOF-terminated token streams.
- `compiler/aic/libs/parser` models the token-stream cursor, expectation diagnostics, dotted module-path parsing, `module`/`import` declaration parsing and ordering, visibility parsing, top-level item header parsing, structured type-reference parsing for unit, hole, named/path, generic application, tuple, `Fn(...) -> ...`, and `dyn Trait` types, parameter-list parsing, function-signature parsing with structured generic parameters, where-clause bound merging, and optional effects/capabilities lists, expression parsing with precedence for unary, binary, call, field-access, `await`, and `?` forms, pattern parsing for wildcard, literal, tuple, variant, struct, and or-pattern forms, block parsing with let/assignment/return/expression statements and tail expressions, function-declaration parsing with structured requires/ensures expressions and bodies, struct-declaration parsing with structured field default and invariant expressions, enum-declaration parsing with optional single-type variant payloads, `type`/`const` declaration parsing with structured const initializer expressions, trait-declaration parsing with method signatures, impl-declaration parsing with structured method signatures and method bodies, top-level and nested declaration attribute parsing with framework-attribute validation, item recovery with deterministic diagnostic ordering and max-error truncation, and full program parsing with source-map entries for later front-end stages.
- `compiler/aic/libs/ast` models AST names, module paths, module declarations, import declarations, top-level item headers, attributes and attribute arguments on items, params, fields, variants, and method signatures, function signatures with parameters and generic parameter lists, structured expression, pattern, statement, block, and match-arm nodes, function declarations with contract expressions and bodies, struct declarations with fields/default/invariant expressions, enum declarations with variants, type alias declarations, const declarations with initializer expressions, trait declarations, impl declarations with method bodies, program items with item visibility, full program AST roots, source-map entries, flat structured type descriptors with type-node metadata, and literal descriptors.
- `compiler/aic/libs/ir` models stable IR node, symbol, and type IDs plus IR symbol/type/function descriptors.
- `compiler/aic/libs/frontend` models self-hosted resolver output for modules, imports, symbols, references, type/value/member namespaces, deterministic symbol IDs, duplicate diagnostics, missing import/module diagnostics, trait impl discovery, enum variant discovery, and same-module versus imported visibility checks.
- `compiler/aic/tools/source_diagnostics_check` imports the implemented libraries and validates the data model through a small executable tool.

## Parity Harness

The initial gate is:

```bash
make test-selfhost
make selfhost-parity
```

`make test-selfhost` tests the parity harness with deterministic test compiler scripts. It does not depend on the current `aic` binary.

`make selfhost-parity` compares a reference compiler command against a candidate compiler command using `tests/selfhost/parity_manifest.json`. By default, both sides use the current Rust compiler command:

```bash
python3 scripts/selfhost/parity.py \
  --manifest tests/selfhost/parity_manifest.json \
  --reference "cargo run --quiet --bin aic --" \
  --candidate "cargo run --quiet --bin aic --"
```

When an AICore-built compiler exists, run:

```bash
python3 scripts/selfhost/parity.py \
  --manifest tests/selfhost/parity_manifest.json \
  --reference "cargo run --quiet --bin aic --" \
  --candidate "path/to/aic_selfhost"
```

The report is written to `target/selfhost-parity/report.json`.

## Required Coverage

The parity manifest should grow in lockstep with the AICore compiler port. It must cover:

- pass and fail frontend diagnostics
- lexer and parser recovery
- canonical formatting
- canonical IR JSON
- resolver and visibility errors
- type, effect, borrow, pattern, and contract checks
- LLVM emission
- executable behavior for representative examples
- deterministic output across repeated runs

Each porting issue must add manifest cases for the compiler surface it implements and keep the existing Rust compiler output unchanged.
