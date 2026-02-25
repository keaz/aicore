# Type System (MVP)

- Strong static typing.
- No implicit casts/coercions.
- Types:
  - `Int`, `Float`, `Bool`, `String`, `()`
  - named structs/enums
  - parametric surface syntax for ADTs (`Option[T]`, `Result[T,E]`)
  - compiler-managed async wrapper `Async[T]` for `async fn` call results
- Generic parameters support trait bounds (`T: Trait` and `T: TraitA + TraitB`).
- Trait bounds are satisfied only through explicit `impl Trait[Type];` declarations.
- Match exhaustiveness checking for Bool/Option/Result/enums.
- Match overlap/dead-arm detection with deterministic diagnostics.
- Pattern bindings are unique within a single pattern tree.
- Pattern-or (`p1 | p2`) alternatives must bind identical name sets.
- Pattern-or bindings must have compatible types across alternatives.
- Match guards (`if <expr>`) must type-check to `Bool`.
- Guarded arms do not satisfy exhaustiveness coverage.
- `await` requires `Async[T]` and is valid only inside `async fn`.
- Result propagation `expr?` requires `expr: Result[T, E]` and enclosing return type `Result[U, E]`.
- `?` never performs implicit error conversion; mismatched `E` types are diagnostics.
- Borrow expressions produce reference wrapper types:
  - `&x` => `Ref[T]`
  - `&mut x` => `RefMut[T]`
- Assignment is type-checked (`name = expr`) and must match binding type.
- Typed holes (`_`) are accepted in type annotations (parameter, return, let, struct-field positions), infer from usage context, and emit warning `E6003` (not a hard error).
- Borrow/alias checks (MVP):
  - mutable borrow requires mutable binding (`E1267`)
  - conflicting mutable/immutable borrows are rejected (`E1263`, `E1264`)
  - assignment while borrowed is rejected (`E1265`)
  - assignment to immutable binding is rejected (`E1266`)
  - borrow target must be a local variable (`E1268`)
- Match-pattern diagnostics:
  - `E1270`: non-`Bool` guard expression
  - `E1271`: or-pattern binding name-set mismatch
  - `E1272`: or-pattern binding type mismatch
- `null` is forbidden; absence is modeled only via `Option[T]`.
- Unknown symbols and type mismatches are reported with structured diagnostics.

## Open issue contracts (current vs target)

Detailed per-issue contracts are tracked in:

- `docs/reference/open-issue-contracts.md`

Type-focused status:

- `#136` trait methods and dispatch
  - Current: trait bounds are marker-only.
  - Target: trait method signatures + impl method conformance + bounded method resolution (static dispatch MVP).
- `#137` borrow checker completeness
  - Current: alias/mutability checks for lexical local borrows (`E1263`-`E1269`).
  - Target: move/use-after-move checks, cross-call borrow reasoning, field-aware ownership checks.
- `#157` deterministic drop ordering
  - Current: runtime-drop locals (`String`, struct, enum) emit reverse-lexical `llvm.lifetime.end` cleanup at scope exits, and handle-backed locals (`FileHandle`, `IntChannel`, `IntMutex`) additionally perform real runtime close/cleanup calls on scope exit and early-return paths (`return`, `break`, `continue`, `?`).
  - Current: direct local move-outs for supported handle-backed resources (`let b = a`, direct `return a`, direct tail `a`) suppress cleanup on the moved-from local to preserve transferred ownership.
  - Target: full destructor invocation semantics (including user-defined `Drop`-style hooks), full move-out tracking across complex expressions, partial-move behavior, and unwind/panic-aware cleanup guarantees.
- `#138` generic constraints and `where`
  - Current: inline bounds (including `+`) only.
  - Target: equivalent constraint model across inline and `where` forms.
- `#139` improved inference
  - Current: local inference with deterministic unresolved failures (`E1204`, `E1212`, `E1280`).
  - Target: stronger local inference (closure-context and usage-driven) with explicit ambiguity diagnostics.

Related syntax issue with type impact:

- `#128` tuple types are not currently available; target adds tuple type/literal/pattern/projection typing rules.
