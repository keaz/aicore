# Type System (MVP)

- Strong static typing.
- No implicit casts/coercions.
- Types:
  - `Int`, `Bool`, `String`, `()`
  - named structs/enums
  - parametric surface syntax for ADTs (`Option[T]`, `Result[T,E]`)
- Match exhaustiveness checking for Bool/Option/Result/enums.
- Match overlap/dead-arm detection with deterministic diagnostics.
- Pattern bindings are unique within a single pattern tree.
- `null` is forbidden; absence is modeled only via `Option[T]`.
- Unknown symbols and type mismatches are reported with structured diagnostics.
