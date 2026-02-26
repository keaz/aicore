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
- `#157` current shipped subset adds real RAII cleanup for compiler-managed resource locals:
  - `FileHandle` => `aic_rt_fs_file_close`
  - `Map[K, V]` => `aic_rt_map_close`
  - `Set[T]` => closes inner `Map[T, Int]` via `aic_rt_map_close`
  - `TcpReader` => `aic_rt_net_tcp_close`
  - `IntChannel` => `aic_rt_conc_close_channel`
  - `IntMutex` => `aic_rt_conc_mutex_close`
- Language-level `Drop` trait methods are dispatched at scope exits before builtin resource cleanup fallback:
  - declare `trait Drop[T] { fn drop(self: T) -> (); }`
  - implement `impl Drop[MyType] { fn drop(self: MyType) -> () { ... } }`
  - locals with a concrete `Drop` impl run the trait method in deterministic reverse lexical order
  - moved-out locals suppress destructor calls on the moved-from slot
- Direct local move-outs (`let b = a`, direct `return a`, and tail `a` for supported resource locals) skip source-local cleanup so ownership transfer does not auto-close the moved value.
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
- `Drop` example with runtime-visible cleanup ordering:

```aic
import std.fs;

trait Drop[T] {
    fn drop(self: T) -> () effects { fs };
}

struct AuditDrop { path: String, marker: String }

impl Drop[AuditDrop] {
    fn drop(self: AuditDrop) -> () effects { fs } {
        let _ = append_text(self.path, self.marker);
        ()
    }
}
```

- Remaining `#157` gaps: full move-out tracking for complex expressions/control-flow joins, partial-move destructor semantics, and panic/unwind drop paths.

## Debug Leak Detection

- Leak tracking is opt-in via `aic run --check-leaks <input>`.
- Normal mode (`aic run` without `--check-leaks`) keeps tracking disabled.
- On leak-free runs, exit behavior is unchanged.
- On detected leaks, runtime exits non-zero and emits a structured stderr record:

```json
{"code":"memory_leak_detected","count":1,"bytes":48,"first_allocation":{"site":"generated-llvm","line":0}}
```

- `count`: number of live allocations at process exit.
- `bytes`: sum of leaked byte sizes.
- `first_allocation`: first live allocation site by allocation order.
- Site precision:
  - Runtime C allocations report `site`/`line` from runtime source locations.
  - LLVM-generated heap allocations (for example closure-capture heap env allocations) report `site` as `generated-llvm` with `line: 0` by design.

- ASan path:
  - `aic run --asan <input>` or `AIC_RUN_ASAN=1 aic run <input>`
  - `AIC_ASAN=1` for direct build/codegen compile paths.

## Vec Capacity APIs

`std.vec` now exposes explicit capacity-management APIs:

- `new_vec_with_capacity[T](capacity: Int) -> Vec[T]`
- `reserve[T](v: Vec[T], additional: Int) -> Vec[T]`
- `shrink_to_fit[T](v: Vec[T]) -> Vec[T]`

Runtime behavior contract:

- `new_vec_with_capacity` pre-allocates backing storage up to `capacity`.
- `reserve` ensures capacity for `len(v) + additional` without changing length.
- `shrink_to_fit` reduces capacity to current length (`cap == len`), preserving values.
- Growth factor remains 2x when capacity must grow.

AI-friendly pattern:

```aic
import std.vec;

fn demo() -> Int {
    let mut v: Vec[Int] = vec.new_vec_with_capacity(8);
    v = vec.reserve(v, 16);   // plan upcoming pushes
    v = vec.push(v, 1);
    v = vec.push(v, 2);
    v = vec.shrink_to_fit(v); // cap now equals len
    vec.vec_cap(v)
}
```

## Diagnostic mapping

- `E1263`: conflicting mutable borrow
- `E1264`: immutable borrow while mutable borrow is active
- `E1265`: assignment while borrowed
- `E1266`: assignment to immutable binding
- `E1267`: mutable borrow of immutable binding
- `E1268`: borrow target is not a local binding
- `E1269`: assignment type mismatch
