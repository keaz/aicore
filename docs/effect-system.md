# Effect System (MVP)

Functions are pure by default.

Effect set syntax:

```aic
fn f() -> () effects { io, fs } { ... }
```

Rules:

- Callee effects must be subset of caller declared effects.
- Async callees follow the same rule; `await` does not erase effect obligations.
- Result propagation `?` preserves the effects of its operand expression.
- Standard effect set: `io`, `fs`, `net`, `time`, `rand`, `env`, `proc`, `concurrency`.
- Effect declarations are canonicalized (sorted, deduplicated known effects) during frontend loading.
- Unknown/duplicate effects are diagnostics.
- Interprocedural call-graph analysis computes transitive required effects.
- Pure callers that only indirectly reach effectful callees are rejected with a call-path diagnostic.
- Contracts are checked in pure mode.
- Selected resource APIs are checked for protocol safety (use-after-close / double-close).

## Resource Protocol Verification (QV-T2)

Compiler enforces stateful protocol rules for selected `std.concurrent` handles:

- `IntChannel`: valid while open for `send_int` / `recv_int`, terminal close via `close_channel`.
- `IntMutex`: valid while open for `lock_int` / `unlock_int`, terminal close via `close_mutex`.
- `Task`: terminal completion via `join_task` or `cancel_task`.

Violation model:

- calling an API after terminal close/consume is rejected at compile time
- repeated terminal calls are rejected at compile time
- branch-local analysis is conservative to avoid false positives on mixed control-flow paths

Diagnostics:

- `E2003`: unknown effect.
- `E2004`: duplicate effect declaration.
- `E2005`: transitive effect path requires undeclared effect.
- `E2006`: resource protocol violation (operation on closed/consumed handle).

Examples:

- Valid protocol usage: `examples/verify/file_protocol.aic`
- Invalid protocol usage: `examples/verify/file_protocol_invalid.aic`
