# AI Agent REST Implementation Guide (REST-T9)

This guide is the canonical agent-facing playbook for implementing and extending REST features in AICore.
Use it when adding request parsing, routing, JSON payload handling, async networking, and associated diagnostics/tests.

## 1. Non-Negotiable Delivery Rules

- Do not ship stubs/placeholders/dummy branches for issue-complete code paths.
- Do not mark an issue Done until all acceptance criteria are implemented and verified.
- Always run `make ci` before issue closure.
- Align every task with `AGENTS.md` DoD and verification policy.

## 2. Architecture Map

Use this as the minimum map before changing REST behavior.

| Concern | Primary files |
|---|---|
| Frontend orchestration and package load | `src/driver.rs`, `src/package_loader.rs` |
| Type/effect/contracts validation | `src/typecheck.rs`, `src/effects.rs`, `src/contracts.rs` |
| Runtime lowering and LLVM backend | `src/codegen/mod.rs` |
| REST stdlib API surface | `std/http_server.aic`, `std/router.aic`, `std/net.aic`, `std/json.aic`, `std/string.aic`, `std/map.aic` |
| Test coverage | `tests/unit_tests.rs`, `tests/execution_tests.rs`, `tests/e8_perf_tests.rs` |
| Example and CI wiring | `scripts/ci/examples.sh` |
| Cross-epic implementation context | `docs/ai-agent-implementation.md` |

Machine-checkable file reference set:

<!-- rest-guide:paths:start -->
AGENTS.md
docs/ai-agent-implementation.md
scripts/ci/examples.sh
src/codegen/mod.rs
src/contracts.rs
src/driver.rs
src/effects.rs
src/package_loader.rs
src/typecheck.rs
std/http_server.aic
std/json.aic
std/map.aic
std/net.aic
std/router.aic
std/string.aic
tests/e8_perf_tests.rs
tests/execution_tests.rs
tests/unit_tests.rs
<!-- rest-guide:paths:end -->

## 3. Where To Change What

| Change goal | Primary files | Required coverage |
|---|---|---|
| New REST stdlib API shape | `std/*.aic` + `src/codegen/mod.rs` intrinsic mapping | Unit delegate tests + execution behavior test + example |
| HTTP parse/serialize behavior | `src/codegen/mod.rs` runtime C section + `std/http_server.aic` | Execution test with malformed and valid requests |
| Route dispatch semantics | `std/router.aic` + `src/codegen/mod.rs` router runtime calls | Deterministic route precedence tests + example |
| JSON payload behavior for REST | `std/json.aic` + `src/codegen/mod.rs` JSON lowering/runtime | Roundtrip and negative-path execution tests |
| Async REST networking behavior | `std/net.aic` + `src/codegen/mod.rs` async runtime section | Multi-connection test + backpressure test + perf gate |

## 4. Deterministic End-To-End Workflow

Follow this sequence for every REST issue:

1. Confirm issue scope and acceptance criteria.
2. Map touched surfaces (stdlib signatures, type/effect checks, codegen/runtime, examples).
3. Add/adjust tests first:
   - `tests/unit_tests.rs` for API delegation/diagnostic checks.
   - `tests/execution_tests.rs` for runtime behavior and error paths.
   - `tests/e8_perf_tests.rs` if performance gates are in scope.
4. Implement frontend/backend/runtime changes.
5. Add or update runnable examples under `examples/io/` or `examples/data/`.
6. Wire examples into `scripts/ci/examples.sh` (check + run paths).
7. Update documentation (`README.md` and relevant `docs/*.md`).
8. Run full verification:
   - `make ci`
   - target-specific tests for the changed subsystem
9. Commit and push.
10. Only after all above: update project status and close issue with evidence.

## 5. Diagnostics Cookbook (REST-Focused)

| Code | Typical trigger | Deterministic fix |
|---|---|---|
| `E1301` | Calling `len(...)` without importing `std.string` | Add `import std.string;` in the source file using `len` |
| `E2001` | REST function performs `net`/`io`/`concurrency` work without declaring effects | Add required effects to function signature |
| `E2005` | Transitive effect missing in caller chain | Propagate effect declaration to calling boundaries |
| `E1248` | Non-exhaustive `Result` match for REST parse/network results | Cover both `Ok` and `Err` branches explicitly |
| `E1270` | Match guard is non-boolean | Ensure guard expression has `Bool` type |
| `E5023` | Guarded match reached backend lowering path | Hoist guard logic outside `match` in codegen-targeted flows |

## 6. Runnable REST Example Set

Required example categories and paths:

<!-- rest-guide:examples:start -->
request_parsing examples/io/http_server_hello.aic
routing examples/io/http_router.aic
json_roundtrip examples/data/config_json.aic
error_paths examples/data/url_http_negative_cases.aic
<!-- rest-guide:examples:end -->

These are already exercised by CI (`scripts/ci/examples.sh`), and should remain there.

<!-- docs-test:start -->
aic check examples/io/http_server_hello.aic
aic check examples/io/http_router.aic
aic check examples/data/config_json.aic
aic check examples/data/url_http_negative_cases.aic
<!-- docs-test:end -->

## 7. Agent Task Checklist (Issue Closure Gate)

Before marking any REST issue Done:

1. Full acceptance criteria implemented in code paths (no placeholders).
2. Relevant unit + execution + perf tests added/updated.
3. Example(s) added/updated and wired in CI.
4. Docs updated (this guide + task-specific docs as needed).
5. `make ci` passes locally.
6. Changes committed and pushed.
7. GitHub issue updated with commit hash and verification evidence.
8. Project item moved to Done only after steps 1-7.
