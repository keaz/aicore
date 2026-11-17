# Integer Arithmetic and Overflow

See also: [Expressions](./expressions.md), [Types](./types.md), [Contracts](./contracts.md)

Integer arithmetic in AICore never wraps silently. An operation whose exact
result does not fit its type stops the program with a structured panic, unless
the code asks for another overflow policy with a method.

## Checked operators (the default)

`+`, `-`, `*` and unary `-` on every integer type (`Int`, `Int8`..`Int128`,
`ISize`, `UInt8`..`UInt128`, `USize`/`UInt`) trap when the result is outside the
type's range. The compound forms `+=`, `-=` and `*=` follow the same rule.

| Expression | Result |
| --- | --- |
| `9223372036854775807 + x` with `x: Int = 1` | panic `integer_overflow{op:add,type:Int}: Int addition overflows` |
| `a - b` with `a: UInt8 = 0`, `b: UInt8 = 1` | panic `integer_overflow{op:sub,type:UInt8}: UInt8 subtraction overflows` |
| `m * 2` with `m: Int32 = 2147483647` | panic `integer_overflow{op:mul,type:Int32}: Int32 multiplication overflows` |
| `-m` with `m: Int8 = -128` | panic `integer_overflow{op:neg,type:Int8}: Int8 negation overflows` |
| `n += 1` with `n: UInt16 = 65535` | panic `integer_overflow{op:add,type:UInt16}: UInt16 addition overflows` |

The panic message has the form
`integer_overflow{op:<add|sub|mul|neg>,type:<T>}: <T> <addition|subtraction|multiplication|negation> overflows`.
It is printed as `AICore panic at <line>:<column>: <message>` on standard error,
and the process exits with a non-zero status. Division follows the same model:
`MIN / -1` panics with `integer_overflow{op:div,type:<T>}`, and a zero divisor
panics with `division_by_zero` (see [Expressions](./expressions.md)).

A negated integer literal is a constant, not a negation at run time: `-128i8`
is `Int8`'s minimum and does not trap, although `128i8` alone does not fit.

When every operand of `+`, `-`, `*` or `/` is an integer literal, `aic check`
evaluates the operation, and a result that does not fit is the compile-time
error `E1320` (`let b: UInt8 = 200 + 100;`). The evaluation is exact on every
width, so a `UInt128` product above `Int128`'s maximum that fits `UInt128`
(range `0..=340282366920938463463374607431768211455`) is accepted. A `const`
initializer or struct field default that overflows is `E1317` instead.

## Explicit overflow policies

Each integer type has methods that name what happens on overflow. The operand
has the receiver's type (an unsuffixed literal takes it: `x.wrapping_add(1)`).

| Method | Result type | On overflow |
| --- | --- | --- |
| `x.wrapping_add(y)`, `wrapping_sub`, `wrapping_mul` | `T` | keeps the low bits (two's complement) |
| `x.wrapping_neg()` | `T` | `0 - x` modulo 2^bits (also on unsigned types) |
| `x.checked_add(y)`, `checked_sub`, `checked_mul` | `Option[T]` | `None` |
| `x.checked_div(y)`, `x.checked_rem(y)` | `Option[T]` | `None` for a zero divisor and for signed `MIN / -1` (also `MIN % -1`) |
| `x.checked_neg()` | `Option[T]` | `None` (on unsigned types, `None` unless `x` is `0`) |
| `x.saturating_add(y)`, `saturating_sub`, `saturating_mul` | `T` | clamps to `T`'s minimum or maximum |
| `x.saturating_neg()` | `T` | clamps (signed types only) |

```aic
fn fnv1a_step(hash: UInt32, byte: UInt32) -> UInt32 {
    (hash ^ byte).wrapping_mul(16777619u32)
}

fn add_quantity(stock: UInt16, delivered: UInt16) -> Option[UInt16] {
    stock.checked_add(delivered)
}
```

The names match the conversion methods (`checked_to_u8`, `saturating_to_i32`,
`wrapping_to_u16`). Misuse is `E1321`: a float receiver (float arithmetic does
not overflow), an operand of another integer type, `wrapping_div` /
`wrapping_rem` / `saturating_div` / `saturating_rem` (use `checked_div` /
`checked_rem` or the operator), or `saturating_neg` on an unsigned type. A wrong
number of arguments is `E1213`.

## Checks the compiler removes

The compiler leaves out an overflow check when interval arithmetic proves that
the exact result fits the type. It knows:

- integer literals;
- the range of each local's declared type (`UInt8` is `0..=255`);
- the range of an immutable `let` from its initializer (`let w = byte as Int` is
  `0..=255`);
- the bounds a function's `requires` clause puts on a parameter
  (`requires n >= 0 && n <= 1000`), when the parameter is never rebound or
  assigned in the body;
- `%` and `/` by a literal, `&` with a non-negative operand, `>>` / `>>>` of a
  non-negative value by a literal, lossless `as` casts, and nested `+ - *`.

```aic
fn area(w: Int, h: Int) -> Int requires w >= 0 && w <= 100000 && h >= 0 && h <= 100000 {
    w * h // no overflow check: the product is at most 10^10
}
```

128-bit operations always keep their check. In optimized builds (`-O1` and
above) LLVM removes further checks that its own range analysis proves.

## Cost

A check is one `llvm.{s,u}{add,sub,mul}.with.overflow` intrinsic and a branch
to a cold panic block. The REST runtime soak gate
(`scripts/ci/rest-runtime-soak-gate.py`, budgets in
`benchmarks/service_baseline/rest-runtime-soak-gate.v1.json`) runs with
checked arithmetic and must stay within its existing budgets; overflow checks
have no separate budget.

## Object and static library artifacts

An overflow check (like a division or bounds check) panics through the runtime
function `aic_rt_panic`. `aic build --artifact obj` emits the module object
without the runtime, and the runtime object inside an `--artifact lib` archive
pulls in the whole runtime (TLS, sockets and their libraries). So that a C
program can link either artifact on its own, the compiler gives the module a
weak fallback `aic_rt_panic` whenever the module can panic. The fallback prints
the same `AICore panic at <line>:<column>: <message>` line to standard error
with `write` (`_write` on Windows) and calls `exit(1)`:

```c
long aic_add(long x, long y);   /* fn add(x: Int, y: Int) -> Int { x + y } */
aic_add(INT64_MAX, 1);
/* stderr: AICore panic at <line>:<column>: integer_overflow{op:add,type:Int}: Int addition overflows
   exit status: 1 */
```

When the full runtime is linked as well, its strong `aic_rt_panic` replaces
the fallback, which adds the `AIC_BACKTRACE` stack trace. Executables
(`--artifact exe`) always link the runtime and have no fallback.

## Shifts and bitwise operators

`<<`, `>>`, `>>>`, `&`, `|`, `^` and `~` are not arithmetic and do not trap.
`<<` discards the bits shifted past the type's width (`1 << 63` is `Int`'s
minimum). Keep shift amounts in `0..bits`; the result of a negative shift
amount, or one of at least the bit width, is not defined (a constant shift
outside that range is `E1317` in a `const`).
