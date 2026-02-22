# 03 Functions

## Goal
Define pure functions, call async functions, and use `await` correctly.

## Syntax you need
- `fn name(params) -> ReturnType { ... }` defines a normal function.
- `async fn name(...) -> T { ... }` defines an async function.
- Calling an async function returns `Async[T]`; consume it with `await`.
- `await` is only valid inside an `async fn`.

## Runnable snippet
```aic
import std.io;

async fn ping(x: Int) -> Int {
    x + 1
}

async fn main() -> Int effects { io } {
    let value = await ping(41);
    print_int(value);
    0
}
```

## Run it
```bash
aic run examples/core/async_ping.aic
```

Expected output:
```text
42
```

## What to remember
- Return the final expression directly for concise functions.
- Keep effect declarations on async functions the same way as normal functions.
