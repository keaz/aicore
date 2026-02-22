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
- `E1060-E1062`: assignment statement parsing errors.
- `E1103`: unknown trait referenced in `impl`.
- `E1104`: trait impl arity mismatch.
- `E1105`: conflicting duplicate trait impl.
- `E1256`: `await` used outside an async function.
- `E1257`: `await` operand is not `Async[T]`.
- `E1258`: generic trait bound not satisfied by concrete type.
- `E1259`: invalid or unknown trait bound declaration.
- `E1260`: `?` operand is not `Result[T, E]`.
- `E1261`: `?` used in function without `Result` return type.
- `E1262`: `?` error type mismatch (`Result[_, E1]` in `Result[_, E2]` function).
- `E1263`: conflicting mutable borrow.
- `E1264`: immutable borrow while mutable borrow is active.
- `E1265`: assignment while borrow is active.
- `E1266`: assignment to immutable binding.
- `E1267`: mutable borrow of immutable binding.
- `E1268`: invalid borrow target (non-local expression).
- `E1269`: assignment type mismatch.
- `E1270`: match guard expression must be `Bool`.
- `E1271`: or-pattern alternatives bind different variable sets.
- `E1272`: or-pattern alternatives bind a variable with incompatible types.
- `E2110`: invalid package install spec or version requirement.
- `E2111`: package manifest read/write error for registry workflows.
- `E2112`: duplicate package version publish attempted.
- `E2113`: package metadata missing/invalid (`[package].name` or `[package].version`).
- `E2114`: semantic version resolution conflict across install requirements.
- `E2115`: package/version not found in configured registry.
- `E2116`: registry/index/package content IO or integrity failure.
- `E2117`: private registry authentication missing or invalid.
- `E2118`: registry configuration or credential source is invalid.
- `E5021`: backend lowering failure for invalid `?` operand/result layout.
- `E5022`: backend lowering failure for incompatible function `Result` return layout.
- `E5023`: backend does not yet lower guarded match arms.

## Change policy

- Never reuse a retired code for a different semantic error.
- Avoid deleting codes once published; keep compatibility for tooling.
- Diagnostic code changes are breaking for machine consumers and require migration notes.
