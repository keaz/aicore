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
- `E2100-E2199`: package/module loading and lockfile workflow
- `E4001-E4099`: contracts
- `E5001-E5099`: LLVM/codegen/runtime lowering
- `E6001-E6099`: std compatibility and deprecation policy

## Enforcement

- `Diagnostic::error` validates code format and registry membership.
- New diagnostics must add a code to `src/diagnostic_codes.rs`.
- Unit tests fail if emitted codes are not registered.
- `aic explain <CODE>` provides deterministic remediation guidance for all registered codes.

Recent core-language additions:

- `E1052`: invalid `async` item form (expected `async fn`).
- `E1053-E1059`: trait/impl declaration syntax errors.
- `E1103`: unknown trait referenced in `impl`.
- `E1104`: trait impl arity mismatch.
- `E1105`: conflicting duplicate trait impl.
- `E1256`: `await` used outside an async function.
- `E1257`: `await` operand is not `Async[T]`.
- `E1258`: generic trait bound not satisfied by concrete type.
- `E1259`: invalid or unknown trait bound declaration.

## Change policy

- Never reuse a retired code for a different semantic error.
- Avoid deleting codes once published; keep compatibility for tooling.
- Diagnostic code changes are breaking for machine consumers and require migration notes.
