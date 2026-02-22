# 05 Structs and Enums

## Goal
Model data with `struct` and `enum`, then destructure with pattern matching.

## Syntax you need
- `struct Name { field: Type, ... }` defines a record type.
- `enum Name { Variant, Variant(Type), ... }` defines tagged variants.
- Construct structs with `Name { field: value }`.
- Construct enum payload variants like `Full(value)`.
- Use `match` to handle each variant.

## Runnable snippet
```aic
import std.io;

struct Pair {
    left: Int,
    right: Int,
}

enum Wrap[T] {
    Empty,
    Full(T),
}

fn fold(x: Wrap[Pair]) -> Int {
    match x {
        Empty => 0,
        Full(p) => p.left + p.right,
    }
}

fn main() -> Int effects { io } {
    print_int(fold(Full(Pair { left: 20, right: 22 })));
    0
}
```

## Run it
```bash
aic run examples/e5/enum_match.aic
```

Expected output:
```text
42
```

## What to remember
- Field access uses dot syntax (`p.left`).
- Patterns can bind payload values (`Full(p)`).
