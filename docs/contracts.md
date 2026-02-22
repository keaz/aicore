# Contracts (MVP)

Supported clauses:

- `requires <bool-expr>`
- `ensures <bool-expr>` (`result` available)
- `invariant <bool-expr>` on structs

Pipeline:

1. Parse and type-check contracts (`Bool` required).
2. Restricted static verifier proves/flags obligations over integer logic:
   - proven false => compile-time error
   - proven true => compile-time discharge note
   - unknown => emit residual-obligation note and keep runtime checks
3. Lower `requires`/`ensures` to runtime asserts:
   - `requires` runs once at function entry
   - `ensures` runs on every explicit `return` and implicit function exit
4. Struct invariants are enforced on construction by rewriting struct literals through synthesized invariant-check helper constructors.
5. Runtime assert failure calls `aic_rt_panic` with structured message text.

Diagnostics:

- `E4001`: statically false `requires`.
- `E4002`: statically false `ensures`.
- `E4003`: residual obligation kept for runtime check (note severity).
- `E4004`: statically false struct invariant.
- `E4005`: obligation discharged statically (note severity).

Reference example:

- `examples/verify/range_proofs.aic` (mixed discharged and residual obligations)
