# Contracts (MVP)

Supported clauses:

- `requires <bool-expr>`
- `ensures <bool-expr>` (`result` available)
- `invariant <bool-expr>` on structs

Pipeline:

1. Parse and type-check contracts (`Bool` required).
2. Static simplification flags always-false contracts.
3. Lower `requires`/`ensures` to runtime assert statements.
4. Runtime assert failure calls `aic_rt_panic` with structured message text.

Limitations:

- `ensures` runtime lowering currently targets tail-return style functions.
