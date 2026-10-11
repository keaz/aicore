# Option and Result

`Option[T]` (`std.option`) and `Result[T, E]` (`std.result`) are enums with inherent methods. All methods below are pure and never panic.

## Option[T]

| Method | Result |
|---|---|
| `is_some()` / `is_none()` | `Bool` |
| `unwrap_or(fallback)` | the `Some` value, or `fallback` |
| `unwrap_or_else(f: Fn() -> T)` | the `Some` value, or `f()` |
| `map(f: Fn(T) -> U)` | `Option[U]` |
| `map_or(fallback, f: Fn(T) -> U)` | `f(value)`, or `fallback` |
| `and_then(f: Fn(T) -> Option[U])` | `Option[U]` |
| `or(other)` / `or_else(f: Fn() -> Option[T])` | `self` when `Some`, otherwise `other` / `f()` |
| `filter(p: Fn(T) -> Bool)` | `self` when `Some` and `p(value)`, otherwise `None` |
| `zip(other: Option[U])` | `Option[(T, U)]`, `Some` only when both are |
| `ok_or(err)` / `ok_or_else(f: Fn() -> E)` | `Result[T, E]` |

## Result[T, E]

| Method | Result |
|---|---|
| `is_ok()` / `is_err()` | `Bool` |
| `ok()` / `err()` | `Option[T]` / `Option[E]` |
| `unwrap_or(fallback)` | the `Ok` value, or `fallback` |
| `unwrap_or_else(f: Fn(E) -> T)` | the `Ok` value, or `f(err)` |
| `map(f: Fn(T) -> U)` | `Result[U, E]` |
| `map_err(f: Fn(E) -> F)` | `Result[T, F]` |
| `map_or(fallback, f: Fn(T) -> U)` | `f(value)`, or `fallback` |
| `and_then(f: Fn(T) -> Result[U, E])` | `Result[U, E]` |
| `or_else(f: Fn(E) -> Result[T, F])` | `self` when `Ok`, otherwise `f(err)` |

## The `?` operator

- `result?` on `Result[T, E]` has type `T`; on `Err(e)` it returns `Err(e)` from the enclosing function or closure, whose return type must be `Result[_, E]` with the same error type (`E1261`, `E1262`).
- `option?` on `Option[T]` has type `T`; on `None` it returns `None` from the enclosing function or closure, whose return type must be `Option[_]` (`E1261`). To use an `Option` in a function that returns `Result`, convert it first: `value.ok_or(err)?`.

```aic
fn add(a: Option[Int], b: Option[Int]) -> Option[Int] {
    let x = a?;
    let y = b?;
    Some(x + y)
}
```

## No panicking `unwrap` / `expect` (yet)

`unwrap()` and `expect(msg)` are intentionally not provided. A panic is an effect that callers should see in signatures, and AICore has no `panic` effect yet. They will be added together with the `panic` effect (roadmap item VERIFY-T2, #487), gated behind it. Until then, use `match`, `if let`, `let ... else`, `?`, or the `unwrap_or*` / `map_or` combinators.
