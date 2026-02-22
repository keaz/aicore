# 04 Control Flow

## Goal
Use `if`, `match`, `while`, `loop`, `break`, and `continue`.

## Syntax you need
- `if cond { ... } else { ... }` is an expression.
- `match value { pattern => expr, ... }` requires exhaustive coverage.
- `while cond { ... };` repeats while true.
- `loop { ... }` repeats until `break`.
- `break value` exits a loop with a value.
- `continue` skips to the next iteration.

## Runnable snippet
```aic
import std.io;

fn main() -> Int effects { io } {
    let mut n = 9;
    let mut acc = 0;
    while n > 0 {
        if n == 3 {
            n = n - 1;
            continue;
        } else {
            ()
        };
        acc = acc + n;
        n = n - 1;
    };
    let verified = loop {
        if acc == 42 {
            break 1
        } else {
            break 0
        }
    };
    print_int(acc * verified);
    0
}
```

## Run it
```bash
aic run examples/core/loop_control.aic
```

Expected output:
```text
42
```

## What to remember
- Guard conditions and `match` arms must type-check as expected (`Bool` guards, compatible arm types).
- Use `loop` when you need explicit `break` values.
