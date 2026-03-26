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
    vec.push(v, 2)
}

fn main() -> Int effects { io } capabilities { io } {
    let mut v: Vec[Int] = vec.new_vec();
    v = vec.push(v, 1);
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
