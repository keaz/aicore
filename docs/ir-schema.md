# IR Schema (MVP)

Canonical serialization: JSON (`serde`), stable IDs and deterministic field ordering.

Schema version: `1` (`CURRENT_IR_SCHEMA_VERSION`).

Core IDs:

- `SymbolId(u32)`
- `TypeId(u32)`
- `NodeId(u32)`

Core entities:

- `Program`
- `Symbol { id, name, kind, span }`
- `TypeDef { id, repr }`
- `Item::{Function,Struct,Enum}`
- `Expr` / `Stmt` / `Pattern`

Invariants:

- IDs are allocated by deterministic source traversal.
- `types` is interned by canonical textual `repr`.
- printer operates on IR, not source text.
- `Program.schema_version` is always emitted.
- legacy unversioned JSON (v0) is migrated by `aic ir-migrate`.

See `docs/id-allocation.md` for the full deterministic ID policy.
