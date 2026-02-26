# Effect Protocol Verification

This runbook documents resource protocol checks in `src/typecheck.rs`.

## Effect Rules

- Functions are pure by default.
- Declared effects must include all direct and transitive callee effects.
- Declared capabilities must authorize all declared/transitive effects.
- Unknown/duplicate effects are rejected.
- Unknown/duplicate capabilities are rejected.
- Protocol-checked resource APIs emit `E2006` on invalid transitions.

## Protocol State Model

Tracked resource kinds:

- `IntChannel`
- `IntMutex`
- `Task`
- `FileHandle`
- TCP handle (`Int`)
- UDP handle (`Int`)
- process handle (`Int`)
- async net handles (`AsyncIntOp`, `AsyncStringOp`)

Operation classes:

- non-terminal operations keep resource open (`send_int`, `recv_int`, `lock_int`, `unlock_int`)
- terminal operations close/consume resource (`close_channel`, `close_mutex`, `join_task`, `cancel_task`)

Invalid transitions:

- non-terminal operation after terminal operation on same handle
- repeated terminal operation on same handle

Control-flow policy:

- branch-local analysis is conservative to avoid false positives in mixed branches

## Valid and Invalid Examples

Valid transitions:

```bash
aic check examples/verify/file_protocol.aic --json
aic check examples/verify/capability_protocol_ok.aic --json
```

Invalid transitions (`E2006`, `E2009`):

```bash
aic check examples/verify/file_protocol_invalid.aic --json
aic check examples/verify/net_proc_protocol_invalid.aic --json
aic check examples/verify/capability_missing_invalid.aic --json
```

## Verifier-Friendly Pattern

Use single-owner handle flow:

1. allocate/acquire handle
2. perform all non-terminal operations
3. perform one terminal operation exactly once
4. do not reuse handle symbol after terminal operation
