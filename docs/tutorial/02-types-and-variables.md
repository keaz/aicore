# 02 Types and Variables

## Goal
Use built-in types, type annotations, and mutable bindings.

## Syntax you need
- Built-in value types: `Int`, `Float`, `Bool`, `String`, and unit `()`.
- `let name = expr;` declares an immutable variable.
- `let mut name = expr;` declares a mutable variable.
- `let x: Type = expr;` adds an explicit type annotation.

## Runnable snippet
```aic
import std.io;
import std.vec;

fn grow(v: Vec[Int]) -> Vec[Int] {
    let next: Vec[Int] = Vec { ptr: v.ptr, len: v.len + 1, cap: v.cap };
    next
}

fn main() -> Int effects { io } {
    let mut v: Vec[Int] = Vec { ptr: 0, len: 1, cap: 4 };
    v = grow(v);
    print_int(vec_len(v));
    0
}
```

## Run it
```bash
aic run examples/core/mut_vec.aic
```

Expected output:
```text
2
```

## What to remember
- Reassignment needs `let mut`.
- Type inference works for many `let` bindings, but explicit types are useful for clarity.
