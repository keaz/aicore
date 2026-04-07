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

- `E0007`: invalid float literal token.
- `E0008`: invalid char literal (must contain exactly one Unicode codepoint).
- `E1052`: invalid `async` item form (expected `async fn`).
- `E1053-E1059`: trait/impl declaration syntax errors.
- `E1060-E1062`: assignment statement parsing errors.
- `E1063-E1068`: `extern`/`unsafe` parsing and declaration form errors.
- `E1093`: invalid `intrinsic fn` declaration form (signature-only rules).
- `E1069-E1074`: `Fn(...) -> ...` type and closure literal parsing errors.
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
- `E1273`: `while` condition must be `Bool`.
- `E1274`: `break` expression type does not match enclosing loop break type.
- `E1275`: `break` used outside loop context.
- `E1276`: `continue` used outside loop context.
- `E1277`: use of moved value.
- `E1278`: move attempted while an overlapping borrow is active.
- `E1280`: closure parameter type must be explicit.
- `E1281`: closure body type does not match declared closure return type.
- `E1282`: generic function value cannot be used without specialization.
- `E1283`: effectful function cannot be converted to first-class function value.
- `E1284`: closure parameter count mismatches expected `Fn(...) -> ...` context.
- `E1285`: closure parameter type mismatches expected `Fn(...) -> ...` context.
- `E1286`: closure return type mismatches expected `Fn(...) -> ...` context.
- `E2110`: invalid package install spec or version requirement.
- `E2111`: package manifest read/write error for registry workflows.
- `E2112`: duplicate package version publish attempted.
- `E2113`: package metadata missing/invalid (`[package].name` or `[package].version`).
- `E2114`: semantic version resolution conflict across install requirements.
- `E2115`: package/version not found in configured registry.
- `E2116`: registry/index/package content IO or integrity failure.
- `E2117`: private registry authentication missing or invalid.
- `E2118`: registry configuration or credential source is invalid.
- `E2119`: trust policy denied package install (allow/deny/signature policy).
- `E2120`: invalid or unsupported extern ABI declaration.
- `E2121`: extern function signature uses unsupported language features.
- `E2122`: extern call requires explicit unsafe boundary.
- `E2123`: unsupported type in extern C-ABI signature.
- `E2124`: package signature verification/trusted-key validation failure.
- `E2125`: workspace manifest/member metadata is invalid or inconsistent.
- `E2126`: workspace package dependency cycle detected.
- `E5021`: backend lowering failure for invalid `?` operand/result layout.
- `E5022`: backend lowering failure for incompatible function `Result` return layout.
- `E5023`: backend const evaluation hit a cycle or an unsupported initializer form.
- `E5024`: backend extern wrapper/link ABI mismatch or unsupported extern lowering.
- `E5025`: backend encountered `break` outside a loop.
- `E5026`: backend encountered `continue` outside a loop.
- `E5031`: closure capture is unavailable in the current lowering scope.
- `E5032`: indirect call target is not a first-class `Fn(...) -> ...` value.
- `E5033`: closure parameter type is missing during backend lowering.
- `E5034`: backend cannot lower the referenced function as a first-class function value.
- `E5035`: closure helper return type mismatch during backend lowering.
- `E5036`: JSON encode/decode for function values is unsupported.
- `E6003`: typed hole (`_`) warning with inferred type/context.
- `E6004`: unused import warning with safe remove-import autofix.
- `E6005`: unreachable/unused function warning.
- `E6006`: unused variable warning with safe underscore-prefix autofix.
- `E6007`: historical Windows build target net/TLS strategy guard (retained for compatibility; not emitted after Windows parity landed).

## IO + Runtime Quick Reference

The table below captures high-frequency IO/runtime diagnostics with deterministic trigger guidance from the current implementation.

| Code | Trigger (current behavior) | Deterministic remediation |
|---|---|---|
| `E2001` | Effectful call is used without required `effects { ... }` declaration. | Add the missing effects to the enclosing function signature. |
| `E2002` | An effectful call appears inside a contract (`requires`/`ensures`/invariant), which must remain pure. | Remove side effects from contracts; move checks into executable code paths. |
| `E2003` | Unknown effect name in a function signature. | Use only known effects: `io, fs, net, time, rand, env, proc, concurrency`. |
| `E2004` | Duplicate effect listed in one function signature. | Remove duplicates; keep one declaration per effect. |
| `E2005` | Transitive effect required through call graph but not declared at caller boundary. | Declare the transitive effect at the root function or refactor call boundaries. |
| `E2006` | Resource protocol violation (operation on already-closed/consumed resource handle). | Recreate resource before reuse or reorder close/use lifecycle. |
| `E2007` | Unknown capability name in a function signature. | Use only known capabilities: `io, fs, net, time, rand, env, proc, concurrency`. |
| `E2008` | Duplicate capability listed in one function signature. | Remove duplicates; keep one declaration per capability. |
| `E2009` | Missing capability authority for declared or transitive effects. | Add `capabilities { ... }` to match required effect authority and thread capability boundaries across callers. |
| `E5023` | Backend const evaluation hit a cycle or an unsupported initializer form. | Keep `const` initializers to backend-supported literal/arithmetic forms or break the constant dependency cycle. |
| `E5024` | Unsupported extern backend lowering path (currently only `extern \"C\"` is supported). | Use `extern \"C\"` plain signatures and wrapper functions. |
| `E5025` | `break` reached backend outside loop context. | Ensure `break` is only emitted inside `loop`/`while`. |
| `E5026` | `continue` reached backend outside loop context. | Ensure `continue` is only emitted inside `loop`/`while`. |
| `E6001` | Deprecated std API usage warning (for example `std.time.now`). | Migrate to replacement API shown in diagnostic help (for example `std.time.now_ms`). |
| `E6002` | `aic std-compat --check` detected baseline incompatibility. | Keep compatibility (or deprecate first), then regenerate baseline only for intentional additive API change. |
| `E6003` | Typed hole (`_`) was accepted and inferred from context. | Replace `_` with the inferred concrete type when finalizing API/contracts. |
| `E6004` | Import was declared but never used. | Remove the import or use a symbol from that module. |
| `E6005` | Function is unreachable from entrypoint or otherwise unused. | Remove dead function code or invoke it from live call paths. |
| `E6006` | Local variable is never used. | Prefix with `_` to mark intentional non-use, or remove the binding. |
| `E6007` | Historical compatibility code for the retired Windows net/TLS build guard. Current builds should not emit it after Windows parity landed. | If it appears, you are likely using stale tooling or stale generated artifacts; rebuild with the current compiler/runtime. |

## Change policy

- Never reuse a retired code for a different semantic error.
- Avoid deleting codes once published; keep compatibility for tooling.
- Diagnostic code changes are breaking for machine consumers and require migration notes.
