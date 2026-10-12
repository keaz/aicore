# ADR 0001: Memory model — hybrid reference counting

- Status: accepted (MEM-T1, #469)
- Date: 2026-10-12
- Applies to: the Rust reference compiler (`src/`), the C runtime (`src/codegen/runtime/`), `std/`
- Follow-up work: MEM-T2 (#470), MEM-T3 (#471), MEM-T4 (#472), MEM-T5 (#473), MEM-T6 (#474), TRAIT-T8 (#484)

## Context

AICore has no reclamation strategy for heap data today:

- `String`, `Bytes`, `Vec` buffers, closure environments and the heap parts of structs and enums are allocated by the runtime or by generated code and never freed. `aic run --check-leaks` reports them, but nothing releases them.
- `Vec[T]` already has value semantics (see [Memory reference](../reference/memory.md#value-semantics-of-collections)): copying a `Vec` is O(1) because copies share one buffer, and the runtime never writes a slot that another value can read. The buffer "frontier" lets the newest value append in place.
- Resource handles (`FileHandle`, `Map`, `Set`, sockets, channels, mutexes, `ByteBuffer`) have reference semantics and a single owner: they are closed at the owner's scope exit, moved by `let b = a`, `return a` and tail expressions, and use after a move is `E1277`. A user `Drop` impl runs at a local's scope exit.
- Closures capture by value when they are created; generated code allocates their environments on the heap.
- Cross-thread payloads (`spawn`, channels) are copied today: they are serialized to JSON and decoded on the other side, and `Send` bounds reject handles.
- Panics abort the process.

MEM-T2 (freeing), MEM-T3 (moves), MEM-T4 (references), MEM-T5 (recursive types) and MEM-T6 (drop on panic) all need one decision about who frees heap data and when.

## Options considered

### (a) Ownership with drop glue and enforced moves (Rust-style)

Every value has one owner. Assignment and argument passing move; a value is freed when its owner goes out of scope; copies need an explicit `clone()`. The checker enforces moves through calls, fields and control-flow joins and rejects use after move.

- For: no runtime cost for sharing; frees are deterministic and statically known; the model is well understood.
- Against: it changes the current value semantics. `let a = vec.push(base, 1); let b = vec.push(base, 2);` uses `base` twice and would need `clone()`. Every program that reuses a value after passing it stops compiling. Agents generating code meet move errors far more often, and the "copies are cheap values" contract in the reference docs no longer holds. Closures would need move/borrow capture modes, and async frames would need lifetimes or explicit moves.

### (b) Plain reference counting with copy-on-write

Every heap block carries a count. Copying a value increments it, dropping decrements it, and the block is freed at zero. A mutation updates the block in place when the count is 1 and copies it otherwise.

- For: keeps the current semantics exactly; simple to implement; deterministic frees.
- Against: every copy and every drop touches a count, including the many copies that are immediately followed by the end of the original's life (passing an argument, returning a local, the last use of a binding). This costs time on every call.

### (c) Hybrid: reference counting with compile-time elision (chosen)

Reference counting and copy-on-write as in (b), plus ownership information the compiler already has:

- the last use of a binding transfers its reference instead of copying it, so no increment happens and no decrement is needed at scope exit;
- a function takes ownership of its arguments, so passing a value at its last use costs nothing;
- operations update in place when the value is unique (count 1), which together with last-use transfer makes the common `v = vec.push(v, x)` loop allocation-free;
- resource handles keep their single-owner model with explicit close and move tracking.

This is the approach of Swift's ARC with ownership optimizations and of Koka's Perceus.

## Decision

AICore uses **hybrid reference counting** (option c). The rest of this record defines the semantics and the implementation plan.

## Semantics agents can rely on

These rules are the contract. Today rule 1, rule 5 (for direct moves), the closure capture rule and the concurrency rule already hold; heap data is not freed yet (rule 3), mutation always copies except at the `Vec` frontier (rule 2), and `Drop` runs per local (rule 6). MEM-T2 and MEM-T3 bring the implementation to the contract; until then, programs observe the same values, only leaks and copying costs differ.

### Value categories

| Category | Types | Copy | Freed |
| --- | --- | --- | --- |
| Scalars | integers, floats, `Bool`, `Char`, `()` | bitwise copy | never allocated |
| Shared values | `String`, `Bytes`, `Vec[T]`, closures (`Fn`), `Box[T]` (MEM-T5), and structs, enums, tuples, `Option`, `Result` that contain any of these | O(1): the copy shares the heap data | when the last copy is gone |
| Resource handles | `FileHandle`, `Map`, `Set`, `TcpReader`, sockets, channels, mutexes, `ByteBuffer` | not copied: `let b = a` moves | at the owner's scope exit, or by explicit `close` |

### Rules

1. **A copy behaves as an independent value.** Changing one copy never changes another. This is the current behavior of every shared value and stays true. Copies are cheap because they share storage until one of them is changed.
2. **Mutation is copy-on-write.** An operation that produces a changed value (`vec.push`, `vec.set`, `s + t`, a place assignment `v[i] = x`, `self.n += 1` in a `&mut self` method) writes in place when no other copy can observe the change (the storage is unique). Otherwise it copies first. The existing `Vec` frontier rule remains a special case: appending at the frontier is in place even when the buffer is shared, because no other value reads past its own length.
3. **Freeing is automatic and deterministic.** Heap data is freed exactly when its last copy is gone: at the end of the scope of the last binding that holds it, when it is overwritten by an assignment, when a temporary is no longer needed, or when the containing value is freed. There is no garbage collector and no pause; the order of frees is fixed by the program text (reverse lexical order within a scope, as today for drops).
4. **Last use is a move, invisibly.** When a binding's value is used for the last time, the compiler hands the reference over instead of copying. Programs never see the difference except in speed; there is no "use after move" error for shared values.
5. **Resource handles stay single-owner.** Handles are not reference counted. `let b = a` moves the handle (use of `a` afterwards is `E1277`), passing it to a consuming call moves it, and the owner closes it at scope exit unless it was moved out. MEM-T3 extends move tracking to calls, fields and control-flow joins for handles. A struct that contains a handle is itself single-owner.
6. **`Drop` for user types runs once, for the last copy.** For a struct or enum with a `Drop` impl and no handles, `drop` runs when its last copy is released (rule 3), not at the scope exit of every local that held a copy. Types holding handles keep the single-owner rule, so their `drop` runs at the owner's scope exit, as today.
7. **No cycles.** Shared values are immutable from the outside (rule 1), so they cannot point back at themselves; handles are single-owner; `RefMut` (MEM-T4) points at stack places, never at reference-counted heap data. No cycle collector is needed. A future feature that adds shared mutable cells must add weak references or a cycle collector first.

### Closures

A closure's environment is a shared value: creating the closure copies (increments) each captured value, copying the closure shares the environment, and the environment and its captures are released when the last copy of the closure is gone. Capture stays by value at creation time (see [Expressions](../reference/expressions.md)). Closures cannot capture resource handles by copy; capturing a handle moves it into the closure (MEM-T3 reports later use of the moved handle).

### Async frames

An async function's frame owns the values it captured and the locals live across `await`. Values passed into an async call follow the ownership rule for arguments. The frame is released when the future completes or is dropped, which releases what it holds.

### Concurrency

Reference counts are not atomic. A value never has copies on two threads: values sent through channels or passed to `spawn` are deep-copied at the boundary, so each thread owns independent storage. Today the copy is a JSON round trip; TRAIT-T8 (#484) replaces it with a native deep copy that keeps the same rule. `Send` keeps rejecting resource handles. Sharing a large immutable value between threads without copying would need atomic counts and is out of scope.

### FFI

Strings and byte buffers passed to C functions are borrowed: the pointer is valid for the duration of the call and C must not keep it. Data returned by C is copied into reference-counted storage (the current `out_ptr`/`out_len` runtime convention already copies). Handles passed to C stay owned by AICore.

### Panics

Panics currently abort, so nothing is freed after a panic. MEM-T6 (#474) adds a recovery boundary; it must release every live shared value and close every live handle on the frames it unwinds, in the order rule 3 defines.

## Implementation outline

- **Representation.** Every runtime heap block that a shared value points to gets a header with a 64-bit count (and, for blocks whose contents hold further shared values, a release function). `String`/`Bytes` keep their `{ptr, len, cap}` layout with `ptr` pointing after the header; `Vec` buffers and closure environments get the same header. Structs, enums and tuples stay by-value aggregates: copying one copies (increments) each shared field, and releasing one releases each field. The compiler generates these per-type copy and release routines ("drop glue").
- **Runtime.** `aic_rt_retain(ptr)`, `aic_rt_release(ptr, release_fn)`, `aic_rt_is_unique(ptr)`, and leak counters tied to `--check-leaks`. Vec front-removal views keep a reference to their base buffer; the frontier table forgets a buffer before it is freed.
- **Code generation.** Copies of shared values emit retain, ends of life emit release, and changes of shared storage check uniqueness first. Elision runs before emission: a liveness pass marks last uses; a use that is a last use transfers instead of retaining; a binding whose value was transferred is not released at scope exit.
- **Checker.** No new errors for shared values. Move tracking (`E1277`, `E1278`) applies to resource handles and to types that contain them.

## Migration plan for std and existing code

1. Public std signatures do not change. Functions that return a changed `Vec`, `String` or `Bytes` keep returning a new value; with uniqueness they update in place when the caller passes the last copy, which turns `v = vec.push(v, x)` loops into in-place appends.
2. `Map`, `Set` and the other handles keep their reference semantics and close behavior; their APIs do not change.
3. Runtime functions that copy their inputs defensively (string operations, `vec_set`, `insert`, `remove_at`) switch to "in place if unique, copy otherwise".
4. The current "buffers are never freed" assumption is removed from the Vec runtime: frontier entries and front-removal views are updated when a buffer is released.
5. Generated closure environments and runtime strings get headers; `aic run --check-leaks` must report zero live allocations for the example and test corpus once MEM-T2 lands, and becomes a CI gate.
6. Programs do not change: the semantics above match what they observe today, minus the leaks.

## Consequences for the follow-up issues

- **MEM-T2 (#470)** implements the representation, the runtime functions, retain/release emission and drop glue for String, Vec, closures, structs and enums, and the leak gates. Elision may land later in MEM-T3, but freeing must be correct without it.
- **MEM-T3 (#471)** adds last-use analysis and ownership transfer (the elision above), and enforces single-owner moves for resource handles and types containing them through calls, fields and control-flow joins, including partial moves of handle fields.
- **MEM-T4 (#472)** adds `RefMut` write-through to stack places and flow-sensitive borrows; references never keep heap data alive and never point into reference-counted storage beyond the borrow.
- **MEM-T5 (#473)** adds `Box[T]` as a shared, immutable heap cell, which makes recursive types possible under rule 7.
- **MEM-T6 (#474)** releases shared values and closes handles while unwinding to a recovery boundary.
- **TRAIT-T8 (#484)** replaces the JSON copy at thread boundaries with a native deep copy, keeping the concurrency rule.
- **CORR-T4** already established `Vec` value semantics; this record keeps them.

## References

- Reinking, Xie, de Moura, Leijen: "Perceus: Garbage Free Reference Counting with Reuse", PLDI 2021.
- Swift Automatic Reference Counting and its ownership-based optimizations.
- [Memory reference](../reference/memory.md) for the current behavior of collections, handles, drops and borrows.
