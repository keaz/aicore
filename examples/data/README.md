# Data Examples

This directory contains runnable examples for the data/text runtime surface.
Use these as the canonical entrypoint when checking `std.regex`, `std.json`, `std.url`, `std.http`, `std.time`, and `std.bytes` behavior.

## Index

- Regex and text parsing: `log_parse_regex.aic`, `regex_capture_groups.aic`, `string_unicode_case_trim.aic`
- JSON and config shaping: `config_json.aic`, `serde_models.aic`, `serde_negative_cases.aic`, `ingest_transform_emit.aic`
- URL and HTTP validation: `http_types.aic`, `url_http_negative_cases.aic`
- Timestamp and byte workflows: `audit_timestamps.aic`, `time_utils.aic`, `bytes_api_roundtrip.aic`, `bytes_random_access.aic`, `net_bytes_pipeline.aic`
- Collections and numeric helpers: `vec_algorithms.aic`, `vec_ops.aic`, `deque_workloads.aic`, `set_ops.aic`, `map_headers.aic`
- End-to-end demonstrations: `binary_protocol.aic`, `bitwise_protocol.aic`, `wave5_numeric_end_to_end.aic`, `wave5_migration_buffer_u32.aic`

## Validation

Most examples are runnable with:

```bash
cargo run --quiet --bin aic -- run examples/data/<example>.aic
```

Negative cases are documented inline and should be checked with `aic check` when the example is meant to fail.
