# Type System (MVP)

- Strong static typing.
- No implicit casts/coercions.
- Types:
  - `Int`, `Bool`, `String`, `()`
  - named structs/enums
  - parametric surface syntax for ADTs (`Option[T]`, `Result[T,E]`)
- Match exhaustiveness checking for Bool/Option/Result/enums.
- Unknown symbols and type mismatches are reported with structured diagnostics.
