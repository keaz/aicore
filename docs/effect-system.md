# Effect System (MVP)

Functions are pure by default.

Effect set syntax:

```aic
fn f() -> () effects { io, fs } { ... }
```

Capability authority syntax:

```aic
fn f() -> () effects { io, fs } capabilities { io, fs } { ... }
```

Rules:

- Callee effects must be subset of caller declared effects.
- Async callees follow the same rule; `await` does not erase effect obligations.
- Result propagation `?` preserves the effects of its operand expression.
- Standard effect set: `io`, `fs`, `net`, `time`, `rand`, `env`, `proc`, `concurrency`.
- Capability tokens use the same fixed taxonomy as effects.
- Effect declarations are canonicalized (sorted, deduplicated known effects) during frontend loading.
- Capability declarations are canonicalized (sorted, deduplicated known capabilities) during frontend loading.
- Unknown/duplicate effects are diagnostics.
- Unknown/duplicate capabilities are diagnostics.
- Interprocedural call-graph analysis computes transitive required effects.
- Pure callers that only indirectly reach effectful callees are rejected with a call-path diagnostic.
- Capability authority is enforced for declared and transitive effects with call-path provenance.
- Contracts are checked in pure mode.
- Selected resource APIs are checked for protocol safety (use-after-close / double-close).

## Resource Protocol Verification (QV-T2)

Compiler enforces stateful protocol rules for selected handles:

- `IntChannel`: valid while open for `send_int` / `recv_int`, terminal close via `close_channel`.
- `IntMutex`: valid while open for `lock_int` / `unlock_int`, terminal close via `close_mutex`.
- `Task`: terminal completion via `join_task` or `cancel_task`.
- `FileHandle`: valid while open for `file_read_line` / `file_write_str`, terminal close via `file_close`.
- TCP handle (`Int`): valid while open for `tcp_send` / `tcp_recv`, terminal close via `tcp_close`.
- UDP handle (`Int`): valid while open for `udp_send_to` / `udp_recv_from`, terminal close via `udp_close`.
- Process handle (`Int`): valid while open for `is_running`, terminal transitions via `wait` or `kill`.
- Async net ops (`AsyncIntOp`/`AsyncStringOp`): terminal consumption via `async_wait_int` / `async_wait_string`.

Violation model:

- calling an API after terminal close/consume is rejected at compile time
- repeated terminal calls are rejected at compile time
- branch-local analysis is conservative to avoid false positives on mixed control-flow paths

Diagnostics:

- `E2003`: unknown effect.
- `E2004`: duplicate effect declaration.
- `E2005`: transitive effect path requires undeclared effect.
- `E2006`: resource protocol violation (operation on closed/consumed handle).
- `E2007`: unknown capability.
- `E2008`: duplicate capability declaration.
- `E2009`: missing capability authority for declared/transitive effects.

Examples:

- Valid capability+protocol usage: `examples/verify/capability_protocol_ok.aic`
- Invalid capability usage: `examples/verify/capability_missing_invalid.aic`
- Invalid protocol usage: `examples/verify/file_protocol_invalid.aic`
