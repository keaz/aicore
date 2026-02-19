# Deterministic ID Allocation Policy (MVP)

Version: `id-policy-v1`

AICore assigns stable IDs during AST -> IR lowering. This policy is part of the deterministic compiler contract.

## ID spaces

- `SymbolId(u32)`
- `TypeId(u32)`
- `NodeId(u32)`

Each ID space is independent and monotonic.

## Allocation rules

1. Allocation starts at `1` for each ID space.
2. IDs are assigned in a single deterministic preorder traversal of the AST.
3. Symbols are allocated when declarations are lowered in source order:
   - item symbol (function/struct/enum)
   - item children (params/fields/variants) left-to-right
   - local bindings in statement order
4. Type IDs are interned by canonical textual representation (`repr`) and allocated on first encounter.
5. Node IDs are allocated on expression/pattern/block construction in lowering order.

## Stability guarantees

Given equivalent parsed AST (including after canonical formatting), generated IR IDs are byte-stable.

The following operations must not change ID assignment for unchanged source semantics:

- Running formatter and re-parsing
- Re-running the compiler on the same input

## Non-goals for MVP

- Preserving IDs across semantic source edits
- Global cross-module stable IDs

## Validation

- Unit tests enforce deterministic roundtrip behavior for ID assignments.
- Golden tests enforce parse -> IR -> print -> parse IR equivalence.
