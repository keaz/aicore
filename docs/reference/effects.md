# Effects Reference

See also: [Expressions](./expressions.md), [Contracts](./contracts.md), [Modules](./modules.md)

This page documents declared effect syntax and enforcement in `compiler/aic/libs/typecheck/src/main.aic` and `compiler/aic/libs/typecheck/src/main.aic`.

## Grammar

```ebnf
effects_clause  = "effects" "{" effect_name ("," effect_name)* ","? "}" ;
capabilities_clause = "capabilities" "{" capability_name ("," capability_name)* ","? "}" ;
effect_name     = ident ;
capability_name = ident ;

fn_decl         = "fn" ident ... "->" type effects_clause? capabilities_clause? ... block ;
async_fn_decl   = "async" "fn" ident ... "->" type effects_clause? capabilities_clause? ... block ;
extern_fn_decl  = "extern" string "fn" ident "(" params ")" "->" type extern_effects ";" ;
extern_effects  = "effects" "{" ( effect_name ("," effect_name)* ","? )? "}" ;
```

## Semantics and Rules

- Functions are pure by default when no `effects { ... }` clause is present.
- Known effects are fixed by `KNOWN_EFFECTS`:
  - `io`, `fs`, `net`, `time`, `rand`, `env`, `proc`, `concurrency`
- Frontend normalization canonicalizes effect signatures:
  - unknown effect names are rejected
  - duplicate effect names are rejected
  - accepted effect sets are sorted deterministically
- Frontend normalization canonicalizes capability signatures:
  - unknown capability names are rejected
  - duplicate capability names are rejected
  - accepted capability sets are sorted deterministically
- Direct-call rule: callee declared effects must be a subset of caller declared effects.
- Transitive rule: call-graph closure is analyzed; if caller can reach an effect transitively, that effect must be declared on the caller.
- Capability authority rule: caller must declare matching `capabilities { ... }` for declared and transitive effects.
- Contract purity rule: `requires`, `ensures`, and struct `invariant` expressions are pure contexts; effectful calls inside them are rejected.
- Async does not bypass effect accounting:
  - calling `async fn` still contributes the callee effect set
  - `await` does not erase effect obligations
- Resource protocol checks detect invalid use-after-close/double-close patterns for selected handles:
  - `Sender[T]`: `send`, `try_send`, terminal `close_sender`
  - `Receiver[T]`: `recv`, `try_recv`, `recv_timeout`, terminal `close_receiver`
  - `IntChannel`: `send_int`, `recv_int`, terminal `close_channel`
  - `IntMutex`: `lock_int`, `unlock_int`, terminal `close_mutex`
  - `Task`: terminal `join_task` or `cancel_task`
  - `FileHandle`: `file_read_line`, `file_write_str`, terminal `file_close`
  - TCP/UDP/process `Int` handles: terminal lifecycle via `tcp_close`, `udp_close`, `wait`, `kill`
  - async net operation handles: terminal wait via `async_wait_int` / `async_wait_string`
- Diagnostics are stable and machine-readable (`E2001`..`E2010` for effect/capability/protocol checks).

## Foreign (`extern`) functions

The checker cannot see inside C code, so an `extern` declaration must state its effects. Callers then inherit them through the normal rules above. This closes the hole where a pure function could print through `puts` inside `unsafe { ... }` (VERIFY-T7, #488).

- Every `extern "C" fn` declaration needs an `effects { ... }` clause. A declaration without one reports `E2010`.
- `effects { }` (empty) declares a pure C function, such as `abs`. It is a trusted claim, like the effects on an `intrinsic fn`: the compiler cannot verify it, so only use it for functions with no observable side effects.
- Extern effects use the same names as other functions (`E2003` for an unknown name, `E2004` for a duplicate).
- `capabilities`, `requires` and `ensures` stay off extern declarations (`E1066`). Authority belongs to the callers: the direct-call rule (`E2001`), the transitive rule (`E2005`), the capability rule (`E2009`) and contract purity (`E2002`) apply to an extern call exactly as to any other call.
- `unsafe` still marks the call boundary (`E2122`); it does not waive effects.

Design note: the alternative was one implicit `ffi` effect on every extern call. Declared per-function effects were chosen because they reuse the existing effect and capability names, let a pure C helper stay callable from pure code, and tell callers what the foreign code actually does (`io`, `fs`, `net`, ...) instead of one opaque bucket.

```aic
extern "C" fn putchar(c: Int32) -> Int32 effects { io };
extern "C" fn abs(x: Int32) -> Int32 effects { };

fn magnitude(x: Int32) -> Int32 {
    unsafe { abs(x) }              // pure: allowed in a pure function
}

fn newline() -> Int32 effects { io } capabilities { io } {
    unsafe { putchar(10i32) }      // without `effects { io }` this reports E2001
}
```

Runnable example: `examples/core/extern_effects.aic`.
