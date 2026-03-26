# 07 Error Handling

## Goal
Handle failures with `Result` and `Option`, and propagate errors with `?`.

## Syntax you need
- `Result[T, E]` models success (`Ok`) or failure (`Err`).
- `Option[T]` models presence (`Some`) or absence (`None`).
- `match` is the standard branching tool for both.
- Postfix `?` propagates `Err(e)` and unwraps `Ok(v)`.

## Runnable snippet
```aic
import std.io;

fn parse_positive(x: Int) -> Result[Int, Int] {
    if x >= 0 { Ok(x) } else { Err(0 - x) }
}

fn double_checked(x: Int) -> Result[Int, Int] {
    let n = parse_positive(x)?;
    if true { Ok(n * 2) } else { Err(0) }
}

fn render(v: Result[Int, Int]) -> Int {
    match v {
        Ok(value) => value,
        Err(err) => 0 - err,
    }
}

fn main() -> Int effects { io } capabilities { io } {
    print_int(render(double_checked(21)));
    0
}
```

## Run it
```bash
aic run examples/core/result_propagation.aic
```

Expected output:
```text
42
```

## What to remember
- `?` requires a `Result[...]` value and a compatible `Result[...]` return type in the enclosing function.
- Prefer small conversion functions plus `match` when mapping one error domain into another.
