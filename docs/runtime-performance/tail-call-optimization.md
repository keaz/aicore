# Tail Call Optimization

This runtime/compiler path enables automatic tail-call optimization (TCO) for eligible tail-position function calls.

## What Is Optimized

- Self recursion in tail position (for example `fib_tail(n - 1, b, a + b)` as the branch/function tail expression)
- Mutual recursion in tail position (for example `even` tail-calls `odd`, and `odd` tail-calls `even`)
- No source-level annotation is required

Current lowering strategy emits LLVM `musttail` when the caller/callee signatures match exactly.

## What Is Not Optimized

- Non-tail recursion (for example `1 + recurse(n - 1)`)
- Calls that are not in direct return position
- Signature-mismatched calls

## Agent-Friendly Rules

1. Put recursion in a true tail position (final expression in a branch or explicit `return f(...)`).
2. Keep mutual-recursive functions signature-compatible.
3. Expect normal recursion semantics for non-tail code paths.

## Example

See: `examples/core/tail_call_optimization.aic`

This example validates:

- 1,000,000-step tail-recursive Fibonacci (`fib_tail`) without stack overflow
- 1,000,000-step mutual recursion for `is_even` / `is_odd`
- Deterministic success output `42`

## Verification Commands

```bash
cargo test exec_tail_call_ -- --nocapture
cargo run --quiet --bin aic -- check examples/core/tail_call_optimization.aic
cargo run --quiet --bin aic -- run examples/core/tail_call_optimization.aic
```
