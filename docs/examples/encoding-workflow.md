# String Encoding Workflow (Wire Protocols)

This workflow demonstrates deterministic UTF-8 validation and lossy decoding for byte-oriented protocol payloads.

## APIs Used

- `string_to_bytes(s: String) -> Bytes`
- `bytes_to_string(data: Bytes) -> Result[String, EncodingError]`
- `bytes_to_string_lossy(data: Bytes) -> String`
- `is_valid_utf8(data: Bytes) -> Bool`
- `byte_length(s: String) -> Int`
- `is_ascii(s: String) -> Bool`

`EncodingError` includes:

- `InvalidSequence`
- `UnsupportedEncoding`
- `BufferTooSmall`

## Runnable Example

- `examples/data/string_encoding.aic`

Run:

```bash
cargo run --quiet --bin aic -- run examples/data/string_encoding.aic
```

Expected output: `42`.
