# Bytes Workflow (Binary FS + Network APIs)

This workflow demonstrates first-class `std.bytes` payloads and compile-intent usage of binary signatures in `std.fs` and `std.net`.

## APIs Used

- `empty() -> Bytes`
- `from_string(s: String) -> Bytes`
- `byte_len(data: Bytes) -> Int`
- `is_empty(data: Bytes) -> Bool`
- `concat(left: Bytes, right: Bytes) -> Bytes`
- `to_string(data: Bytes) -> Result[String, BytesError]`
- `to_string_lossy(data: Bytes) -> String`
- `is_valid_utf8(data: Bytes) -> Bool`
- `std.fs.write_bytes(path: String, content: Bytes) -> Result[Bool, FsError] effects { fs }`
- `std.net.tcp_send(handle: Int, payload: Bytes) -> Result[Int, NetError] effects { net }`
- `std.net.UdpPacket { from: String, payload: Bytes }`

## Runnable Example

- `examples/data/bytes_api_roundtrip.aic`

Run:

```bash
cargo run --quiet --bin aic -- run examples/data/bytes_api_roundtrip.aic
```

Expected output: `42`.
