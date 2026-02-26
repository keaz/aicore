# Capability-Safe Effects And Resource Protocols

This guide defines capability authority for effects and the protocol checker behavior for lifecycle-sensitive resources.

## Capability Tokens

Supported capability tokens are fixed and machine-checkable:

- `io`
- `fs`
- `net`
- `time`
- `rand`
- `env`
- `proc`
- `concurrency`

Use them in function signatures:

```aic
fn run() -> Int effects { fs, net } capabilities { fs, net } {
    0
}
```

## Signature Threading Rules

- Effects declare which side effects a function may perform.
- Capabilities declare authority to perform those effects.
- Capability requirements are enforced for direct and transitive call paths.
- Missing capability authority emits `E2009` with deterministic call-path provenance.
- `aic diag apply-fixes` provides deterministic capability insertion/upgrade fixes.

## Async, Higher-Order, And Modules

- `async fn` follows the same effect+capability authority rules.
- `await` does not erase capability obligations.
- Higher-order calls rely on the called function signature and resolved call graph.
- Module boundaries preserve the same authority contract: callers must thread required capabilities explicitly.

## Migration From Declaration-Only Effects

1. Keep existing `effects { ... }` clauses.
2. Add matching `capabilities { ... }` clauses.
3. Run `aic suggest-effects` to find missing transitive authority.
4. Run `aic diag apply-fixes --json` for deterministic one-pass remediation.

## Resource Protocol Diagnostics

`E2006` is emitted for invalid lifecycle transitions (use-after-close / post-terminal operations):

- `std.concurrent`: `IntChannel`, `IntMutex`, `Task`
- `std.fs`: `FileHandle`
- `std.net`: TCP/UDP handles + async wait handles
- `std.proc`: process handles (`wait`/`kill` terminal transitions)

Diagnostic catalog references:

- `docs/diagnostic-codes.md`
- `docs/errors/catalog.md`

## Executable Examples

- `examples/verify/capability_protocol_ok.aic`
- `examples/verify/capability_missing_invalid.aic`
- `examples/verify/fs_protocol_ok.aic`
- `examples/verify/fs_protocol_invalid.aic`
- `examples/verify/net_proc_protocol_ok.aic`
- `examples/verify/net_proc_protocol_invalid.aic`

## One-Pass Agent Remediation

Use this deterministic loop:

1. `aic check <file> --json`
2. `aic suggest-effects <file>`
3. `aic diag apply-fixes <file> --json`
4. `aic check <file> --json`

## Docs Test

<!-- docs-test:start -->
aic check examples/verify/capability_protocol_ok.aic --json
! aic check examples/verify/capability_missing_invalid.aic --json
aic check examples/verify/fs_protocol_ok.aic --json
! aic check examples/verify/fs_protocol_invalid.aic --json
aic check examples/verify/net_proc_protocol_ok.aic --json
! aic check examples/verify/net_proc_protocol_invalid.aic --json
! aic suggest-effects examples/e7/suggest_effects_demo.aic
aic diag apply-fixes examples/e7/suggest_effects_demo.aic --dry-run --json
<!-- docs-test:end -->
