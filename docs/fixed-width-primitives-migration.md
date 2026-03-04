# Fixed-Width Primitives Migration Guide

This guide covers practical migration from `Int`-based integer usage to fixed-width primitives:

- `Int8`, `Int16`, `Int32`, `Int64`, `Int128`
- `UInt8`, `UInt16`, `UInt32`, `UInt64`, `UInt128`
- `ISize`, `USize` (`UInt` alias)

`Int` remains available as the general integer type (signed 64-bit range in current backend/runtime).

## Migration rules

1. Use fixed-width API contracts where width/sign matters (wire formats, byte-oriented APIs, protocol fields).
2. Keep positions/offsets/count-style indices as `Int` unless an API explicitly requires a fixed-width type.
3. Prefer literal suffixes (`1u8`, `42i16`, `7u32`) at protocol boundaries for readability and deterministic typing.
4. Expect only lossless implicit conversions; narrowing/sign-changing assignments are rejected.

## Wave 1 migration contract (`#317`, `#320`)

- Current behavior:
  - Fixed-width families are available up to 64-bit (`Int64`/`UInt64`).
  - No dedicated `std.numeric` helper module is documented.
- Target behavior:
  - Add `Int128`/`UInt128` for explicit 128-bit storage and arithmetic boundaries.
  - Keep `Int` unchanged as the general signed 64-bit integer.
  - Introduce `std.numeric` for explicit numeric-boundary operations where source code must state overflow/conversion policy.

Migration guidance for Wave 1:

1. Use `Int128`/`UInt128` only at boundaries that require >64-bit range (ledger ids, hash-partition counters, high-range protocol values).
2. Keep existing `Int`/`Int64` signatures where 64-bit range is sufficient to avoid unnecessary ABI churn.
3. Move risky conversion points to explicit `std.numeric` helper calls rather than relying on implicit boundary checks.
4. Keep deterministic policy at call sites:
   - checked conversion path for fallible narrowing/sign-change
   - saturating path where upper/lower clamp is business-correct
   - wrapping path only where modular arithmetic is intentionally required

Canonical Wave 1 examples for CI wiring:

- `examples/core/int128_uint128.aic` (primitive/literal/operator coverage for `Int128` + `UInt128`)
- `examples/data/std_numeric.aic` (`std.numeric` conversion/overflow policy walkthrough)

## Wave 2A size-family contract (`#318`)

- `ISize` and `USize` are deterministic 64-bit integer families in type-checking and codegen lowering.
- `UInt` is a source-level alias of `USize`.
- Implicit conversions remain lossless-only:
  - `UInt32 -> USize` is allowed.
  - `USize -> Int` is rejected.
  - `Int -> ISize` is allowed (same signed 64-bit range).
- Operator rules remain exact-kind:
  - `USize` and `UInt` are treated as the same kind.
  - `ISize` and `Int` remain distinct operator kinds.

Canonical Wave 2A examples for CI wiring:

- `examples/core/isize_usize_uint.aic` (size-family primitive and alias behavior)
- `examples/core/isize_usize_conversions.aic` (lossless vs rejected conversion boundaries)

## Buffer API migration (`std.buffer`)

Before (legacy `Int` payload assumptions):

```aic
import std.buffer;

fn read_len_or(v: Result[Int, BufferError], fallback: Int) -> Int {
    match v {
        Ok(x) => x,
        Err(_) => fallback,
    }
}

fn main() -> Int {
    let buf = new_buffer(16);
    let _ = buf_write_u32_be(buf, 512);
    buf_reset(buf);
    read_len_or(buf_read_u32_be(buf), -1)
}
```

After (typed payloads, offsets still `Int`):

```aic
import std.buffer;

fn read_len_or(v: Result[UInt32, BufferError], fallback: Int) -> Int {
    match v {
        Ok(x) => x,
        Err(_) => fallback,
    }
}

fn main() -> Int {
    let buf = new_buffer(16);
    let _ = buf_write_u32_be(buf, 512u32);
    let _ = buf_patch_u32_be(buf, 0, 1024u32); // offset remains Int
    buf_reset(buf);
    read_len_or(buf_read_u32_be(buf), -1)
}
```

## Bytes API migration (`std.bytes`)

Before:

```aic
import std.bytes;
import std.vec;

let mut raw: Vec[Int] = vec.new_vec();
raw = vec.push(raw, 65);
raw = vec.push(raw, 66);
let payload = bytes.from_byte_values(raw);
```

After:

```aic
import std.bytes;
import std.vec;

let mut raw: Vec[UInt8] = vec.new_vec();
raw = vec.push(raw, 65u8);
raw = vec.push(raw, 66u8);
let payload = bytes.from_byte_values(raw);
```

Also updated:

- `bytes.byte_at(data, index)` returns `Result[UInt8, BytesError]`
- `bytes.to_byte_values(data)` returns `Vec[UInt8]`
- `bytes.find_byte(data, value)` takes `UInt8`

## Protocol/process example

Scenario: process emits a one-byte status code in stdout.

Before:

```aic
import std.proc;
import std.bytes;
import std.string;
import std.vec;

fn status_code(out: ProcOutput) -> Int {
    let payload = string_to_bytes(out.stdout);
    let values: Vec[Int] = bytes.to_byte_values(payload);
    match vec.get(values, 0) {
        Some(v) => v,
        None => -1,
    }
}
```

After:

```aic
import std.proc;
import std.bytes;
import std.string;

fn status_code(out: ProcOutput) -> Int {
    let payload = string_to_bytes(out.stdout);
    match bytes.byte_at(payload, 0) {
        Ok(code) => if code == 1u8 { 1 } else { 0 },
        Err(_) => -1,
    }
}
```

## Diagnostic expectations during migration

- `E1204`: assignment/argument/return mismatch, including out-of-range expression literals.
- `E1230`: arithmetic/bitwise/shift integer kind mismatch.
- `E1231`: equality integer kind mismatch.
- `E1232`: comparison integer kind mismatch.
- `E1234`: out-of-range integer literal in pattern context.
