# Agent-First Language Feature Playbook

Purpose: map implemented AICore language features to practical AI-agent authoring loops.

Primary implementation references:
- [`README.md`](../../README.md)
- [`OriginalPrompt.md`](../../OriginalPrompt.md)
- [`src/parser.rs`](../../src/parser.rs)
- [`src/typecheck.rs`](../../src/typecheck.rs)
- [`docs/reference/`](../reference/)
- [`docs/diagnostic-codes.md`](../diagnostic-codes.md)

Agent rule: treat this playbook as the fast path for deciding which implemented language surface to use, but confirm final semantics against the linked reference docs and `aic check --json` before applying automated edits.

## Core Features (Issue #322)

For each feature area, use the table as an operational contract.

| Feature area | When to use | How to use | Common failure modes (diagnostics) | Verification commands | Canonical references |
|---|---|---|---|---|---|
| Syntax and parsing | Any source authoring/edit loop | Keep syntax canonical; prefer deterministic formatting after edits | Parser-family errors (`E1001-E1099`), intrinsic form errors (`E1093`) | `aic fmt <file> --check`, `aic check <file> --json` | [`syntax.md`](../reference/syntax.md) |
| Modules/imports | Multi-file/package decomposition | Require explicit imports; keep transitive dependencies explicit | Import/module errors (`E2100-E2105`) | `aic check <entry> --json`, `aic ir <entry> --emit json` | [`modules.md`](../reference/modules.md) |
| Visibility boundaries | Public API shaping across modules | Default to private; expose only required functions/types/fields with `pub` / `pub(crate)` | Visibility/access failures (`E12xx`), unresolved symbols (`E2102`) | `aic check <entry> --json` | [`modules.md`](../reference/modules.md) |
| Statements and control expressions | Binding/mutation/branching loops | Keep assignment statement-only; keep branch/loop typing explicit | Loop/control typing errors (`E1273-E1276`), named/positional call ordering (`E1092`) | `aic check <file> --json`, `aic fmt <file> --check` | [`statements.md`](../reference/statements.md), [`expressions.md`](../reference/expressions.md) |
| Types + aliases + consts | Reusable type intent and compile-time constants | Use `type` aliases for repeated signatures; keep `const` initializers pure/constant-safe | Alias/const parse errors (`E1075-E1081`), assignment type mismatch (`E1269`) | `aic check <file> --json`, `aic ir <file> --emit json` | [`types.md`](../reference/types.md) |
| Functions, async, extern/intrinsic | Core behavior + FFI/runtime boundaries | Start pure; add async/effects/unsafe explicitly; keep intrinsics declaration-only | `await` misuse (`E1256`,`E1257`), intrinsic shape errors (`E1093`), extern ABI errors (`E2120-E2124`) | `aic check <file> --json`, `aic verify-intrinsics --json` | [`syntax.md`](../reference/syntax.md), [`expressions.md`](../reference/expressions.md) |
| Option/Result and `?` propagation | Null-free absence and recoverable errors | Model absence with `Option[T]`; failure with `Result[T,E]`; use `?` only in compatible `Result` contexts | `?` misuse (`E1260-E1262`), typed-hole warning for unresolved placeholders (`E6003`) | `aic check <file> --json`, `aic explain E1260` | [`types.md`](../reference/types.md), [`expressions.md`](../reference/expressions.md) |

## Advanced Features (Issue #323)

Use these features only when their verification and diagnostics loops are part of the plan.

| Feature area | When to use | How to use | Pitfalls + diagnostics | Verification commands | Canonical references |
|---|---|---|---|---|---|
| Pattern matching | Enum/Option/Result exhaustive branching | Prefer `match` over nested conditionals for variant logic | Guard/or-pattern typing failures (`E1270-E1272`) and other `E12xx` match typing/exhaustiveness diagnostics | `aic check <file> --json`, `aic explain E1270` | [`pattern-matching.md`](../reference/pattern-matching.md) |
| Generics + trait bounds + `where` | Reusable constrained APIs | Declare explicit bounds inline and/or in `where`; avoid implicit assumptions | Bound failures (`E1258`), invalid bound declarations (`E1259`), unspecialized generic function values (`E1282`) | `aic check <file> --json`, `aic ir <file> --emit json` | [`generics.md`](../reference/generics.md) |
| Effects + capabilities + protocol discipline | Side-effect and authority-safe APIs | Declare both `effects { ... }` and `capabilities { ... }`; keep resource lifecycle legal | Effect/capability/protocol errors (`E2001-E2009`) | `aic check <file> --json`, `aic suggest-effects <file>` | [`effects.md`](../reference/effects.md) |
| Contracts (`requires/ensures/invariant`) | Semantic correctness guards at API boundaries | Keep contract expressions pure; encode executable intent in pre/post/invariant clauses | Contract failures (`E4001-E4005`), effectful contract expressions (`E2002`) | `aic check <file> --json`, `aic run <file>` | [`contracts.md`](../reference/contracts.md) |
| Memory/borrows/drop | Alias/mutation-safe local ownership | Keep borrow scopes tight; avoid assignment while borrowed; rely on deterministic cleanup model | Borrow/mutation failures (`E1263-E1269`) | `aic check <file> --json`, optional `aic run <file> --check-leaks` | [`memory.md`](../reference/memory.md) |
| Iterators and `for-in` | Lazy pipeline composition and collection traversal | Use `iter().map().filter().take().collect()` for deterministic transform chains | Usually surfaced as type/bound failures (`E1258`,`E1269`) when iterator shapes mismatch | `aic check <file> --json`, `aic run <file>` | [`iterators.md`](../reference/iterators.md) |
| Dyn trait objects | Runtime polymorphism where static dispatch is impractical | Use object-safe traits and explicit impl blocks; store/pass as `dyn Trait` | Object-safety/type mismatch diagnostics (`E12xx` type-check family) | `aic check <file> --json`, `aic run <file>` | [`dyn-trait-objects.md`](../reference/dyn-trait-objects.md) |

## Guaranteed vs Open-Contract Behavior

Guaranteed (ship-level) behavior comes from:
- [`docs/reference/*.md`](../reference/)
- [`src/parser.rs`](../../src/parser.rs) + [`src/typecheck.rs`](../../src/typecheck.rs)
- [`docs/diagnostic-codes.md`](../diagnostic-codes.md)

Open/future behavior must be treated as non-guaranteed backlog and verified before automation depends on it:
- [`docs/reference/open-issue-contracts.md`](../reference/open-issue-contracts.md)

Rule for agents: if a workflow depends on an open-contract item, gate it behind explicit `aic check --json` and fail closed.

## Automation Guardrails

1. Edit in smallest compilable increments.
2. Run `aic fmt <file> --check` before and after semantic edits.
3. Run `aic check <entry> --json` after each feature-level step.
4. Use `aic explain <CODE>` to generate deterministic remediation plans.
5. For API-sensitive changes, run `aic diff --semantic <old> <new> --fail-on-breaking`.
