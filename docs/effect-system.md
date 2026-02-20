# Effect System (MVP)

Functions are pure by default.

Effect set syntax:

```aic
fn f() -> () effects { io, fs } { ... }
```

Rules:

- Callee effects must be subset of caller declared effects.
- Async callees follow the same rule; `await` does not erase effect obligations.
- Standard effect set: `io`, `fs`, `net`, `time`, `rand`.
- Effect declarations are canonicalized (sorted, deduplicated known effects) during frontend loading.
- Unknown/duplicate effects are diagnostics.
- Interprocedural call-graph analysis computes transitive required effects.
- Pure callers that only indirectly reach effectful callees are rejected with a call-path diagnostic.
- Contracts are checked in pure mode.

Diagnostics:

- `E2003`: unknown effect.
- `E2004`: duplicate effect declaration.
- `E2005`: transitive effect path requires undeclared effect.
