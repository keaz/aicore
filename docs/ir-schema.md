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
- `GenericInstantiation { id, kind, name, symbol, type_args, mangled }`
- `Item::{Function,Struct,Enum}`
- `Item::{Function,Struct,Enum,Trait,Impl}`
- `Expr` / `Stmt` / `Pattern`

Notable function/expression fields:

- `Function.is_async: bool`
- `GenericParam.bounds: Vec<String>`
- `Stmt::Let.mutable: bool`
- `Stmt::Assign { target, expr, span }`
- `ExprKind::Borrow { mutable, expr }`
- `ExprKind::Await { expr }`
- `ExprKind::Try { expr }`
- `MatchArm.guard: Option<Expr>`
- `PatternKind::Or { patterns }`

Invariants:

- IDs are allocated by deterministic source traversal.
- `types` is interned by canonical textual `repr`.
- `generic_instantiations` is deduplicated and stably ordered by canonical instantiation key.
- printer operates on IR, not source text.
- `Program.schema_version` is always emitted.
- legacy unversioned JSON (v0) is migrated by `aic ir-migrate`.

See `docs/id-allocation.md` for the full deterministic ID policy.
