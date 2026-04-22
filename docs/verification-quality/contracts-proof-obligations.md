# Contracts Proof Obligations

This runbook documents the static verifier model in `compiler/aic/libs/typecheck/src/main.aic`.

## Obligation Types

- `requires`: checked against parameter ranges at function entry.
- `ensures`: checked against all explicit returns and implicit tail return.
- `invariant`: checked on struct construction through synthesized helper constructors.

Diagnostics:

- `E4001`: statically false `requires`
- `E4002`: statically false `ensures`
- `E4003`: residual obligation kept for runtime assertion
- `E4004`: statically false `invariant`
- `E4005`: obligation discharged at compile time

## Theorem Subset (Supported)

The static prover is intentionally small and deterministic. It supports:

- boolean literals and `!`, `&&`, `||`
- integer literals and range reasoning for variables
- integer comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`
- integer arithmetic in range form: unary `-`, binary `+`, `-`
- path-sensitive return analysis for `ensures` over `if`-based control flow

Anything outside this subset becomes `E4003` (residual runtime check), not an unsound proof.

## Failure Example and Fix

Failing proof (`E4002`):

```bash
aic check examples/verify/qv_contract_proof_fail.aic --json
```

Fixed version:

```bash
aic check examples/verify/qv_contract_proof_fixed.aic --json
aic run examples/verify/qv_contract_proof_fixed.aic
```

## Authoring Guidance

- Prefer simple linear integer invariants (`>=`, `<=`, `+`, `-`).
- Keep postconditions local to one return expression when possible.
- Use helper functions to isolate complex arithmetic and expose simple contracts.
- If `E4003` appears repeatedly, simplify the contract or split logic so the prover sees tighter ranges.
