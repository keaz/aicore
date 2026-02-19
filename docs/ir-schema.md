# IR Schema (MVP)

Canonical serialization: JSON (`serde`), stable IDs and deterministic field ordering.

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

See `docs/id-allocation.md` for the full deterministic ID policy.
