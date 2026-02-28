# Bytes data API

`std.bytes` provides binary-safe payload handling for filesystem and networking APIs.

## Core type

`Bytes` is the standard payload container:

```aic
struct Bytes {
  data: String,
}
```

Use constructors/helpers from `std.bytes`:

- `empty() -> Bytes`
- `from_string(s: String) -> Bytes`
- `byte_len(data: Bytes) -> Int`
- `byte_at(data: Bytes, index: Int) -> Result[Int, BytesError]`
- `byte_slice(data: Bytes, start: Int, end: Int) -> Result[Bytes, BytesError]`
- `find_byte(data: Bytes, value: Int) -> Option[Int]`
- `starts_with(data: Bytes, prefix: Bytes) -> Bool`
- `ends_with(data: Bytes, suffix: Bytes) -> Bool`
- `compare_bytes(a: Bytes, b: Bytes) -> Int`
- `from_byte_values(values: Vec[Int]) -> Result[Bytes, BytesError]`
- `to_byte_values(data: Bytes) -> Vec[Int]`
- `concat(left: Bytes, right: Bytes) -> Bytes`
- `is_valid_utf8(data: Bytes) -> Bool`
- `to_string(data: Bytes) -> Result[String, BytesError]`
- `to_string_lossy(data: Bytes) -> String`

`to_string` preserves strict UTF-8 semantics (`InvalidUtf8` on invalid sequences).
`to_string_lossy` always returns text and replaces invalid sequences.

Indexing/slicing semantics:
- `byte_at` returns `Err(InvalidInput)` for out-of-bounds indexes.
- `byte_slice` uses half-open ranges `[start, end)` and returns `Err(InvalidInput)` for invalid ranges.
- `from_byte_values` validates every value is in `[0, 255]`.
- `compare_bytes` is lexicographic (`-1`, `0`, `1`).

## Filesystem byte APIs

`std.fs` byte APIs remain `Bytes`-typed at the public boundary:

- `read_bytes(path: String) -> Result[Bytes, FsError]`
- `write_bytes(path: String, content: Bytes) -> Result[Bool, FsError]`
- `append_bytes(path: String, content: Bytes) -> Result[Bool, FsError]`

These functions convert between `Bytes` and runtime string payload intrinsics internally, so call sites stay binary-oriented while runtime dispatch remains intrinsic-backed.

## Networking byte APIs

`std.net` byte payload APIs:

- `tcp_send(handle: Int, payload: Bytes) -> Result[Int, NetError]`
- `tcp_recv(handle: Int, max_bytes: Int, timeout_ms: Int) -> Result[Bytes, NetError]`
- `udp_send_to(handle: Int, addr: String, payload: Bytes) -> Result[Int, NetError]`
- `async_tcp_send_submit(handle: Int, payload: Bytes) -> Result[AsyncIntOp, NetError]`
- `async_wait_string(op: AsyncStringOp, timeout_ms: Int) -> Result[Bytes, NetError]`

Like fs byte APIs, these wrappers keep `Bytes` in stdlib-facing signatures and bridge to intrinsic string payload contracts internally.

## Examples

- `/Users/kasunranasinghe/Projects/Rust/aicore/examples/data/bytes_api_roundtrip.aic`
- `/Users/kasunranasinghe/Projects/Rust/aicore/examples/data/net_bytes_pipeline.aic`
- `/Users/kasunranasinghe/Projects/Rust/aicore/examples/data/bytes_random_access.aic`
