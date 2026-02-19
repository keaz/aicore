# Effect System (MVP)

Functions are pure by default.

Effect set syntax:

```aic
fn f() -> () effects { io, fs } { ... }
```

Rules:

- Callee effects must be subset of caller declared effects.
- Standard effect set: `io`, `fs`, `net`, `time`, `rand`.
- Unknown/duplicate effects are diagnostics.
- Contracts are checked in pure mode.
