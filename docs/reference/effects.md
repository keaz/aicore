# Effects Reference

See also: [Expressions](./expressions.md), [Contracts](./contracts.md), [Modules](./modules.md)

This page documents declared effect syntax and enforcement in `src/effects.rs` and `src/typecheck.rs`.

## Grammar

```ebnf
effects_clause  = "effects" "{" effect_name ("," effect_name)* ","? "}" ;
effect_name     = ident ;

fn_decl         = "fn" ident ... "->" type effects_clause? ... block ;
async_fn_decl   = "async" "fn" ident ... "->" type effects_clause? ... block ;
```

## Semantics and Rules

- Functions are pure by default when no `effects { ... }` clause is present.
- Known effects are fixed by `KNOWN_EFFECTS`:
  - `io`, `fs`, `net`, `time`, `rand`, `env`, `proc`, `concurrency`
- Frontend normalization canonicalizes effect signatures:
  - unknown effect names are rejected
  - duplicate effect names are rejected
  - accepted effect sets are sorted deterministically
- Direct-call rule: callee declared effects must be a subset of caller declared effects.
- Transitive rule: call-graph closure is analyzed; if caller can reach an effect transitively, that effect must be declared on the caller.
- Contract purity rule: `requires`, `ensures`, and struct `invariant` expressions are pure contexts; effectful calls inside them are rejected.
- Async does not bypass effect accounting:
  - calling `async fn` still contributes the callee effect set
  - `await` does not erase effect obligations
- Resource protocol checks under `concurrency` detect invalid use-after-close/double-close patterns for selected handles:
  - `IntChannel`: `send_int`, `recv_int`, terminal `close_channel`
  - `IntMutex`: `lock_int`, `unlock_int`, terminal `close_mutex`
  - `Task`: terminal `join_task` or `cancel_task`
- Diagnostics are stable and machine-readable (`E2001`, `E2002`, `E2003`, `E2004`, `E2005`, `E2006`).
