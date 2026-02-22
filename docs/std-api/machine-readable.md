# Std API Machine-Readable Contract

This document defines the machine-readable contract for std API snapshots produced by:

```bash
cargo run --quiet --bin aic -- std-compat
```

The command prints JSON matching `/Users/kasunranasinghe/Projects/Rust/aicore/src/std_policy.rs` data structures.

## Snapshot schema

Top-level shape:

```json
{
  "schema_version": 1,
  "symbols": [
    {
      "module": "std.fs",
      "kind": "fn",
      "signature": "read_file(path: String) -> Result[String, FsError] effects { fs }"
    }
  ]
}
```

Field contract:

- `schema_version` (`u32`): currently `1`.
- `symbols` (`array`): sorted and deduplicated list of exported std API symbols.
- `symbols[].module` (`string`): declared module path (for example `std.fs`).
- `symbols[].kind` (`string`): one of `fn`, `struct`, `enum`, `trait`, `impl`.
- `symbols[].signature` (`string`): normalized textual signature emitted by the snapshot renderer.

## Determinism guarantees

Snapshot generation is deterministic for a given std tree:

- `.aic` files under `std/` are collected recursively and sorted by path.
- symbols are rendered from parsed AST items.
- rendered symbol list is globally sorted and deduplicated.

This enables stable CI comparisons and reproducible baseline updates.

## Compatibility semantics

`aic std-compat --check --baseline <path>` compares:

- `baseline.symbols - current.symbols` => breaking changes
- `current.symbols - baseline.symbols` => additive changes

Behavior:

- if breaking set is non-empty, command exits with diagnostic error and reports `E6002`.
- if breaking set is empty, check passes and reports additive count.

## Consumer contract for automation

Recommended CI assertions:

1. baseline JSON parses into `StdApiSnapshot` shape.
2. `schema_version == 1`.
3. symbol tuples `(module, kind, signature)` are unique.
4. compatibility check reports no breaking symbols.

Example CI command:

```bash
cargo run --quiet --bin aic -- std-compat --check --baseline /Users/kasunranasinghe/Projects/Rust/aicore/docs/std-api-baseline.json
```

## Updating the baseline intentionally

When making intentional std API additions or planned compatibility changes:

1. update std declarations,
2. regenerate baseline JSON,
3. update docs and migration notes,
4. rerun compatibility check to ensure resulting baseline is internally consistent.

## Notes on errors and examples

The JSON snapshot itself records signatures, not runtime behavior. Human-facing docs should pair each symbol with:

- effect requirements,
- error enum semantics for `Result` APIs,
- concise compile-intent examples aligned with AIC syntax.
