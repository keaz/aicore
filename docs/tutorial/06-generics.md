# 06 Generics

## Goal
Write reusable types and functions with type parameters.

## Syntax you need
- Declare generic parameters with brackets: `struct Pair[T, U]`.
- Generic functions use the same bracket form: `fn id[T](x: T) -> T`.
- Instantiate generic types with concrete arguments: `Pair[Int, Bool]`.
- Type arguments are often inferred from call arguments.
- Trait-bounded generics use `T: TraitName` when constraints are required.

## Runnable snippet
```aic
import std.io;

struct Pair[T, U] {
    left: T,
    right: U,
}

fn pair_first[T, U](x: Pair[T, U]) -> T {
    x.left
}

fn main() -> Int effects { io } {
    let pair = Pair { left: 42, right: true };
    print_int(pair_first(pair));
    0
}
```

## Run it
```bash
aic run examples/e5/generic_pair.aic
```

Expected output:
```text
42
```

## What to remember
- Generic arity must match declarations (`Pair[T, U]` needs two type arguments).
- Generic substitution is enforced for function calls, struct construction, and field access.
