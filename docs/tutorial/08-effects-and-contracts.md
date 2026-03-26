# 08 Effects and Contracts

## Goal
Declare side effects explicitly and enforce behavior with contracts.

## Syntax you need
- Effects: `fn f() -> T effects { io, fs } capabilities { io, fs } { ... }`
- Preconditions: `requires <bool-expr>`
- Postconditions: `ensures <bool-expr>` (`result` is available in `ensures`)
- Struct invariants: `struct X { ... } invariant <bool-expr>`

## Runnable snippet (contracts)
```aic
module examples.e4.verified_abs;
import std.io;

fn abs(x: Int) -> Int ensures result >= 0 {
    if x >= 0 { x } else { 0 - x }
}

fn main() -> Int effects { io } capabilities { io } {
    print_int(abs(-7));
    0
}
```

Run it:
```bash
aic run examples/e4/verified_abs.aic
```

Expected output:
```text
7
```

## Runnable snippet (effect declarations)
```aic
module examples.e4.effect_decl;
import std.io;

fn noisy() -> () effects { time, io, fs } capabilities { time, io, fs } {
    print_int(1);
    ()
}

fn main() -> Int effects { fs, io, time } capabilities { fs, io, time } {
    noisy();
    0
}
```

Run it:
```bash
aic run examples/e4/effect_decl.aic
```

Expected output:
```text
1
```

## What to remember
- Functions are pure by default.
- The caller must declare all effects required by callees, including transitive paths.
- The caller must also carry the matching capabilities.
- Contracts are checked statically when possible and enforced at runtime when obligations remain.
