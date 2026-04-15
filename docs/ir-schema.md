# IR Schema

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
- `Function.is_intrinsic: bool`
- `Function.intrinsic_abi: Option<String>`
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
- legacy unversioned JSON (v0) is migrated by `aic ir-migrate` (and by `aic migrate` when scanning project trees).

See `docs/id-allocation.md` for the full deterministic ID policy.

## Self-Hosted IR Serialization

The AIC implementation in `compiler/aic/libs/ir` emits a versioned self-host IR envelope:

- `format: "aicore-selfhost-ir-v1"`
- `schema_version: 1`
- `module` as the Rust-compatible segment array or `null`
- `module_path` as the self-host dotted path
- `imports` as segment arrays
- `items`, `symbols`, `types`, and `generic_instantiations` in deterministic lowering order
- `source_map` with `{ kind, name, node, span }` entries for parity diagnostics
- `span` with `{ start, end }` for Rust-compatible spans; source-map and diagnostic spans additionally carry `source`

The self-host JSON intentionally keeps alias and const items as explicit `TypeAlias` and `Const` item variants while preserving `runtime_visible: false`; these are self-host metadata surfaces and do not add runtime functions. Rust-compatible frontend IR still uses the existing `aic ir --emit json` shape. The parity harness compares `ir-json` output through canonical JSON fingerprints, so whitespace and object-key order do not affect parity results.

Serialization validation emits deterministic diagnostics before artifact output:

- `E5010`: schema version is not the current self-host IR schema.
- `E5011`: item-bearing IR is missing source-map metadata.
- `E5012`: symbols, types, or source-map entries are not in stable increasing ID order.
- `E5013`: required serialization metadata such as symbol/type/source-map names is empty.

The self-host report surfaces are:

- `serialize_ir_program(program)` for the JSON/debug/digest/diagnostic bundle.
- `ir_program_to_json(program)` for canonical self-host IR JSON.
- `ir_lowering_result_to_json(result)` for lowering output plus diagnostics.
- `ir_program_to_debug_text(program)` for deterministic developer-readable summaries.
- `ir_program_to_parity_artifact_json(program, case, path)` for parity artifacts that can be stored under `target/selfhost-parity/`.
