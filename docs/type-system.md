# Type System (MVP)

- Strong static typing.
- No implicit casts/coercions.
- Types:
  - `Int`, `Bool`, `String`, `()`
  - named structs/enums
  - parametric surface syntax for ADTs (`Option[T]`, `Result[T,E]`)
  - compiler-managed async wrapper `Async[T]` for `async fn` call results
- Generic parameters support trait bounds (`T: Trait` and `T: TraitA + TraitB`).
- Trait bounds are satisfied only through explicit `impl Trait[Type];` declarations.
- Match exhaustiveness checking for Bool/Option/Result/enums.
- Match overlap/dead-arm detection with deterministic diagnostics.
- Pattern bindings are unique within a single pattern tree.
- `await` requires `Async[T]` and is valid only inside `async fn`.
- Result propagation `expr?` requires `expr: Result[T, E]` and enclosing return type `Result[U, E]`.
- `?` never performs implicit error conversion; mismatched `E` types are diagnostics.
- `null` is forbidden; absence is modeled only via `Option[T]`.
- Unknown symbols and type mismatches are reported with structured diagnostics.
