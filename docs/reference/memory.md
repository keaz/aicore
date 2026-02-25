# Memory and Mutability Reference

See also: [Statements](./statements.md), [Expressions](./expressions.md), [Types](./types.md)

This page documents the current mutability and borrow discipline implemented by type checking.

## Grammar

```ebnf
let_stmt        = "let" "mut"? ident (":" type)? "=" expr ";" ;
assign_stmt     = ident "=" expr ";" ;
borrow_expr     = "&" "mut"? unary_expr ;
```

## Semantics and Rules

- Bindings are immutable by default.
- Reassignment requires `let mut` at declaration time.
- Borrow expressions create reference wrappers:
  - immutable borrow: `Ref[T]`
  - mutable borrow: `RefMut[T]`
- Borrow target must be a local variable binding name.
- Borrow conflict checks:
  - cannot take `&mut x` if `x` has any active borrow
  - cannot take `&x` while a mutable borrow of `x` is active
- Mutation safety checks:
  - cannot assign to immutable binding
  - cannot assign to binding while any active borrow of that binding exists
- Direct borrow stored in a local (`let r = &x;`) is tracked as a persistent lexical borrow for that binding scope.
- Temporary borrows in expression position (for example call arguments) are released after expression evaluation.
- Borrow checking is block-structured and conservative across control flow:
  - branch bodies are checked in isolated cloned borrow states
  - loop bodies are checked under loop-local borrow state snapshots
- Assignment remains type-checked in addition to mutability/borrow checks.
- Current model focuses on alias/mutability safety for local bindings; explicit lifetime syntax is not part of the surface language.
- Runtime-drop local values (`String`, struct, enum) are cleaned up in deterministic reverse lexical order at scope exits. Codegen emits `llvm.lifetime.end.p0i8` for this ordering across normal exits and early control-flow exits (`return`, `break`, `continue`, and `?` error propagation returns).
- `#157` current shipped subset adds real RAII cleanup for handle-backed resource locals:
  - `FileHandle` => `aic_rt_fs_file_close`
  - `IntChannel` => `aic_rt_conc_close_channel`
  - `IntMutex` => `aic_rt_conc_mutex_close`
- Direct local move-outs (`let b = a`, direct `return a`, and tail `a` for supported handle-backed resource locals) skip source-local cleanup so ownership transfer does not auto-close the moved value.
- AI-friendly quick pattern for current #157 behavior:

```aic
import std.fs;

fn open_for_append(path: String) -> Result[FileHandle, FsError] effects { fs } {
    let file = open_append(path)?;
    Ok(file) // move-out keeps returned handle alive
}

fn append_once(path: String, text: String) -> Result[Int, FsError] effects { fs } {
    let file = open_append(path)?; // auto-close on all exits below
    file_write_str(file, text)?;   // ownership consumed by call
    read_text("")?;                // forces early return, file still cleaned up
    Ok(1)
}
```
- Remaining `#157` gaps: user-defined destructor hooks (`Drop`-style behavior), full move-out tracking for complex expressions/control-flow joins, partial-move destructor semantics, and panic/unwind drop paths.

## Diagnostic mapping

- `E1263`: conflicting mutable borrow
- `E1264`: immutable borrow while mutable borrow is active
- `E1265`: assignment while borrowed
- `E1266`: assignment to immutable binding
- `E1267`: mutable borrow of immutable binding
- `E1268`: borrow target is not a local binding
- `E1269`: assignment type mismatch
