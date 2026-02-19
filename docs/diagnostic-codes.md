# Diagnostic Code Registry (MVP)

AICore diagnostics use stable codes in `E####` format.

Registry source of truth:

- `src/diagnostic_codes.rs` (`REGISTERED_DIAGNOSTIC_CODES`)

## Subsystem ranges

- `E0001-E0099`: lexer
- `E1001-E1099`: parser
- `E1100-E1199`: resolver
- `E1200-E1399`: type checker and exhaustiveness
- `E2001-E2099`: effect checker
- `E4001-E4099`: contracts
- `E5001-E5099`: LLVM/codegen/runtime lowering

## Enforcement

- `Diagnostic::error` validates code format and registry membership.
- New diagnostics must add a code to `src/diagnostic_codes.rs`.
- Unit tests fail if emitted codes are not registered.

## Change policy

- Never reuse a retired code for a different semantic error.
- Avoid deleting codes once published; keep compatibility for tooling.
- Diagnostic code changes are breaking for machine consumers and require migration notes.
