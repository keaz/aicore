# 01 Hello World

## Goal
Write and run a minimal AIC program, and understand the entry point shape.

## Syntax you need
- `import std.io;` brings IO APIs like `print_int` into scope.
- `fn main() -> Int effects { io } { ... }` is the executable entry function.
- Statements end with `;`.
- The last expression in a block is the return value when it has no trailing `;`.

## Runnable snippet
```aic
import std.io;

fn main() -> Int effects { io } {
    print_int(42);
    0
}
```

## Run it
```bash
aic run examples/e5/hello_int.aic
```

Expected output:
```text
42
```

## What to remember
- AIC functions are pure by default.
- Any call to `std.io` requires `effects { io }` on the caller.
